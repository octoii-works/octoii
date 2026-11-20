//! The DevTools protocol over the browser's pipe, as Chrome speaks it with `--remote-debugging-pipe`: each message
//! JSON ended by a NUL, the host's on the browser's descriptor 3 and the browser's on 4 — Beak's channel. A task of its
//! own owns the pipe: it writes each call with an id of its own and hands the browser's answer to what waits for it, and
//! each event on, as it came, to whoever hears the browser. Calls may be typed by the protocol's generated commands.

use std::collections::HashMap;

use base64::Engine as _;
use bytes::{Bytes, BytesMut};
use chromiumoxide_types::Command as CdpCommand;
use octoii::machine::{Command, CommandEvent, CommandEvents, Stdin};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt as _;
use tokio::sync::{mpsc, oneshot};

/// Why a call came to nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CdpError {
    /// The browser refused it, saying why.
    #[error("{message}")]
    Refused { code: i64, message: String },
    #[error("the browser ended")]
    Ended,
    #[error("the browser pipe failed: {0}")]
    Transport(String),
    #[error("the browser lost output at offsets {from}..{through}")]
    OutputLost { from: u64, through: u64 },
    /// No answer came in time, as a page a dialog holds gives none.
    #[error("the browser gave no answer in {0:?}")]
    TimedOut(std::time::Duration),
    #[error("the answer was not what the call gives: {0}")]
    Unread(String),
}

#[derive(Debug)]
struct Call {
    method: String,
    session: Option<String>,
    params: Value,
    reply: oneshot::Sender<Result<Value, CdpError>>,
}

/// The pipe, reached through the task that owns it.
#[derive(Debug, Clone)]
pub struct Cdp {
    calls: mpsc::UnboundedSender<Call>,
}

/// How long a call waits for its answer where it says no other time.
pub const CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

impl Cdp {
    pub(crate) async fn screenshot(&self, session: &str, quality: u32) -> Result<Bytes, CdpError> {
        let shot = self
            .call(
                Some(session),
                "Page.captureScreenshot",
                json!({ "format": "jpeg", "quality": quality }),
            )
            .await?;
        let data = shot["data"]
            .as_str()
            .ok_or_else(|| CdpError::Unread("screenshot data is missing".into()))?;
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map(Bytes::from)
            .map_err(|error| CdpError::Unread(error.to_string()))
    }

    /// Calls `method` with `params`, of the browser's own or of the target of `session`: what it gave.
    ///
    /// # Errors
    /// A browser refusal, transport failure, or a timeout past `CALL_TIMEOUT`.
    pub async fn call(&self, session: Option<&str>, method: &str, params: Value) -> Result<Value, CdpError> {
        self.call_within(session, method, params, CALL_TIMEOUT).await
    }

    /// Calls `method` with `params`, waiting `timeout` at most for its answer.
    ///
    /// # Errors
    /// As `call`, `TimedOut` past `timeout`.
    pub async fn call_within(
        &self,
        session: Option<&str>,
        method: &str,
        params: Value,
        timeout: std::time::Duration,
    ) -> Result<Value, CdpError> {
        let (reply, answer) = oneshot::channel();
        let call = Call {
            method: method.to_owned(),
            session: session.map(str::to_owned),
            params,
            reply,
        };
        self.calls.send(call).map_err(|_| CdpError::Ended)?;
        match tokio::time::timeout(timeout, answer).await {
            Ok(answered) => answered.map_err(|_| CdpError::Ended)?,
            Err(_) => Err(CdpError::TimedOut(timeout)),
        }
    }

    /// Sends `command`, of the browser's own or of the target of `session`: its answer.
    ///
    /// # Errors
    /// As `call`; `Unread` for an answer that is not the command's.
    pub async fn send<C: CdpCommand>(&self, session: Option<&str>, command: C) -> Result<C::Response, CdpError> {
        let method = command.identifier();
        let params = serde_json::to_value(&command).map_err(|error| CdpError::Unread(error.to_string()))?;
        let answer = self.call(session, &method, params).await?;
        C::response_from_value(answer).map_err(|error| CdpError::Unread(error.to_string()))
    }
}

/// Speaks the protocol over the pipe of `command`, whose events are `told`: the pipe, and the browser's events, which
/// end as the browser ends.
pub(crate) fn connect(command: Command, told: CommandEvents) -> (Cdp, mpsc::UnboundedReceiver<Value>) {
    let (calls, inbox) = mpsc::unbounded_channel();
    let (events, heard) = mpsc::unbounded_channel();
    tokio::spawn(own(command, told, inbox, events));
    (Cdp { calls }, heard)
}

