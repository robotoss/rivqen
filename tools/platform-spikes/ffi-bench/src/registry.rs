// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Handle table of the C ABI and JNI paths.
//!
//! A handle is a number, not a pointer: `(generation << SLOT_BITS) | (slot + 1)`. The C ABI
//! gives it to C as an opaque `RqBenchStream *` and never dereferences it. So a null, unknown,
//! stale (closed) or forged handle is a table miss and gives an error code: no use after free,
//! no double free (FFI-01, FFI-02). The table has at most [`MAX_OPEN`] streams.
//!
//! Known limit: the generation of a slot wraps after `2^(usize::BITS - 12)` closes of that
//! slot; a handle kept that long could then name a new stream (no memory error, only a wrong
//! stream).

use std::num::NonZeroUsize;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::doc::{Cursor, OpenError, Stream};

/// Most streams open at the same time.
pub const MAX_OPEN: usize = 1024;

/// Bits of the slot field (slot + 1 is at most 1024 < 2^11).
const SLOT_BITS: u32 = 11;

/// Generation bits: one bit less than the rest, so a handle is always below 2^(BITS-1) and
/// fits in a Java `long` and in a non-negative `isize`.
const GEN_BITS: u32 = usize::BITS - SLOT_BITS - 1;

const SLOT_MASK: usize = (1 << SLOT_BITS) - 1;
const GEN_MASK: usize = (1 << GEN_BITS) - 1;

/// Why a registry call failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryError {
    /// Null, unknown, closed or forged handle.
    Handle,
    /// Bad `size` or `chunk`.
    Open(OpenError),
    /// [`MAX_OPEN`] streams are open.
    Full,
}

#[derive(Debug)]
struct Slot {
    generation: usize,
    stream: Option<Stream>,
}

/// A bounded table of open streams.
#[derive(Debug, Default)]
pub struct Registry {
    slots: Vec<Slot>,
    open: usize,
}

fn encode(slot: usize, generation: usize) -> Option<NonZeroUsize> {
    let low = slot.checked_add(1)?;
    if low > SLOT_MASK || generation > GEN_MASK {
        return None;
    }
    NonZeroUsize::new((generation << SLOT_BITS) | low)
}

fn decode(handle: usize) -> Option<(usize, usize)> {
    let slot = (handle & SLOT_MASK).checked_sub(1)?;
    let generation = handle >> SLOT_BITS;
    (generation <= GEN_MASK).then_some((slot, generation))
}

impl Registry {
    /// An empty table (no allocation).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            open: 0,
        }
    }

    /// Streams open now.
    #[must_use]
    pub fn open_count(&self) -> usize {
        self.open
    }

    /// Opens a stream and returns its handle (never 0).
    ///
    /// # Errors
    ///
    /// [`RegistryError::Open`] for a bad `size` or `chunk`; [`RegistryError::Full`] when
    /// [`MAX_OPEN`] streams are open.
    pub fn open(&mut self, size: u64, chunk: u32) -> Result<NonZeroUsize, RegistryError> {
        let cursor = Cursor::new(size, chunk).map_err(RegistryError::Open)?;
        let index = match self.slots.iter().position(|s| s.stream.is_none()) {
            Some(i) => i,
            None if self.slots.len() < MAX_OPEN => {
                self.slots.push(Slot {
                    generation: 0,
                    stream: None,
                });
                self.slots.len().saturating_sub(1)
            }
            None => return Err(RegistryError::Full),
        };
        let slot = self.slots.get_mut(index).ok_or(RegistryError::Full)?;
        let handle = encode(index, slot.generation).ok_or(RegistryError::Full)?;
        slot.stream = Some(Stream::new(cursor));
        self.open = self.open.saturating_add(1);
        Ok(handle)
    }

    /// Runs `f` on the open stream of `handle`.
    ///
    /// # Errors
    ///
    /// [`RegistryError::Handle`] when `handle` names no open stream.
    pub fn with<R>(
        &mut self,
        handle: usize,
        f: impl FnOnce(&mut Stream) -> R,
    ) -> Result<R, RegistryError> {
        let (index, generation) = decode(handle).ok_or(RegistryError::Handle)?;
        let slot = self.slots.get_mut(index).ok_or(RegistryError::Handle)?;
        if slot.generation != generation {
            return Err(RegistryError::Handle);
        }
        slot.stream.as_mut().map(f).ok_or(RegistryError::Handle)
    }

    /// Closes the stream of `handle`. A second close of the same handle is an error, not a
    /// double free.
    ///
    /// # Errors
    ///
    /// [`RegistryError::Handle`] when `handle` names no open stream.
    pub fn close(&mut self, handle: usize) -> Result<(), RegistryError> {
        let (index, generation) = decode(handle).ok_or(RegistryError::Handle)?;
        let slot = self.slots.get_mut(index).ok_or(RegistryError::Handle)?;
        if slot.generation != generation || slot.stream.is_none() {
            return Err(RegistryError::Handle);
        }
        slot.stream = None;
        slot.generation = slot.generation.wrapping_add(1) & GEN_MASK;
        self.open = self.open.saturating_sub(1);
        Ok(())
    }
}

