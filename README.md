# dcx

[![CI](https://github.com/DCjanus/dcx/actions/workflows/ci.yml/badge.svg?event=pull_request)](https://github.com/DCjanus/dcx/actions/workflows/ci.yml)
[![依赖状态](https://deps.rs/repo/github/DCjanus/dcx/status.svg)](https://deps.rs/repo/github/DCjanus/dcx)
[![许可证](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

`dcx` 是一组用于处理 UTF-8 文本、维护 Git 仓库和辅助 Coding Agent 的小型命令行工具。

名称保持为三个字符，便于在 QWERTY 键盘上仅用左手快速输入。

## 子命令

### `dcx ssh public-key`

从 OpenSSH 私钥提取公钥，输出到 stdout：

```console
dcx ssh public-key ~/.ssh/id_ed25519
dcx ssh public-key ~/.ssh/id_ed25519 > ~/.ssh/id_ed25519.pub
```

省略路径或使用 `-` 时从 stdin 读取。支持 Ed25519、RSA 和 ECDSA 等 OpenSSH 私钥，使用纯 Rust 实现，无需安装或调用 `ssh-keygen`。直接读取文件中包含的公钥，仅检查容器结构与公钥格式，不解析或验证私钥数据，因此加密私钥也无需口令。输出仅包含算法与 Base64 数据，省略注释。暂不支持旧式 PEM、PKCS#8 或 PuTTY 私钥格式。

### `dcx feed`

在交互式 TUI 中阅读本地 RSS 或 Atom 文件：

```console
dcx feed articles.atom
```

使用 `dcx feed -` 从 stdin 读取订阅内容。界面显示条目列表、标题、日期、原文链接与正文；HTML/XHTML 保留强调、链接和代码样式，明确声明为 Markdown 的内容支持排版、表格和代码语法高亮，其余内容按纯文本显示。右栏标明格式及正文或摘要来源，按 `r` 切换富文本与原始正文。保留订阅中的条目顺序，正文缺失时显示摘要。命令只读取输入，不抓取原文或保存阅读状态。

使用 `↑` / `↓` 或 `j` / `k` 选择条目，`Enter` / `Tab` 切换到正文后用相同按键滚动；`PgUp` / `PgDn` 翻动正文，`Home` / `End` 跳到正文开头或末尾。按 `/` 搜索标题与正文，`Enter` 结束输入，`Esc` 返回列表或清除筛选，`q` 退出。宽终端使用双栏，窄终端在列表与正文之间切换。

### `dcx sort`

用于就地排序文本文件，省去手动管理输入文件、临时文件和结果替换的步骤。

### `dcx uniq`

用于不依赖预排序的全局去重，避免经典 `sort | uniq` 流程中额外的排序开销。

### `dcx git branch cleanup`

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

需要刷新远端 refs 时显式使用 `dcx git branch cleanup --update`；只有这个显式网络操作仍会调用 `git fetch --all --prune`。使用 `dcx git branch cleanup exclude add|remove|list` 管理仓库级硬保护规则。

远端默认分支引用 `refs/remotes/<remote>/HEAD` 指向已不存在的分支时，会跳过该失效基准，继续使用其它有效的远端默认分支和当前 branch 的 upstream，不修改 Git 引用。没有可用基准时仍可审计 branch，但不会将其判定为已合并或等价，详情会提示“没有可用的审计基准”。

> **Breaking change：** 原 `dcx git cleanup` 已移除，不提供兼容 alias；请改用 `dcx git branch cleanup`。现有 exclude 规则继续生效，管理命令迁移为 `dcx git branch cleanup exclude add|remove|list`。

### `dcx git worktree cleanup`

用于在交互式 TUI 中审计并删除 linked worktree。界面会展示 main worktree 和全部已注册的 linked worktree，以及各自的路径、branch、HEAD、工作区状态、相对默认基准 branch 的吸收判断与 diff 统计。

main worktree、当前运行命令所在的 worktree、通过 `git worktree lock` 锁定的 worktree，以及存在已跟踪或未跟踪改动的有效 worktree 会显示为锁定状态，不能选择。目录已经丢失，或目录仍在但 `.git` 文件丢失的 worktree 会标明登记已失效；选中并确认后仅清理对应注册信息，保留残留目录和文件。不会自动运行全局 `git worktree prune`，未选中的登记不会被清理。

确认页会逐项标明“仅清理登记”或“删除目录与登记”。清理前会重新审计保护状态；有效 worktree 默认执行 `git worktree remove`，包含已初始化 submodule 时可在 TUI 中勾选强制删除，改用 `git worktree remove --force`。强制模式不会绕过上述硬保护，也不会删除失效登记对应的残留目录。对应的 local branch 均会保留，可再使用 `dcx git branch cleanup` 独立审计和删除。

操作方式与 branch cleanup 一致：使用方向键移动、空格选择、`a` 全选可删除项、`x` 清空选择；按 `f` 勾选或取消强制删除。按回车查看最终删除清单，再次按下回车才执行。

> **Breaking change：** 已移除 `dcx cargo` 子命令；构建缓存维护请改用 mbx。

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

预编译包覆盖 x86_64 Linux、Apple Silicon macOS，以及 x86_64 与 ARM64 Windows。Intel macOS 用户请使用下方的源码安装方式。项目只维护持续移动的 `latest` Release，重复安装时需要使用 `--force` 获取最新构建。

如需从源码安装，请使用最新的 Rust stable toolchain：

```console
cargo +stable install --force --git https://github.com/DCjanus/dcx --locked dcx
```

`dcx` 会安装到 Cargo 的二进制目录。

## 开发

使用 [just](https://github.com/casey/just) 运行完整的本地检查：

```console
just check
```

从当前 checkout 安装时运行 `just install`，或直接运行 `cargo +stable install --path . --force --locked`。若本机 rustup 默认工具链为 stable，也可以省略 `+stable`。

在 macOS 上从源码安装前，需运行 `rustup component add llvm-tools --toolchain stable`，供发布构建裁剪二进制使用。

底层命令使用最新的 Rust stable toolchain：

```console
cargo +stable machete
cargo +stable fmt --all -- --check
cargo +stable clippy --locked --all-targets -- -D warnings
cargo +stable test --locked --all-targets
```

## 许可证

[MIT](LICENSE)
