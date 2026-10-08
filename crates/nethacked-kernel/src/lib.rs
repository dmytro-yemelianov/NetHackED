//! A byte-oriented runtime with explicit authority and ephemeral guest memory.
//!
//! Each native invocation is responsible for atomic validation of its own state.
//! Completed calls remain committed if a later call refuses or the guest traps.
//! This package provides neither resource transactions nor durable guest RAM.
#![forbid(unsafe_code)]

mod kernel;
mod model;
mod wasm;

pub use kernel::{Kernel, NativePort};
pub use model::*;
