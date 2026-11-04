# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Image of the Rivqen reference server (WP-03 S2, H-115, DL-095).
# The build context is the repository root, because the server depends by path on
# tools/rqp-markup/rust. Build from the repository root:
#   docker build -f server/rust-reference/Dockerfile -t rivqen-reference:local .
# The ignore list is server/rust-reference/Dockerfile.dockerignore (allow list).

FROM rust:1.99.0-alpine3.22@sha256:c3a5ad77ff2e5ec99fffaf62cb518e61a9dcbd9e1d7526d281a380db0be3bb3b AS build
# musl is the target of this image. On x86_64 the default is a binary that needs the musl loader,
# which the scratch image does not have, so crt-static is set on purpose. The image has no `file`,
# and ldd cannot tell a static-pie binary from a dynamic one. The proof is that the binary starts
# in the scratch stage: tools/reference-image/run.sh runs it there.
# The image toolchain is the pinned one; do not let rustup fetch components for rust-toolchain.toml.
ENV RUSTUP_TOOLCHAIN=1.99.0 \
    RUSTFLAGS="-C target-feature=+crt-static"
WORKDIR /src
COPY tools/rqp-markup/rust tools/rqp-markup/rust
COPY server/rust-reference server/rust-reference
WORKDIR /src/server/rust-reference
RUN cargo build --release --locked \
 && cp target/release/rivqen-reference /rivqen-reference

FROM scratch
COPY --from=build /rivqen-reference /rivqen-reference
USER 65532:65532
EXPOSE 18080
ENTRYPOINT ["/rivqen-reference"]
# --lab-peer needs a per-run control token (H-117): mount a token file at
# /run/rivqen/token (read-only), for example
#   docker run -p 127.0.0.1:18080:18080 -v "$PWD/token:/run/rivqen/token:ro" rivqen-reference:local
# Without the file the server exits 1 at start (fail closed).
CMD ["--conformance", "--listen", "0.0.0.0", "--port", "18080", "--lab-peer", "--control-token-file", "/run/rivqen/token"]
