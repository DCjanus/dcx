# dtools

`dtools` 是一组用于处理 UTF-8 文本文件和流的小型命令行工具。

## 使用

### `dtools sort`

原地排序文本文件。命令会将整个文件读入内存，把结果写入同目录下的临时文件，再原子替换原文件。

```console
dtools sort [OPTIONS] <PATH>
```

参数：

- `--size-limit <SIZE>`：允许读入内存的最大文件大小，默认为 `1GB`
- `-u, --uniq`：排序后删除重复行
- `-t, --trim-whitespace`：排序前移除行首和行尾空白

```console
dtools sort --uniq --trim-whitespace words.txt
```

### `dtools uniq`

从标准输入中删除重复行，不要求输入预先排序。默认按照首次出现的顺序输出。

```console
dtools uniq [OPTIONS]
```

使用 `--count` 输出各唯一行及其出现次数；计数模式不保证输出顺序。

```console
cat words.txt | dtools uniq --count
```

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

## 从旧版迁移

项目不再提供独立的 `sort_in_place` 和 `uniq_any_order` binary。曾经安装过旧版时，先卸载旧包，再重新安装：

```console
cargo uninstall dtools
cargo binstall --force --git https://github.com/DCjanus/dtools dtools
```

命令迁移关系：

- `sort_in_place ...` 改为 `dtools sort ...`
- `uniq_any_order ...` 改为 `dtools uniq ...`

## 开发

使用 [just](https://github.com/casey/just) 运行完整的本地检查：

```console
just check
```

底层命令只依赖 stable Rust：

```console
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
```

## 许可证

[MIT](LICENSE)