/// The table of the process (C ABI and JNI share it).
static GLOBAL: Mutex<Registry> = Mutex::new(Registry::new());

/// Locks the table of the process. A poisoned lock is taken over: every table operation
/// changes the table in one step, so a panic cannot leave it half changed.
pub fn global() -> MutexGuard<'static, Registry> {
    GLOBAL.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;

    #[test]
    fn handle_encoding_round_trip() {
        assert_eq!(encode(0, 0).map(NonZeroUsize::get), Some(1));
        assert_eq!(encode(1023, 0).map(NonZeroUsize::get), Some(1024));
        assert_eq!(
            encode(0, 1).map(NonZeroUsize::get),
            Some(1 << SLOT_BITS | 1)
        );
        assert_eq!(encode(SLOT_MASK, 0), None);
        assert_eq!(encode(0, GEN_MASK + 1), None);
        assert_eq!(decode(0), None);
        assert_eq!(decode(1 << SLOT_BITS), None); // slot field 0
        assert_eq!(decode(1), Some((0, 0)));
        assert_eq!(decode((5 << SLOT_BITS) | 3), Some((2, 5)));
        assert_eq!(decode(usize::MAX), None); // generation above GEN_MASK
        let top = encode(SLOT_MASK - 1, GEN_MASK).unwrap().get();
        assert!(i64::try_from(top).is_ok(), "fits in a Java long");
        assert_eq!(decode(top), Some((SLOT_MASK - 1, GEN_MASK)));
    }

    #[test]
    fn open_read_close() {
        let mut r = Registry::new();
        let h = r.open(10, 4).unwrap().get();
        assert_eq!(r.open_count(), 1);
        assert_eq!(r.with(h, |s| s.cursor().size()), Ok(10));
        assert_eq!(r.close(h), Ok(()));
        assert_eq!(r.open_count(), 0);
    }

    #[test]
    fn close_twice_is_an_error() {
        let mut r = Registry::new();
        let h = r.open(10, 4).unwrap().get();
        assert_eq!(r.close(h), Ok(()));
        assert_eq!(r.close(h), Err(RegistryError::Handle));
        assert_eq!(r.with(h, |_| ()), Err(RegistryError::Handle));
        assert_eq!(r.open_count(), 0);
    }

    #[test]
    fn stale_handle_does_not_reach_the_new_stream() {
        let mut r = Registry::new();
        let old = r.open(10, 4).unwrap().get();
        r.close(old).unwrap();
        let new = r.open(20, 4).unwrap().get();
        assert_ne!(old, new, "same slot, new generation");
        assert_eq!(r.with(old, |_| ()), Err(RegistryError::Handle));
        assert_eq!(r.close(old), Err(RegistryError::Handle));
        assert_eq!(r.with(new, |s| s.cursor().size()), Ok(20));
    }

    #[test]
    fn null_and_forged_handles() {
        let mut r = Registry::new();
        let h = r.open(10, 4).unwrap().get();
        for bad in [0, h + 1, h ^ (1 << SLOT_BITS), usize::MAX, 1 << 40] {
            assert_eq!(r.with(bad, |_| ()), Err(RegistryError::Handle), "{bad:#x}");
            assert_eq!(r.close(bad), Err(RegistryError::Handle), "{bad:#x}");
        }
        assert_eq!(r.open_count(), 1);
    }

    #[test]
    fn bad_arguments() {
        let mut r = Registry::new();
        assert_eq!(r.open(1, 0), Err(RegistryError::Open(OpenError::Chunk)));
        assert_eq!(
            r.open(u64::MAX, 1),
            Err(RegistryError::Open(OpenError::Size))
        );
        assert_eq!(r.open_count(), 0);
    }

    #[test]
    fn table_is_bounded() {
        let mut r = Registry::new();
        let handles: Vec<usize> = (0..MAX_OPEN).map(|_| r.open(1, 1).unwrap().get()).collect();
        assert_eq!(r.open_count(), MAX_OPEN);
        assert_eq!(r.open(1, 1), Err(RegistryError::Full));
        r.close(handles[17]).unwrap();
        let again = r.open(1, 1).unwrap().get();
        assert_eq!(decode(again), Some((17, 1)), "the free slot is used again");
        assert_eq!(r.slots.len(), MAX_OPEN);
    }

    #[test]
    fn global_table_works() {
        let h = global().open(3, 1).unwrap().get();
        assert_eq!(global().close(h), Ok(()));
        assert_eq!(global().close(h), Err(RegistryError::Handle));
    }
}
