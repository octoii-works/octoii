//! Siphon's runtime on Beak. Programs run as Beak's processes, a phase at a time, each phase's changes and ending one
//! commit; the requests they make of the outside are answered by the handler of their name, each in a task of its own,
//! or by a person; what streams while a request is answered is published on a topic for whoever follows it; and
//! whoever follows the volume watches its latest commit and reads its journal on.

mod changes;
mod person;
mod program;
mod request;
mod runtime;
mod streams;

pub use changes::{Changes, Mail, Named};
pub use person::{AskedOfPerson, NotGiven};
pub use program::{Ending, Program, Retries, Run, Wait};
pub use request::{Answer, Asked, Request, Unanswered};
pub use runtime::{Builder, Clock, Collection, Options, Runtime, SystemClock};
pub use streams::{Following, Published, STREAM_BYTES_MAX, Streams};
