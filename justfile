default: install

install: prepare
    cargo install --path . -f --locked

prepare:
    just fmt
    just check

fmt:
    cargo fmt --all

fix:
    cargo clippy --fix --allow-dirty --all-targets

check:
    cargo machete
    cargo fmt --all -- --check
    cargo clippy --locked --all-targets -- -D warnings
    cargo test --locked --all-targets
