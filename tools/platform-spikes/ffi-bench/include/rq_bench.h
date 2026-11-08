/* SPDX-License-Identifier: Apache-2.0 */
/* SPDX-FileCopyrightText: The Rivqen Authors */

/*
 * C ABI of the WP-04 FFI benchmark (tools/platform-spikes/CONTRACT.md section 10).
 * Written by hand (DL-137: no cbindgen). The Rust test header_matches_the_rust_values
 * checks the codes and the declarations against src/abi.
 *
 * Handles: an RqBenchStream pointer is an opaque handle, never a memory address.
 * A NULL, closed or unknown handle gives RQ_BENCH_E_HANDLE; closing twice is safe.
 * No function panics across the boundary (RQ_BENCH_E_PANIC instead).
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
 * RQ_BENCH_OK, RQ_BENCH_END (*out_len == 0) or a negative error.
 * buf: valid for cap bytes; out_len: valid for one size_t. */
int32_t rq_bench_read(RqBenchStream *s, uint8_t *buf, size_t cap, size_t *out_len);

/* Closes the stream: RQ_BENCH_OK, or RQ_BENCH_E_HANDLE when it is not open. */
int32_t rq_bench_close(RqBenchStream *s);

/* *out = bytes that the Rust side copied out of the stream. */
int32_t rq_bench_copied(RqBenchStream *s, uint64_t *out);

/* Writes the 32-byte SHA-256 of the document of size bytes to out (cap >= 32). */
int32_t rq_bench_sha256(uint64_t size, uint8_t *out, size_t cap);

#ifdef __cplusplus
}
#endif

#endif /* RQ_BENCH_H */
