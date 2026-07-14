# dtools

`dtools` 是一组用于处理 UTF-8 文本文件和流的小型命令行工具。

## 子命令

### `dtools sort`

原地排序文本文件。命令会将整个文件读入内存，把结果写入同目录下的临时文件，再原子替换原文件；也可以在排序时去重或移除行首和行尾空白。

### `dtools uniq`

从标准输入中删除重复行，不要求输入预先排序。默认按照首次出现的顺序输出，也可以统计各唯一行的出现次数。

## 为什么不直接用 coreutils

`dtools` 不是要替代功能完整的 coreutils，而是固化几个个人常用、用现有命令组合起来不够顺手的文本处理流程：

- GNU `sort` 支持将结果写回输入文件，但其[官方文档](https://www.gnu.org/software/coreutils/sort)也提醒，原地写入在系统崩溃或发生严重 I/O 错误时可能丢失数据。`dtools sort` 会先在同目录写入唯一临时文件，同步内容并保留原权限，再替换原文件，同时可以在排序前统一处理行首和行尾空白。
- coreutils `uniq` 只识别相邻的重复行；处理不相邻重复行通常需要先排序，这会改变原始顺序。`dtools uniq` 直接对整个输入去重，默认保留首次出现顺序，也能统计全局出现次数。详见 GNU [`uniq` 文档](https://www.gnu.org/software/coreutils/uniq)。

这些便利建立在将输入保存在内存中的前提上，因此它更适合有明确大小边界的个人数据处理，不适合作为 coreutils 处理超大文件能力的替代品。

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
