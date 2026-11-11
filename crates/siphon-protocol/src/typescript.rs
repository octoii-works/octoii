//! The protocol in TypeScript, for the client: every type a call, an answer or an event names, declared once each in
//! one module, by their names' order, so the module is the same however the types were reached; the methods as a map of their params and results; and the messages
//! either side sends, as the client reads and writes them. Large integers are numbers, as JSON carries them.

use std::any::TypeId;
use std::collections::BTreeMap;
use std::fmt::Write as _;

use ts_rs::{Config, TS, TypeVisitor};

use crate::{Error, Event, Machine, Scope};

/// What leads the module.
const HEAD: &str = "// Siphon's protocol, generated from its Rust types by `siphon-protocol`'s test — not to be edited by hand.\n";

/// What closes it: the methods' map, and the messages either side sends.
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
struct Declarations<'a> {
    cfg: &'a Config,
    out: BTreeMap<String, (TypeId, String)>,
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
            return;
        }
        let docs = T::docs().unwrap_or_default();
        self.out.insert(ident, (id, format!("{docs}export {}", T::decl(self.cfg))));
        T::visit_dependencies(self);
        T::visit_generics(self);
    }
}

/// The protocol's TypeScript module.
#[must_use]
pub fn typescript() -> String {
    let cfg = Config::new().with_large_int("number");
    let mut declarations = Declarations {
        cfg: &cfg,
        out: BTreeMap::new(),
    };
    declarations.visit::<Scope>();
    declarations.visit::<Machine>();
    declarations.visit::<Error>();
    declarations.visit::<Event>();
    crate::methods::visit(&mut declarations);
    let mut methods = String::new();
    for (name, params, result) in crate::methods::signatures(&cfg) {
        let _ = writeln!(methods, "  '{name}': {{ params: {params}; result: {result} }};");
    }
    format!(
        "{HEAD}\n{}\n\n/** The methods, each by its name: the params it takes and the result it answers with. */\nexport type Methods = {{\n{methods}}};\n{MESSAGES}",
        declarations
            .out
            .into_values()
            .map(|(_, declared)| declared)
            .collect::<Vec<_>>()
            .join("\n\n")
    )
}
