//! The engine `engine.c` makes of QuickJS, as Rust declares its calls.

use std::ffi::{c_char, c_int, c_void};

pub(crate) const WAITING: c_int = 0;
pub(crate) const RETURNED: c_int = 1;
pub(crate) const THREW: c_int = 2;

/// An engine, `engine.c`'s own.
#[repr(C)]
#[derive(Debug)]
pub(crate) struct Engine {
    _private: [u8; 0],
}

/// Bytes the host gives, not ended by a NUL.
#[repr(C)]
pub(crate) struct Name {
    bytes: *const c_char,
    len: usize,
}

impl Name {
    pub(crate) fn of(text: &str) -> Self {
        Self {
            bytes: text.as_ptr().cast::<c_char>(),
            len: text.len(),
        }
    }
}

pub(crate) type TextFn = unsafe extern "C" fn(to: *mut c_void, bytes: *const c_char, len: usize);
pub(crate) type CallFn = unsafe extern "C" fn(
    to: *mut c_void,
    id: u32,
    function: *const c_char,
    function_len: usize,
    arguments: *const c_char,
    arguments_len: usize,
);
pub(crate) type ThrownFn = unsafe extern "C" fn(
    to: *mut c_void,
    name: *const c_char,
    name_len: usize,
    message: *const c_char,
    message_len: usize,
    stack: *const c_char,
    stack_len: usize,
);

unsafe extern "C" {
    pub(crate) fn engine_new(
        namespace: Name,
        functions: *const Name,
        function_count: usize,
        memory_max: usize,
        steps_max: u64,
        seed: u64,
    ) -> *mut Engine;
    pub(crate) fn engine_run(engine: *mut Engine, source: *const c_char, len: usize, time: i64) -> c_int;
    pub(crate) fn engine_step(engine: *mut Engine, time: i64) -> c_int;
    pub(crate) fn engine_calls(engine: *mut Engine, each: CallFn, to: *mut c_void);
    pub(crate) fn engine_settle(engine: *mut Engine, id: u32, ok: c_int, text: *const c_char, len: usize) -> c_int;
    pub(crate) fn engine_returned(engine: *mut Engine, each: TextFn, to: *mut c_void);
    pub(crate) fn engine_thrown(engine: *mut Engine, each: ThrownFn, to: *mut c_void);
    pub(crate) fn engine_output(engine: *mut Engine, each: TextFn, to: *mut c_void);
    pub(crate) fn engine_steps(engine: *const Engine) -> u64;
    pub(crate) fn engine_free(engine: *mut Engine);
}
