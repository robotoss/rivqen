// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Peak resident memory of the process through `getrusage(RUSAGE_SELF)` (`ru_maxrss`).
//!
//! The JVM has no API for it, so the JVM harness reads it through JNI; the Swift harness calls
//! `getrusage` itself. Layout of `struct rusage` on 64-bit Darwin (macOS SDK
//! `sys/resource.h`, `sys/_types/_timeval.h`): two `struct timeval` (`long tv_sec`,
//! `int32 tv_usec`, padded to 16 bytes each) and 14 `long` values, 144 bytes; `ru_maxrss` is
//! the first `long`, in bytes on Darwin (`man getrusage`) and in KiB on Linux and Android
//! (getrusage(2) of the Linux man pages). Other targets give `None`.

#[cfg(all(
    target_pointer_width = "64",
    any(
        target_os = "macos",
        target_os = "ios",
        target_os = "linux",
        target_os = "android"
    )
))]
mod imp {
    use std::os::raw::{c_int, c_long};

    #[repr(C)]
    #[derive(Default)]
    struct Timeval {
        tv_sec: i64,
        tv_usec: c_long,
    }

    #[repr(C)]
    #[derive(Default)]
    pub(super) struct Rusage {
        ru_utime: Timeval,
        ru_stime: Timeval,
        pub(super) ru_maxrss: c_long,
        rest: [c_long; 13],
    }

    const RUSAGE_SELF: c_int = 0;

    unsafe extern "C" {
        fn getrusage(who: c_int, usage: *mut Rusage) -> c_int;
    }

    pub(super) fn maxrss() -> Option<c_long> {
        let mut usage = Rusage::default();
        // SAFETY: `usage` is a valid, aligned, writable `struct rusage` of the layout above
        // (144 bytes; the size is checked by a test); getrusage writes only that struct.
        let rc = unsafe { getrusage(RUSAGE_SELF, &raw mut usage) };
        (rc == 0).then_some(usage.ru_maxrss)
    }

    /// Bytes per unit of `ru_maxrss`.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub(super) const UNIT: u64 = 1;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    pub(super) const UNIT: u64 = 1024;

    #[cfg(test)]
    #[test]
    fn rusage_layout() {
        // tv_usec is int32 on Darwin and long on Linux; both pad the struct to 16 bytes.
        assert_eq!(std::mem::size_of::<Timeval>(), 16);
        assert_eq!(std::mem::size_of::<Rusage>(), 144);
        assert_eq!(std::mem::offset_of!(Rusage, ru_maxrss), 32);
    }
}

/// Peak resident set size of the process in bytes, or `None` when it is not known.
#[must_use]
pub fn peak_rss_bytes() -> Option<u64> {
    #[cfg(all(
        target_pointer_width = "64",
        any(
            target_os = "macos",
            target_os = "ios",
            target_os = "linux",
            target_os = "android"
        )
    ))]
    {
        let raw = imp::maxrss()?;
        u64::try_from(raw).ok()?.checked_mul(imp::UNIT)
    }
    #[cfg(not(all(
        target_pointer_width = "64",
        any(
            target_os = "macos",
            target_os = "ios",
            target_os = "linux",
            target_os = "android"
        )
    )))]
    {
        None
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn peak_rss_grows_with_touched_memory() {
        let before = peak_rss_bytes().unwrap();
        // The test process has at least a few MiB resident.
        assert!(before > 1 << 20, "peak {before}");
        // Touch 64 MiB more than the peak so far; the peak must follow.
        let size = usize::try_from(before).unwrap().saturating_add(64 << 20);
        let v = vec![1_u8; size];
        let after = peak_rss_bytes().unwrap();
        assert!(
            after >= before.saturating_add(32 << 20),
            "{before} -> {after}"
        );
        assert_eq!(v.iter().map(|&b| u64::from(b)).sum::<u64>(), size as u64);
    }
}
