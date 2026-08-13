# dcx

`dcx` 是一组用于处理 UTF-8 文本、维护 Git 仓库和辅助 Coding Agent 的小型命令行工具。

名称保持为三个字符，便于在 QWERTY 键盘上仅用左手快速输入。

## 子命令

### `dcx sort`

用于就地排序文本文件，省去手动管理输入文件、临时文件和结果替换的步骤。

### `dcx uniq`

用于不依赖预排序的全局去重，避免经典 `sort | uniq` 流程中额外的排序开销。

### `dcx git cleanup`

用于在交互式 TUI 中审计并批量删除本地 branch。界面默认显示全部本地 branch，可按 `g` 切换为只看 upstream 已消失的 branch；左侧独立展示“正常”“丢失”或“未设”跟踪状态，以及“已合并”“等价”“待复核”或“锁定”审计状态。使用方向键移动、空格选择，右侧展示 upstream、保护状态、最后提交、相对审计 base 的领先/落后提交数、内容吸收判断与 diff 统计。按回车后会显示最终删除清单，再次回车才通过 `gix` 事务删除本地 ref。

当前 branch、其它 worktree 正在使用的 branch、远端默认 base，以及匹配 repository-local exclude 规则的 branch 会显示为锁定状态，不能选择。自动分析只提供审计信息，不会自动选择或删除 branch。仓库发现、ref/upstream/worktree 读取、提交图审计、diff 统计和本地 ref 删除均由 `gix` 在进程内串行完成；该子命令需要支持交互输入的终端。

“已合并”表示 branch tip 是审计 base 的祖先；“等价”表示 base 在共同祖先之后出现过与 branch tip 完全相同的 tree。经过 rebase、cherry-pick 或解决冲突后仅部分等价的 branch 会保守显示为“待复核”，由用户结合 diff 信息决定是否删除。

常用按键：

| 按键 | 操作 |
| --- | --- |
| `↑` / `↓`、`j` / `k` | 移动光标 |
| `Space` | 选择或取消当前 branch |
| `g` | 切换全部 branch / 仅 upstream gone |
| `a` | 选择或取消全部可见且未锁定的 branch |
| `x` | 清空选择 |
| `Enter` | 查看删除确认；确认页再次按下后执行 |
| `Esc` / `q` | 返回或退出 |

需要刷新远端 refs 时显式使用 `dcx git cleanup --update`；只有这个显式网络操作仍会调用 `git fetch --all --prune`。使用 `dcx git cleanup exclude add|remove|list` 管理仓库级硬保护规则。

> **Breaking change：** 原 `dcx git branches` 已移除，不提供兼容 alias；请改用 `dcx git cleanup`。现有 exclude 规则继续生效，管理命令迁移为 `dcx git cleanup exclude add|remove|list`。

### `dcx jwt inspect`

用于以人类可读的表格查看 JWT header 与 claims，包括 issuer、audience，以及转换为 UTC 时间的 `iat`、`nbf`、`exp`。该命令只解码内容，不验证签名，并且不会输出签名段。

推荐传入只包含 token 的文件：

```console
dcx jwt inspect /path/to/token
```

省略路径或使用 `-` 时从 stdin 读取，避免把 token 放入 shell 历史和进程参数：

```console
pbpaste | dcx jwt inspect
dcx jwt inspect - < /path/to/token
```

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

如需从源码安装，请使用最新的 Rust nightly toolchain：

```console
cargo +nightly install --force --git https://github.com/DCjanus/dcx --locked dcx
```

`dcx` 会安装到 Cargo 的二进制目录。

## 开发

使用 [just](https://github.com/casey/just) 运行完整的本地检查：

```console
just check
```

底层命令使用最新的 Rust nightly toolchain：

```console
cargo machete
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
```

## 许可证

[MIT](LICENSE)
