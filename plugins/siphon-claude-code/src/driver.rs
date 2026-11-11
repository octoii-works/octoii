//! Claude through the person's own Claude Code, as a driver of ai's: Claude thinks, the thread acts. The thread's log
//! is the master and Claude Code's sessions its replicas, as Redis's are: a request goes on in the live session that
//! stands where its conversation's last reply ends; else a session is resumed at the longest prefix of the
//! conversation one holds, given what it missed as history between checkpoint tags — in place where that prefix is the
//! session's tip, forked where the session went on past it — else a new session is given the conversation so. A
//! request whose answer is aside from the conversation — none of its tools may be called, or its answer is JSON of a
//! schema — is answered by a fork of its own, kept nowhere. Each provider is an account: the person's own login, or
//! one they added, whose Claude Code keeps its configuration apart.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;
use octoii_ai::catalogue::{Features, InputLimits, Model, ModelReasoning};
use octoii_ai::driver::{Call, Connection, Driver};
use octoii_ai::protocols::anthropic;
use octoii_ai::stream::Source;
use octoii_ai::vocabulary::{Failure, FailureKind, ReasoningEffort, Request, Tool, ToolChoice};
use serde_json::{Value, json};
use tokio::sync::{mpsc, watch};

use crate::account::{self, Account, Asked, Begin, Place, Settling};
use crate::conversation::{self, Told};
use crate::session::{Session, Settled};
use crate::store::Position;

/// The id of the person's own Claude Code's provider.
pub const ID: &str = "claude";

/// Claude Code's accounts, each opened as it is first used.
#[derive(Debug)]
pub struct ClaudeCode {
    place: Arc<Place>,
    accounts: Mutex<HashMap<String, Arc<Account>>>,
    /// The models each account's Claude Code offers, by Claude Code's name for each and the model it stands for.
    rows: Mutex<HashMap<String, Vec<(String, String)>>>,
}

impl ClaudeCode {
    /// Claude Code where `place` says.
    #[must_use]
    pub fn new(place: Place) -> Self {
        Self {
            place: Arc::new(place),
            accounts: Mutex::new(HashMap::new()),
            rows: Mutex::new(HashMap::new()),
        }
    }

    /// The account of the provider `provider`: the person's own for Claude Code's own provider, else one they added,
    /// whose configuration is its own.
    ///
    /// # Errors
    /// Where its files cannot be opened.
    ///
    /// # Panics
    /// Where a holder of the lock panicked.
    pub fn account(&self, provider: &str) -> Result<Arc<Account>, Failure> {
        let mut accounts = self.accounts.lock().expect("no holder of the accounts panics");
        if let Some(account) = accounts.get(provider) {
            return Ok(Arc::clone(account));
        }
        let folder = self.place.folder.join(provider);
        let config = (provider != ID).then(|| folder.join("claude"));
        let account = Account::open(Arc::clone(&self.place), folder, config)
            .map_err(|error| Failure::new(FailureKind::Server, format!("Claude Code's files: {error}")))?;
        let account = Arc::new(account);
        accounts.insert(provider.to_owned(), Arc::clone(&account));
        tokio::spawn(sweep(Arc::downgrade(&account)));
        Ok(account)
    }

    /// Lets go of what the conversation `conversation` holds in each account, as it ended.
    pub fn forget(&self, conversation: &str) {
        let Ok(folders) = std::fs::read_dir(&self.place.folder) else {
            return;
        };
        for folder in folders.flatten() {
            let Some(provider) = folder.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if provider == "work" || !folder.path().join("holds.jsonl").is_file() {
                continue;
            }
            if let Ok(account) = self.account(&provider) {
                account.forget(conversation);
            }
        }
    }

    fn rows(&self, provider: &str) -> Vec<(String, String)> {
        self.rows
            .lock()
            .expect("no holder of the rows panics")
            .get(provider)
            .cloned()
            .unwrap_or_default()
    }