/// The same DevTools messages, relayed by an attached desktop over the node's control connection.
pub(crate) fn connect_desktop(
    outgoing: mpsc::UnboundedSender<Value>,
    mut incoming: mpsc::UnboundedReceiver<Value>,
) -> (Cdp, mpsc::UnboundedReceiver<Value>) {
    let (calls, mut inbox) = mpsc::unbounded_channel();
    let (events, heard) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut pending = Pending::default();
        loop {
            tokio::select! {
                call = inbox.recv() => {
                    let Some(call) = call else { break };
                    if outgoing.send(pending.sent(call)).is_err() {
                        break;
                    }
                }
                message = incoming.recv() => {
                    let Some(message) = message else { break };
                    pending.heard(message, &events);
                }
            }
        }
        pending.ended(&mut inbox, &CdpError::Ended);
    });
    (Cdp { calls }, heard)
}

/// Owns the pipe till the browser ends, or every handle to it goes.
async fn own(command: Command, mut told: CommandEvents, mut calls: mpsc::UnboundedReceiver<Call>, events: mpsc::UnboundedSender<Value>) {
    let (outgoing, mut input) = mpsc::unbounded_channel::<Vec<u8>>();
    let writing = async move {
        let mut stdin = Stdin::new(command.into());
        while let Some(bytes) = input.recv().await {
            stdin.write_all(&bytes).await?;
        }
        Ok::<_, std::io::Error>(())
    };
    tokio::pin!(writing);
    let mut pending = Pending::default();
    let mut unended = BytesMut::new();
    let mut offset = 0;
    let end = loop {
        tokio::select! {
            result = &mut writing => break match result {
                Ok(()) => CdpError::Ended,
                Err(error) => CdpError::Transport(error.to_string()),
            },
            call = calls.recv() => {
                let Some(call) = call else { break CdpError::Ended };
                let mut bytes = serde_json::to_vec(&pending.sent(call)).unwrap_or_default();
                bytes.push(0);
                let _ = outgoing.send(bytes);
            }
            event = told.next() => match event {
                Ok(Some(CommandEvent::Output(chunk))) if chunk.offset == offset => {
                    offset += chunk.bytes.len() as u64;
                    unended.extend_from_slice(&chunk.bytes);
                    while let Some(end) = unended.iter().position(|&byte| byte == 0) {
                        let message = unended.split_to(end + 1);
                        if let Ok(value) = serde_json::from_slice(&message[..end]) {
                            pending.heard(value, &events);
                        }
                    }
                }
                Ok(Some(CommandEvent::Output(chunk))) => break CdpError::OutputLost { from: offset, through: chunk.offset },
                Ok(Some(CommandEvent::Gap { from, through })) => break CdpError::OutputLost { from, through },
                Ok(Some(CommandEvent::Title(_))) => {}
                Ok(Some(CommandEvent::Exit(_)) | None) => break CdpError::Ended,
                Err(error) => break CdpError::Transport(error.to_string()),
            },
        }
    };
    pending.ended(&mut calls, &end);
}

#[derive(Default)]
struct Pending {
    last_id: u64,
    replies: HashMap<u64, oneshot::Sender<Result<Value, CdpError>>>,
}

impl Pending {
    fn sent(&mut self, call: Call) -> Value {
        self.last_id += 1;
        let mut message = json!({ "id": self.last_id, "method": call.method, "params": call.params });
        if let Some(session) = call.session {
            message["sessionId"] = Value::String(session);
        }
        self.replies.retain(|_, reply| !reply.is_closed());
        self.replies.insert(self.last_id, call.reply);
        message
    }

    /// Answers end a call; events go on to the owner of the pages.
    fn heard(&mut self, mut value: Value, events: &mpsc::UnboundedSender<Value>) {
        if let Some(id) = value.get("id").and_then(Value::as_u64) {
            let Some(reply) = self.replies.remove(&id) else { return };
            let answer = match value.get_mut("error") {
                Some(error) => Err(CdpError::Refused {
                    code: error.get("code").and_then(Value::as_i64).unwrap_or_default(),
                    message: error.get("message").and_then(Value::as_str).unwrap_or_default().to_owned(),
                }),
                None => Ok(value.get_mut("result").map_or(Value::Null, Value::take)),
            };
            let _ = reply.send(answer);
        } else {
            let _ = events.send(value);
        }
    }

    fn ended(self, calls: &mut mpsc::UnboundedReceiver<Call>, error: &CdpError) {
        calls.close();
        for reply in self.replies.into_values() {
            let _ = reply.send(Err(error.clone()));
        }
        while let Ok(call) = calls.try_recv() {
            let _ = call.reply.send(Err(error.clone()));
        }
    }
}
