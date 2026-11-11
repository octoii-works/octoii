//! The protocol in TypeScript, for the client, as the sets a host assembled make it: every type a call, an answer or
//! an event names, declared once each in one module, by their names' order, so the module is the same however the
//! types were reached; the methods as a map of their params and results; the topics and events of every set as one
//! union each; and the messages either side sends. Large integers are numbers, as JSON carries them.

use std::any::TypeId;
use std::collections::BTreeMap;
use std::fmt::Write as _;

use ts_rs::{Config, TS, TypeVisitor};

use crate::{ConnectionEvent, Error, Machine, Scope, Set, Subscribed, Unsubscribe};

/// What leads the module.
const HEAD: &str = "// Siphon's protocol, generated from its Rust types — not to be edited by hand.\n";

/// What closes it: the messages either side sends.
const MESSAGES: &str = r"
/** A call: its id, by which its answer comes, its method and the method's params. */
export type Call = { [M in keyof Methods]: { id: string; method: M; params: Methods[M]['params'] } }[keyof Methods];

/** An answer: the call's id and its method's result, or the error it was refused with. */
export type Answer<M extends keyof Methods = keyof Methods> =
  | { id: string; result: Methods[M]['result'] }
  | { id: string | null; error: Error };

/** What Siphon sends: an answer, or an event. */
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
    let mut methods = String::new();
    let (mut topics, mut events) = (Vec::new(), vec![<ConnectionEvent as TS>::name(&cfg)]);
    for set in sets {
        (set.visit)(&mut declarations);
        for (name, params, result) in (set.signatures)(&cfg) {
            let _ = writeln!(methods, "  '{name}': {{ params: {params}; result: {result} }};");
        }
        topics.push((set.topic)(&cfg));
        events.push((set.event)(&cfg));
    }
    let _ = writeln!(methods, "  'subscribe': {{ params: {{ topic: Topic }}; result: Subscribed }};");
    let _ = writeln!(methods, "  'unsubscribe': {{ params: Unsubscribe; result: Nothing }};");
    let declared: Vec<String> = declarations.out.into_values().map(|(_, declared)| declared).collect();
    format!(
        "{HEAD}\n{}\n\n/** What a client subscribes to. */\nexport type Topic = {};\n\n/** An event: the connection's, or a subscription's. */\nexport type Event = {};\n\n/** The methods, each by its name: the params it takes and the result it answers with. */\nexport type Methods = {{\n{methods}}};\n{MESSAGES}",
        declared.join("\n\n"),
        topics.join(" | "),
        events.join(" | "),
    )
}
