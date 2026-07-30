# dcx

`dcx` 是一组用于处理 UTF-8 文本、维护 Git 仓库和辅助 Coding Agent 的小型命令行工具。

名称保持为三个字符，便于在 QWERTY 键盘上仅用左手快速输入。

## 子命令

### `dcx sort`

用于就地排序文本文件，省去手动管理输入文件、临时文件和结果替换的步骤。

### `dcx uniq`

用于不依赖预排序的全局去重，避免经典 `sort | uniq` 流程中额外的排序开销。

### `dcx git trim`

用于清理远端 upstream 已消失、且内容已经合入目标分支的本地 tracking branch，并保护当前分支、base、worktree 与仓库级 exclude 规则。该子命令需要 Git 2.38 或更高版本。

### `dcx skill`

用于把项目自带的 `dcx-cli` skill 安装到 Agent skills 目录。首次安装需要显式执行，之后 skill 会随实际运行的 `dcx` 自动保持同步。

### 动态补全

使用 `dcx --install-completion bash|fish|zsh` 一键安装 shell 注册脚本。补全候选会在使用时由当前 `PATH` 中的 `dcx` 动态生成，不会安装需要随版本更新的静态补全脚本。

## 安装

推荐使用 [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) 安装。它会从本仓库持续更新的 `latest` Release 下载预编译二进制，无需本地编译：

```console
cargo binstall --force --git https://github.com/DCjanus/dcx dcx
```

预编译包覆盖 x86_64 Linux、Intel 与 Apple Silicon macOS，以及 x86_64 与 ARM64 Windows。项目只维护持续移动的 `latest` Release，重复安装时需要使用 `--force` 获取最新构建。

如需从源码安装，请使用 Rust 1.85 或更高版本：

```console
cargo install --force --git https://github.com/DCjanus/dcx --locked dcx
```

`dcx` 会安装到 Cargo 的二进制目录。

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
