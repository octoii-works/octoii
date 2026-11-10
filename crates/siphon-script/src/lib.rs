//! Scripts: JavaScript run on QuickJS, each on an engine of its own, as the body of an async function, with the host's
//! functions under a global object it names, a call of one a promise the host settles. A script is stepped until it
//! waits on the calls it made, returns, throws or spends its steps, each step at the time its host gives; bounded by
//! steps and memory; its clock and its random numbers the host's. Given the same times, seed and settlements, a script
//! runs alike, as a host that replays it needs. A script stays on the thread it was started on.

mod engine;

use std::ffi::{c_char, c_void};
use std::ptr::NonNull;

/// What a script may spend: the bytes of its memory, and its steps — the engine's interrupt ticks, counted, so a script
/// stops at the same point every time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub memory_max: usize,
    pub steps_max: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            memory_max: 64 << 20,
            steps_max: 1_000_000,
        }
    }
}

/// What a script is given: the global object the host's functions are under, by its name, as Math holds its own, and
/// the functions, each `group.name` under it; what it may spend; the seed of its random numbers.
#[derive(Debug, Clone, Copy)]
pub struct Host<'a> {
    pub namespace: &'a str,
    pub functions: &'a [&'a str],
    pub limits: Limits,
    pub seed: u64,
}

/// A call a script made: its id, which settles it; the function, `group.name` under the namespace; its argument, as
/// JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub id: u32,
    pub function: String,
    pub arguments: String,
}

/// What a script threw: an error's name, message and stack, the stack's places in `script.js` as the script's own
/// lines and columns; any other value its JSON as the message, named `Thrown`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thrown {
    pub name: String,
    pub message: String,
    pub stack: String,
}

/// Where a step left a script: waiting on its calls — those it made in the step given — returned a value, as JSON;
/// threw; or ran past its steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    Waiting(Vec<Call>),
    Returned(String),
    Threw(Thrown),
    OutOfSteps,
}

/// The call a settlement names is none the script waits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("no-such-call")]
pub struct NoSuchCall;

/// A script on an engine of its own.
#[derive(Debug)]
pub struct Script {
    engine: NonNull<engine::Engine>,
}

impl Script {
    /// Starts `source`, JavaScript, as `host` says, its clock telling `time_ms` — milliseconds since 1970 — and runs it
    /// to where its first step leaves it.
    ///
    /// # Panics
    /// Where there is no memory for an engine.
    #[must_use]
    pub fn start(source: &str, host: &Host<'_>, time_ms: i64) -> (Self, Progress) {
        let functions: Vec<engine::Name> = host.functions.iter().map(|function| engine::Name::of(function)).collect();
        // SAFETY: every name points into memory that outlives the call, which copies them.
        let made = unsafe {
            engine::engine_new(
                engine::Name::of(host.namespace),
                functions.as_ptr(),
                functions.len(),
                host.limits.memory_max,
                host.limits.steps_max,
                host.seed,
            )
        };
        let mut script = Self {
            engine: NonNull::new(made).expect("memory for a script's engine"),
        };
        // SAFETY: the engine is the script's, and the source outlives the call.
        let status = unsafe { engine::engine_run(script.engine.as_ptr(), source.as_ptr().cast::<c_char>(), source.len(), time_ms) };
        let step = script.progress_of(status);
        (script, step)
    }

    /// Runs it on, its clock telling `time_ms`, until it waits on calls, returns, throws or spends its steps. A script
    /// that came to an end stays as it ended.
    pub fn resume(&mut self, time_ms: i64) -> Progress {
        // SAFETY: the engine is the script's.
        let status = unsafe { engine::engine_step(self.engine.as_ptr(), time_ms) };
        self.progress_of(status)
    }

