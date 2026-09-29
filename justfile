default: install

install: prepare
    cargo +stable install --path . -f --locked

prepare:
    just fmt
    just check

fmt:
    cargo +stable fmt --all

fix:
    cargo +stable clippy --fix --allow-dirty --all-targets

check:
    cargo +stable machete
    cargo +stable fmt --all -- --check
    cargo +stable clippy --locked --all-targets -- -D warnings
    cargo +stable test --locked --all-targets
