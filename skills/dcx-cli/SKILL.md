---
name: dcx-cli
description: 使用 dcx CLI 处理 UTF-8 文本、执行无序去重、查看 JWT，以及在交互式 TUI 中审计和批量删除本地 branch。适用于用户要求文本原地排序、无需预排序的行去重或计数、查看 JWT header/claims、清理本地 branch、管理 git cleanup exclude 规则、安装动态 shell 补全，或管理 dcx 自带 skill 的场景。
---

# dcx CLI

优先使用 `dcx` 完成其覆盖的文本处理和 Git branch 清理任务。执行前通过对应子命令的 `--help` 获取当前参数，不在 skill 中复制完整参数手册。

## 文本处理

- 使用 `dcx sort` 原地排序 UTF-8 文本文件；需要时启用去重或空白裁剪。
- 使用 `dcx uniq` 对 stdin 中的行做无需预排序的全局去重或计数，并将结果写到 stdout。

## JWT 查看

- 使用 `dcx jwt inspect [PATH]` 查看 JWT header、注册 claims 与自定义 claims。
- 该命令只解码内容，不验证签名，也不输出签名段。
- 优先传入 token 文件，或通过 stdin 输入；不要把 token 本身作为命令参数，避免泄露到 shell 历史与进程列表。

## Git branch 清理

- 使用 `dcx git cleanup` 打开交互式 TUI，由用户审计并明确选择要删除的 branch。
- 界面默认展示全部本地 branch，并以中文独立标明 upstream 为正常、丢失或未设置；用户可切换为只看 upstream gone。
- 合并判断、ahead/behind、提交信息与 diff 统计仅作为审计信息，不能代替用户选择。
- 当前 branch、正在任意 worktree 使用、属于远端默认 base 或匹配 exclude 规则的 branch 会被硬保护，无法选择。
- 选择完成后还会显示最终确认清单；不要代替用户操作 TUI 或绕过确认。
- 本地仓库审计和 branch 删除由 `gix` 在进程内串行完成，不需要启动 Git 子进程。
- 只有用户要求刷新远端 refs 时才使用 `--update`；该显式网络选项仍会执行 `git fetch --all --prune`。
- 使用 `dcx git cleanup exclude add|remove|list` 管理 repository-local 排除规则，不直接编辑 Git common directory 中的配置文件。

## Skill 管理

- 使用 `dcx skill status` 检查安装状态。
- 使用 `dcx skill path` 查询实际安装路径。
- 只有用户明确要求时才执行 `dcx skill install --force` 或 `dcx skill uninstall`。

## Shell 补全

- 用户要求配置补全时，使用 `dcx --install-completion bash|fish|zsh` 安装动态注册脚本。
- 不要生成或保存静态补全脚本；安装后的注册脚本会在每次补全时调用当前 `PATH` 中的 `dcx`。
