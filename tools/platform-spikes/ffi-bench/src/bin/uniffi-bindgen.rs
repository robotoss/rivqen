// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! UniFFI binding generator of the benchmark (`--features bindgen`); `run.sh` calls it.

fn main() {
    uniffi::uniffi_bindgen_main();
}