    /// Settles call `id`: with the JSON `Ok` holds, or rejected with an Error whose message `Err` holds. JSON that does
    /// not parse rejects it with what parsing threw.
    ///
    /// # Errors
    /// `NoSuchCall` where the script waits on no call of that id.
    pub fn settle(&mut self, id: u32, outcome: Result<&str, &str>) -> Result<(), NoSuchCall> {
        let (ok, text) = match outcome {
            Ok(json) => (1, json),
            Err(message) => (0, message),
        };
        // SAFETY: the engine is the script's, and the text outlives the call, which copies it.
        let settled = unsafe { engine::engine_settle(self.engine.as_ptr(), id, ok, text.as_ptr().cast::<c_char>(), text.len()) };
        if settled == 0 { Ok(()) } else { Err(NoSuchCall) }
    }

    /// The lines it wrote to its console since they were last taken.
    pub fn output(&mut self) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        // SAFETY: the engine is the script's; `push_text` is handed `lines`, which outlives the call.
        unsafe { engine::engine_output(self.engine.as_ptr(), push_text, (&raw mut lines).cast::<c_void>()) };
        lines
    }

    /// The steps it spent.
    #[must_use]
    pub fn steps(&self) -> u64 {
        // SAFETY: the engine is the script's.
        unsafe { engine::engine_steps(self.engine.as_ptr()) }
    }

    fn progress_of(&mut self, status: i32) -> Progress {
        let engine = self.engine.as_ptr();
        match status {
            engine::WAITING => {
                let mut calls: Vec<Call> = Vec::new();
                // SAFETY: the engine is the script's; `push_call` is handed `calls`, which outlives the call.
                unsafe { engine::engine_calls(engine, push_call, (&raw mut calls).cast::<c_void>()) };
                Progress::Waiting(calls)
            }
            engine::RETURNED => {
                let mut returned: Vec<String> = Vec::new();
                // SAFETY: as above.
                unsafe { engine::engine_returned(engine, push_text, (&raw mut returned).cast::<c_void>()) };
                Progress::Returned(returned.pop().unwrap_or_default())
            }
            engine::THREW => {
                let mut thrown = None;
                // SAFETY: as above.
                unsafe { engine::engine_thrown(engine, keep_thrown, (&raw mut thrown).cast::<c_void>()) };
                Progress::Threw(thrown.unwrap_or_else(|| Thrown {
                    name: String::new(),
                    message: String::new(),
                    stack: String::new(),
                }))
            }
            _ => Progress::OutOfSteps,
        }
    }
}

impl Drop for Script {
    fn drop(&mut self) {
        // SAFETY: the engine is the script's, and freed once, here.
        unsafe { engine::engine_free(self.engine.as_ptr()) };
    }
}

/// The text of `len` bytes at `bytes`, as UTF-8, what is not read as U+FFFD.
///
/// # Safety
/// `bytes` points to `len` bytes, or `len` is 0.
unsafe fn text(bytes: *const c_char, len: usize) -> String {
    if len == 0 {
        return String::new();
    }
    // SAFETY: as the caller says.
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(bytes.cast::<u8>(), len) }).into_owned()
}

unsafe extern "C" fn push_text(to: *mut c_void, bytes: *const c_char, len: usize) {
    // SAFETY: the engine hands back what it was given, a `Vec<String>`, and bytes of `len`.
    unsafe { (*to.cast::<Vec<String>>()).push(text(bytes, len)) };
}

unsafe extern "C" fn push_call(
    to: *mut c_void,
    id: u32,
    function: *const c_char,
    function_len: usize,
    arguments: *const c_char,
    arguments_len: usize,
) {
    // SAFETY: the engine hands back what it was given, a `Vec<Call>`, and texts of their lengths.
    unsafe {
        (*to.cast::<Vec<Call>>()).push(Call {
            id,
            function: text(function, function_len),
            arguments: text(arguments, arguments_len),
        });
    }
}

unsafe extern "C" fn keep_thrown(
    to: *mut c_void,
    name: *const c_char,
    name_len: usize,
    message: *const c_char,
    message_len: usize,
    stack: *const c_char,
    stack_len: usize,
) {
    // SAFETY: the engine hands back what it was given, an `Option<Thrown>`, and texts of their lengths.
    unsafe {
        *to.cast::<Option<Thrown>>() = Some(Thrown {
            name: text(name, name_len),
            message: text(message, message_len),
            stack: text(stack, stack_len),
        });
    }
}