    /// The answer to `request` of `model`, as a stream.
    async fn answer(&self, call: &Call, model: &Model, request: &Request) -> Result<Source, Failure> {
        let account = self.account(&call.provider)?;
        let rendered = anthropic::rendered(call, model, request)?;
        let system = rendered
            .system
            .iter()
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        let messages: Vec<Value> = rendered
            .messages
            .into_iter()
            .filter(|message| message["role"] != "system")
            .collect();
        if messages.last().is_none_or(|message| message["role"] != "user") {
            return Err(Failure::new(
                FailureKind::InvalidRequest,
                "The conversation must end with the person's words or the results of calls",
            ));
        }
        let tools = listed_tools(&request.tools);
        let settings = conversation::canonical(&json!([model.id, rendered.effort, tools]));
        let aside = matches!(request.tool_choice, Some(ToolChoice::None)) || request.format.is_some();
        let asked = Asked {
            model: model.id.clone(),
            system: system.clone(),
            tools: if aside { Vec::new() } else { tools },
            effort: rendered.effort,
            schema: request.format.as_ref().map(|format| format.schema.clone()),
            kept: !aside,
            settings,
        };
        let plan = Plan {
            chain: conversation::chain(&system, &messages),
            last: messages.iter().rposition(|message| message["role"] == "assistant"),
            root: conversation::root(&system),
            messages,
        };
        let (events, told) = mpsc::unbounded_channel();
        let rows = self.rows(&call.provider);
        if aside {
            let (session, told) = start_at_known(&account, call.now_ms, &asked, &plan, true).await?;
            tokio::spawn(answer_aside(account, session, told, asked.schema.is_some(), events, rows));
        } else {
            let prefix = plan.last.map_or_else(|| plan.root.clone(), |at| plan.chain[at].clone());
            let (session, told) = match account.take_live(&prefix, &asked.settings).await {
                Some(session) => (session, conversation::told_after(&plan.messages, plan.last)),
                None => start_at_known(&account, call.now_ms, &asked, &plan, false).await?,
            };
            let end = plan.chain.last().cloned().unwrap_or(plan.root);
            let answering = Answering {
                account,
                end,
                conversation: request.prompt_cache_key.clone(),
                rows,
            };
            tokio::spawn(answering.run(session, told, events));
        }
        Ok(source(told))
    }

    /// The models the account of `call` offers, each with the window Claude Code gives it, the account's plan's.
    async fn listed(&self, call: &Call) -> Result<Vec<Model>, Failure> {
        let account = self.account(&call.provider)?;
        let (process, answer) = account.asked(call.now_ms).await?;
        let version = answer["claude_code_version"].as_str().unwrap_or_default().to_owned();
        let rows: Vec<Value> = answer["models"].as_array().cloned().unwrap_or_default();
        let windows = {
            let kept = account.kept();
            if kept.windows.version == version {
                kept.windows.windows.clone()
            } else {
                HashMap::new()
            }
        };
        let mut found = windows.clone();
        for row in &rows {
            let Some(value) = row["value"].as_str() else { continue };
            if found.contains_key(value) {
                continue;
            }
            if let Some(window) = window_of(&process, value).await {
                found.insert(value.to_owned(), window);
            }
        }
        drop(process);
        if found != windows {
            let mut kept = account.kept();
            kept.windows.version = version;
            kept.windows.windows.clone_from(&found);
            if let Err(error) = kept.windows.write(&account.folder.join("windows.json")) {
                tracing::warn!(%error, "the windows of Claude's models could not be kept");
            }
        }
        let names = rows
            .iter()
            .filter_map(|row| {
                Some((
                    row["value"].as_str()?.to_owned(),
                    row["resolvedModel"].as_str().unwrap_or_default().to_owned(),
                ))
            })
            .collect();
        self.rows
            .lock()
            .expect("no holder of the rows panics")
            .insert(call.provider.clone(), names);
        Ok(rows.iter().filter_map(|row| model_of(&call.provider, row, &found)).collect())
    }
}

