# dtools

`dtools` 是一组用于处理 UTF-8 文本文件和流的小型命令行工具。

## 子命令

### `dtools sort`

用于就地排序文本文件，省去手动管理输入文件、临时文件和结果替换的步骤。

### `dtools uniq`

用于不依赖预排序的全局去重，避免经典 `sort | uniq` 流程中额外的排序开销。

## 安装

推荐使用 [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) 安装。它会从本仓库持续更新的 `latest` Release 下载预编译二进制，无需本地编译：

```console
cargo binstall --force --git https://github.com/DCjanus/dtools dtools
```

预编译包覆盖 x86_64 Linux、Intel 与 Apple Silicon macOS，以及 x86_64 与 ARM64 Windows。项目只维护持续移动的 `latest` Release，重复安装时需要使用 `--force` 获取最新构建。

如需从源码安装，请使用 Rust 1.85 或更高版本：

```console
cargo install --force --git https://github.com/DCjanus/dtools --locked dtools
```

`dtools` 会安装到 Cargo 的二进制目录。

## 开发

使用 [just](https://github.com/casey/just) 运行完整的本地检查：

```console
just check
```

底层命令只依赖 stable Rust：

```console
cargo machete
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
```

## 许可证

[MIT](LICENSE)
