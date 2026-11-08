// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The only module with hand-written unsafe code: the C ABI (`include/rq_bench.h`), the JNI
//! glue of `dev.rivqen.spikes.ffi.NativeBench`, and `getrusage`.
//!
//! Rules of every exported function: no panic crosses the boundary (each body runs in
//! [`guard`]); a handle is a table key, never a pointer ([`crate::registry`]); a caller buffer
//! is written only inside `cap` bytes; every error is a negative status code.

pub mod c;
pub mod jni;
pub mod sys;

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::registry::RegistryError;

/// Success.
pub const OK: i32 = 0;
/// End of the document.
pub const END: i32 = 1;
/// Null, unknown or closed handle.
pub const E_HANDLE: i32 = -1;
/// Null pointer argument.
pub const E_NULL: i32 = -2;
/// Bad argument (buffer of 0 bytes, size or chunk out of range, table full).
pub const E_ARG: i32 = -3;
/// A panic was caught at the boundary.
pub const E_PANIC: i32 = -4;
/// A JNI call failed.
pub const E_JNI: i32 = -5;

/// Runs `f`; a panic gives `on_panic`. Every exported function goes through this.
pub fn guard<T>(on_panic: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(on_panic)
}

/// Status code of a registry error.
#[must_use]
pub fn code(e: RegistryError) -> i32 {
    match e {
        RegistryError::Handle => E_HANDLE,
        RegistryError::Open(_) | RegistryError::Full => E_ARG,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::OpenError;

    #[test]
    fn guard_turns_a_panic_into_the_error_value() {
        assert_eq!(guard(E_PANIC, || OK), OK);
        // The panic message goes to stderr of the test; the call returns the error value.
        #[allow(clippy::panic)]
        let r = guard(E_PANIC, || -> i32 { panic!("injected") });
        assert_eq!(r, E_PANIC);
    }

    #[test]
    fn codes_are_distinct_and_negative() {
        let errors = [E_HANDLE, E_NULL, E_ARG, E_PANIC, E_JNI];
        assert!(errors.iter().all(|&e| e < 0));
        for (i, a) in errors.iter().enumerate() {
            for b in errors.iter().skip(i.saturating_add(1)) {
                assert_ne!(a, b);
            }
        }
        assert_eq!((OK, END), (0, 1));
        assert_eq!(code(RegistryError::Handle), E_HANDLE);
        assert_eq!(code(RegistryError::Full), E_ARG);
        assert_eq!(code(RegistryError::Open(OpenError::Chunk)), E_ARG);
    }
}
