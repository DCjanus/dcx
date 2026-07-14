# dtools

`dtools` is a small collection of command-line tools for processing UTF-8 text files and streams.

## Tools

### `sort_in_place`

Sort a text file in place. The command reads the entire file into memory, writes the result to a temporary file in the same directory, and atomically replaces the original file.

```console
sort_in_place [OPTIONS] <PATH>
```

Options:

- `--size-limit <SIZE>`: maximum file size to read into memory (default: `1GB`)
- `-u, --uniq`: remove duplicate lines after sorting
- `-t, --trim-whitespace`: trim leading and trailing whitespace before sorting

```console
sort_in_place --uniq --trim-whitespace words.txt
```

### `uniq_any_order`

Remove duplicate lines from standard input without requiring sorted input. By default, lines are emitted in first-seen order.

```console
uniq_any_order [OPTIONS]
```

Use `--count` to print each unique line with its occurrence count. Counted output order is unspecified.

```console
cat words.txt | uniq_any_order --count
```

## Installation

The recommended installation method is [cargo-binstall](https://github.com/cargo-bins/cargo-binstall). It downloads the prebuilt binaries from this repository's continuously updated `latest` release:

```console
cargo binstall --git https://github.com/DCjanus/dtools dtools
```

This installs both `sort_in_place` and `uniq_any_order`. Prebuilt packages are available for x86_64 Linux, Intel and Apple Silicon macOS, and x86_64 and ARM64 Windows.

To build the latest checkout from source with Rust 1.85 or newer instead:

```console
cargo install --git https://github.com/DCjanus/dtools --locked dtools
```

Both `sort_in_place` and `uniq_any_order` will be installed in Cargo's binary directory.

## Development

Run the complete local check suite with [just](https://github.com/casey/just):

```console
just check
```

The underlying commands only require stable Rust:

```console
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
```

## License

[MIT](LICENSE)
