set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

build:
    cargo build --release

test:
    cargo test

check:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test

# Install (or reinstall) into CARGO_HOME/bin. Run by the user, never by an agent.
install:
    cargo install --path . --force

uninstall:
    cargo uninstall flatdir