impl Driver for ClaudeCode {
    fn id(&self) -> &'static str {
        ID
    }

    fn name(&self) -> &'static str {
        "Claude"
    }

    fn connection(&self) -> Connection {
        Connection::Agent { through: "Claude Code" }
    }

    fn stream<'a>(&'a self, call: &'a Call, model: &'a Model, request: &'a Request) -> BoxFuture<'a, Result<Source, Failure>> {
        Box::pin(self.answer(call, model, request))
    }

    fn list<'a>(&'a self, call: &'a Call) -> BoxFuture<'a, Result<Option<Vec<Model>>, Failure>> {
        Box::pin(async move { self.listed(call).await.map(Some) })
    }
}

/// A request's conversation: its messages, the hash of each prefix, the last message of the model's, and the hash it
/// begins from.
#[derive(Debug)]
struct Plan {
    messages: Vec<Value>,
    chain: Vec<String>,
    last: Option<usize>,
    root: String,
}

/// The thread's function tools as MCP lists them.
fn listed_tools(tools: &[Tool]) -> Vec<Value> {
    tools
        .iter()
        .filter_map(|tool| match tool {
            Tool::Function {
                name,
                description,
                parameters,
            } => Some(account::listed_tool(name, description, parameters)),
            Tool::WebSearch => None,
        })
        .collect()
}

/// A session started at the longest prefix of the conversation a session still holds — forked, for an answer aside or
/// behind a session's tip; in place at its tip, any live one of it closed — or anew; and what it is given.
async fn start_at_known(account: &Account, time: i64, asked: &Asked, plan: &Plan, aside: bool) -> Result<(Session, Told), Failure> {
    let known = longest_present(account, plan);
    let begin = match &known {
        None => Begin::New,
        Some((at, position)) => {
            let fork = aside || !account.kept().positions.is_tip(&plan.chain[*at]);
            if !fork {
                account
                    .kept()
                    .live
                    .retain(|session| session.id.as_deref() != Some(position.session.as_str()));
            }
            Begin::Resume {
                at: position.clone(),
                fork,
            }
        }
    };
    let session = account.start(time, asked, &begin).await?;
    let told = conversation::told_after(&plan.messages, known.map(|(at, _)| at));
    Ok((session, told))
}

/// The longest prefix of the conversation, ending at a reply no later than its last, that a session still holds: one
/// found gone from Claude Code's folder forgotten, and the prefix sought again.
fn longest_present(account: &Account, plan: &Plan) -> Option<(usize, Position)> {
    loop {
        let known = {
            let kept = account.kept();
            (0..=plan.last?).rev().find_map(|at| {
                let position = (plan.messages[at]["role"] == "assistant").then(|| kept.positions.get(&plan.chain[at]))??;
                Some((at, position.clone()))
            })
        };
        let (at, position) = known?;
        let live = account
            .kept()
            .live
            .iter()
            .any(|session| session.id.as_deref() == Some(position.session.as_str()));
        if live || account.has(&position.session) {
            return Some((at, position));
        }
        let mut kept = account.kept();
        let _ = kept.positions.forget(&position.session);
        let _ = kept.holds.drop_session(&position.session);
    }
}

/// An answer in the conversation, as it runs: the account, the conversation's hash with what the session is given,
/// the conversation it is of, and the models the account offers.
struct Answering {
    account: Arc<Account>,
    end: String,
    conversation: Option<String>,
    rows: Vec<(String, String)>,
}

impl Answering {
    /// Answers in `session`, given `told`, onto `events`; then keeps where it settled, the conversation holding the
    /// session, and the session live for the next request.
    async fn run(self, mut session: Session, told: Told, events: crate::session::Events) {
        session.give(told);
        let mut measured = HashMap::new();
        let Some(answer) = session.stream(Some(&self.end), &events, &mut measured).await else {
            return;
        };
        let (settled, settling) = watch::channel(false);
        self.account.kept().settling.push(Settling {
            position: session.position.clone().unwrap_or_default(),
            settings: session.settings.clone(),
            settled: settling.clone(),
        });
        // The answer's stream ends only once a request that goes on from it would find the session settling.
        drop(events);
        let outcome = session.settle(answer, &mut measured).await;
        self.account.measured(&measured, &self.rows);
        self.keep(session, outcome, &settling);
        let _ = settled.send(true);
    }

