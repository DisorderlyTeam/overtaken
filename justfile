#!/usr/bin/env just --justfile

DEFAULT_RUST_LOG := "info,engine=debug,game=debug,symphonia_format_ogg=warn"

# Builds the project (by default, the game)
build *CARGO_ARGS:
    just ensure-vendor
    cargo build {{ CARGO_ARGS }}

# Runs the project (by default, the game)
run $RUST_LOG=DEFAULT_RUST_LOG *CARGO_ARGS:
    just ensure-vendor
    cargo run {{ CARGO_ARGS }}

# Exports everything necessary to run the project in release
export *EXPORTER_ARGS:
    just ensure-vendor
    cargo run --release --package exporter -- {{ EXPORTER_ARGS }}

# Generates `vendor/gilrs-core` (patched) used by Cargo via `[patch.crates-io]`.
vendor:
    cargo run -q --manifest-path vendor_tool/Cargo.toml -- vendor

# Ensures `vendor/gilrs-core` exists (runs `vendor` if missing).
ensure-vendor:
    cargo run -q --manifest-path vendor_tool/Cargo.toml -- ensure-vendor
