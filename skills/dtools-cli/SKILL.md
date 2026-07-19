---
name: dtools-cli
description: 使用 dtools CLI 处理 UTF-8 文本、执行无序去重，以及安全清理 Git 仓库中远端 upstream 已消失且内容已合入目标分支的本地 branch。适用于用户要求文本原地排序、无需预排序的行去重或计数、清理已合并 branch、管理 git trim exclude 规则、安装动态 shell 补全，或管理 dtools 自带 skill 的场景。
---

# dtools CLI

优先使用 `dtools` 完成其覆盖的文本处理和 Git branch 清理任务。执行前通过对应子命令的 `--help` 获取当前参数，不在 skill 中复制完整参数手册。

## 文本处理

- 使用 `dtools sort` 原地排序 UTF-8 文本文件；需要时启用去重或空白裁剪。
- 使用 `dtools uniq` 对 stdin 中的行做无需预排序的全局去重或计数，并将结果写到 stdout。

## Git branch 清理

- 使用 `dtools git trim --dry-run` 预览候选结果。
- 仅把 upstream 已为 gone、且内容已被某个 base 吸收的本地 tracking branch 视为删除候选。
- upstream 仍存在、没有 upstream、仍含独有内容、正在任意 worktree 使用、属于 base 或匹配 exclude 规则的 branch 必须保留。
- 真正删除前，向用户展示候选 branch 并取得明确确认。
- 除非用户明确授权，否则不要使用 `--yes`。
- 只有用户要求刷新远端 refs 时才使用 `--update`；该选项会执行 fetch/prune。
- 使用 `dtools git trim exclude add|remove|list` 管理 repository-local 排除规则，不直接编辑 Git common directory 中的配置文件。

## Skill 管理

- 使用 `dtools skill status` 检查安装状态。
- 使用 `dtools skill path` 查询实际安装路径。
- 只有用户明确要求时才执行 `dtools skill install --force` 或 `dtools skill uninstall`。

## Shell 补全

- 用户要求配置补全时，使用 `dtools --install-completion bash|fish|zsh` 安装动态注册脚本。
- 不要生成或保存静态补全脚本；安装后的注册脚本会在每次补全时调用当前 `PATH` 中的 `dtools`。