    /// Keeps where `session` settled, the conversation holding it, and it live for the next request; the session it
    /// held before let go of where no conversation holds that now.
    fn keep(&self, session: Session, outcome: Settled, settling: &watch::Receiver<bool>) {
        let released = {
            let mut kept = self.account.kept();
            kept.settling.retain(|each| !each.settled.same_channel(settling));
            let Settled::At(entry) = outcome else { return };
            if !session.is_open() {
                return;
            }
            if let (Some(id), Some(entry), Some(position)) = (session.id.clone(), entry, session.position.clone())
                && let Err(error) = kept.positions.set(position, Position { session: id, entry })
            {
                tracing::warn!(%error, "the positions of Claude's sessions could not be kept");
            }
            let released = match (&self.conversation, &session.id) {
                (Some(conversation), Some(id)) => kept.holds.hold(conversation, id).unwrap_or_default(),
                _ => None,
            };
            kept.live.push(session);
            released
        };
        if let Some(released) = released {
            self.account.let_go(&released);
        }
    }
}

/// Answers aside in `session`, given `told`: the stream as it comes, or the JSON its turn ends with; the session
/// closed after.
async fn answer_aside(
    account: Arc<Account>,
    mut session: Session,
    told: Told,
    structured: bool,
    events: crate::session::Events,
    rows: Vec<(String, String)>,
) {
    session.give(told);
    let mut measured = HashMap::new();
    if structured {
        session.answer_structured(&events, &mut measured).await;
    } else if let Some(answer) = session.stream(None, &events, &mut measured).await {
        drop(events);
        session.settle(answer, &mut measured).await;
    }
    account.measured(&measured, &rows);
    session.close();
}

/// The stream of what `told` is sent.
fn source(told: mpsc::UnboundedReceiver<Result<octoii_ai::stream::Event, Failure>>) -> Source {
    Box::pin(futures::stream::unfold(told, |mut told| async move {
        told.recv().await.map(|event| (event, told))
    }))
}

/// Closes the account's sessions that stood idle, and removes those nothing holds, as long as the account is open.
async fn sweep(account: std::sync::Weak<Account>) {
    let mut ticks = tokio::time::interval(account::IDLE / 4);
    loop {
        ticks.tick().await;
        let Some(account) = account.upgrade() else { return };
        account.sweep();
    }
}

/// The window Claude Code gives the model `value`, in tokens, set on `process`; none where it would not tell.
async fn window_of(process: &crate::process::Process, value: &str) -> Option<u64> {
    process.ask(json!({ "subtype": "set_model", "model": value })).await.ok()?;
    let usage = process.ask(json!({ "subtype": "get_context_usage" })).await.ok()?;
    usage["rawMaxTokens"].as_u64().filter(|window| *window > 0)
}

/// A model Claude Code offers, as the directory lists it: its own name, the name a person reads, its window, how it
/// thinks; no price, as the person's plan pays for it.
fn model_of(provider: &str, row: &Value, windows: &HashMap<String, u64>) -> Option<Model> {
    let id = row["value"].as_str()?.to_owned();
    let efforts: Vec<ReasoningEffort> = row["supportedEffortLevels"]
        .as_array()
        .into_iter()
        .flatten()
        .rev()
        .filter_map(|level| serde_json::from_value(level.clone()).ok())
        .collect();
    let reasoning = (row["supportsEffort"] == true || row["supportsAdaptiveThinking"] == true).then(|| ModelReasoning {
        efforts: (!efforts.is_empty()).then_some(efforts),
        default_effort: None,
        mandatory: row["supportsThinkingOff"] == false,
    });
    Some(Model {
        provider: provider.to_owned(),
        name: row["displayName"].as_str().unwrap_or(&id).to_owned(),
        context_length: windows.get(&id).copied(),
        max_output_tokens: None,
        reads_images: true,
        reads_files: true,
        reasoning,
        parameters: None,
        pricing: None,
        release_date: None,
        searches_web: false,
        limits: InputLimits::default(),
        features: Features {
            instruction_updates: Some(false),
            effort_updates: Some(false),
            tool_choices: Some(vec!["auto".into(), "none".into()]),
        },
        id,
    })
}
