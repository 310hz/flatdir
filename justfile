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

# 自分の環境(CARGO_HOME の bin)へ入れる。ユーザーが自分で実行する
install:
    cargo install --path .
