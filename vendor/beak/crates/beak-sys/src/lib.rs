//! Beak's C ABI, as bindgen declares it from `include/beak.h`: every call `unsafe`, as the header's comments say what
//! each asks. The safe API is the `beak` crate's.
#![allow(
    non_camel_case_types,
    non_upper_case_globals,
    non_snake_case,
    missing_debug_implementations,
    unsafe_code,
    clippy::all,
    clippy::pedantic
)]

include!(concat!(env!("OUT_DIR"), "/beak.rs"));
