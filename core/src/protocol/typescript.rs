//! The protocol in TypeScript, for the client, as the sets a host assembled make it: every type a call, an answer or
//! an event names, declared once each in one module, by their names' order, so the module is the same however the
//! types were reached; the methods as a map of their params and results; the topics and events of every set as one
//! union each; and the messages either side sends. Large integers are numbers, as JSON carries them.

use std::any::TypeId;
use std::collections::BTreeMap;
use std::fmt::Write as _;

use ts_rs::{Config, TS, TypeVisitor};

use crate::protocol::{CALL_PATH, ConnectionEvent, Error, Machine, SUBPROTOCOL, Scope, Set, Subscribed, Unsubscribe, WATCH_PATH};

/// What leads the module.
const HEAD: &str = "// Octoii's protocol, generated from its Rust types — not to be edited by hand.\n";

/// What closes it: the messages either side sends.
const MESSAGES: &str = r"
/** An HTTP command or query. */
export type Command = { [M in keyof Methods]: { method: M; params: Methods[M]['params'] } }[keyof Methods];

export type CommandReply<M extends keyof Methods = keyof Methods> = { result: Methods[M]['result'] } | { error: Error };

/** The methods carried on the watch connection. */
export type WatchMethods = {
  subscribe: { params: { topic: Topic }; result: Subscribed };
  unsubscribe: { params: Unsubscribe; result: Nothing };
};

/** A response to a subscription control frame. */
export type Answer =
  | { id: string; result: WatchMethods[keyof WatchMethods]['result'] }
  | { id: string | null; error: Error };

/** A message from the watch connection. */
export type Received = Answer | Event;
";

/// Collects each type's declaration once, by its name, with every type it names — each name one type's alone, as a
/// module's names are.
pub struct Declarations<'a> {
    cfg: &'a Config,
    out: BTreeMap<String, (TypeId, String)>,
}

impl std::fmt::Debug for Declarations<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Declarations").field("names", &self.out.keys()).finish()
    }
}

impl TypeVisitor for Declarations<'_> {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        if T::output_path().is_none() {
            return;
        }
        let ident = T::ident(self.cfg);
        let id = TypeId::of::<T::WithoutGenerics>();
        if let Some((seen, _)) = self.out.get(&ident) {
            assert!(*seen == id, "two types are named {ident} in TypeScript; name one of them anew");
            // A generic type declared already may be named here with other arguments, each to be declared.
            T::visit_generics(self);
            return;
        }
        let docs = T::docs().unwrap_or_default();
        self.out.insert(ident, (id, format!("{docs}export {}", T::decl(self.cfg))));
        T::visit_dependencies(self);
        T::visit_generics(self);
    }
}

/// The TypeScript module of the protocol `sets` make together.
#[must_use]
pub fn typescript(sets: &[Set]) -> String {
    let cfg = Config::new().with_large_int("number");
    let mut declarations = Declarations {
        cfg: &cfg,
        out: BTreeMap::new(),
    };
    declarations.visit::<Scope>();
    declarations.visit::<Machine>();
    declarations.visit::<Error>();
    declarations.visit::<ConnectionEvent>();
    declarations.visit::<Subscribed>();
    declarations.visit::<Unsubscribe>();
    // What Siphon's own tools tell typed of a call, which a result keeps as JSON beside any MCP tool's.
    declarations.visit::<crate::tools::Structured>();
    let mut methods = String::new();
    let (mut topics, mut events) = (Vec::new(), vec![<ConnectionEvent as TS>::name(&cfg)]);
    for set in sets {
        (set.visit)(&mut declarations);
        for (name, params, result) in (set.signatures)(&cfg) {
            let _ = writeln!(methods, "  '{name}': {{ params: {params}; result: {result} }};");
        }
        if let Some(subscriptions) = &set.subscriptions {
            topics.push((subscriptions.topic)(&cfg));
            events.push((subscriptions.event)(&cfg));
        }
    }
    let declared: Vec<String> = declarations.out.into_values().map(|(_, declared)| declared).collect();
    format!(
        "{HEAD}\nexport const CALL_PATH = {CALL_PATH:?};\nexport const WATCH_PATH = {WATCH_PATH:?};\nexport const PROTOCOL = {SUBPROTOCOL:?};\n\n{}\n\n/** What a client subscribes to. */\nexport type Topic = {};\n\n/** An event: the connection's, or a subscription's. */\nexport type Event = {};\n\n/** HTTP methods and their parameters and results. */\nexport type Methods = {{\n{methods}}};\n{MESSAGES}",
        declared.join("\n\n"),
        topics.join(" | "),
        events.join(" | "),
    )
}
