/* SPDX-License-Identifier: Apache-2.0 */
/* SPDX-FileCopyrightText: The Rivqen Authors */

/*
 * C ABI of the WP-04 FFI benchmark (tools/platform-spikes/CONTRACT.md section 10).
 * Written by hand (DL-137: no cbindgen). The Rust tests header_matches_the_rust_values and
 * header_has_the_pointer_contracts check the codes, the declarations and the pointer
 * contracts against src/abi/c.rs.
 *
 * Handles: an RqBenchStream pointer is an opaque handle, never a memory address.
 * A NULL, closed or unknown handle gives RQ_BENCH_E_HANDLE; closing twice is safe.
 * No function panics across the boundary (RQ_BENCH_E_PANIC instead).
 * Every function is thread-safe: one process-wide lock protects the handle table.
 *
 * Pointer contracts. They are the "# Safety" sections of the Rust functions. A call that
 * breaks one is undefined behavior, and the library cannot detect it:
 *   - s: any value. The library never dereferences it.
 *   - Every other pointer argument is NULL (the function then gives RQ_BENCH_E_NULL) or
 *     points to memory that is valid for writes of the stated size and aligned for its
 *     type, for the whole call.
 *   - No other thread reads or writes that memory during the call.
 *   - The library keeps no pointer after the call returns.
 */

#ifndef RQ_BENCH_H
#define RQ_BENCH_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define RQ_BENCH_OK (0)
#define RQ_BENCH_END (1)
#define RQ_BENCH_E_HANDLE (-1)
#define RQ_BENCH_E_NULL (-2)
#define RQ_BENCH_E_ARG (-3)
#define RQ_BENCH_E_PANIC (-4)
#define RQ_BENCH_E_JNI (-5)

typedef struct RqBenchStream RqBenchStream;

/* Opens the document of size bytes in chunks of chunk bytes (1..65536).
 * NULL for a bad argument or when 1024 streams are open. */
RqBenchStream *rq_bench_open(uint64_t size, uint32_t chunk);

/* Copies the next chunk (at most min(chunk, cap) bytes) into buf; *out_len gets its length.
 * RQ_BENCH_OK, RQ_BENCH_END (*out_len == 0) or a negative error; cap == 0 gives
 * RQ_BENCH_E_ARG. After an error the stream is where it was.
 * Safety: buf is NULL or valid for writes of cap bytes.
 * Safety: out_len is NULL or valid for one aligned size_t write.
 * Safety: buf and out_len do not overlap. */
int32_t rq_bench_read(RqBenchStream *s, uint8_t *buf, size_t cap, size_t *out_len);

/* Closes the stream: RQ_BENCH_OK, or RQ_BENCH_E_HANDLE when it is not open. */
int32_t rq_bench_close(RqBenchStream *s);

/* *out = bytes that the Rust side copied out of the stream.
 * Safety: out is NULL or valid for one aligned uint64_t write. */
int32_t rq_bench_copied(RqBenchStream *s, uint64_t *out);

/* Writes the 32-byte SHA-256 of the document of size bytes to out; cap < 32 or a size above
 * the limit gives RQ_BENCH_E_ARG.
 * Safety: out is NULL or valid for writes of cap bytes. */
int32_t rq_bench_sha256(uint64_t size, uint8_t *out, size_t cap);

#ifdef __cplusplus
}
#endif

#endif /* RQ_BENCH_H */
