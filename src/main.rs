use std::io;
use std::path::PathBuf;

use clap::{Args, CommandFactory, Parser, Subcommand};
use dtools::{AnyResult, completion, git, skill, sort_file, uniq_any_order};
use size::Size;

/// 文本处理、Git 仓库维护与 Coding Agent 辅助工具集。
#[derive(Debug, Parser)]
#[command(
    about = "Tools for text processing, Git maintenance, and Coding Agents",
    arg_required_else_help = true
)]
struct Command {
    /// 安装动态 shell 补全注册脚本
    #[arg(
        long,
        value_enum,
        value_name = "SHELL",
        help = "Install dynamic shell completion for bash, fish, or zsh"
    )]
    install_completion: Option<completion::CompletionShell>,
    #[command(subcommand)]
    subcommand: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// 原地排序 UTF-8 文本文件。
    #[command(about = "Sort a UTF-8 text file in place")]
    Sort {
        /// 要原地排序的文件
        #[arg(help = "The file to sort in place")]
        path: PathBuf,
        /// 允许在内存中排序的最大文件大小
        #[arg(
            long,
            default_value = "1GB",
            help = "Maximum file size to sort in memory"
        )]
        size_limit: Size,
        /// 只保留唯一行
        #[arg(short, long, help = "Keep only unique lines")]
        uniq: bool,
        /// 移除行首和行尾空白
        #[arg(short, long, help = "Trim leading and trailing whitespace")]
        trim_whitespace: bool,
    },
    /// 无需预先排序即可对输入行去重或计数。
    #[command(about = "Deduplicate or count input lines without sorting first")]
    Uniq {
        /// 在每行前输出出现次数
        #[arg(short, long, help = "Prefix each line with its occurrence count")]
        count: bool,
    },
    /// 管理 Git 仓库。
    #[command(about = "Manage Git repositories")]
    Git {
        #[command(subcommand)]
        subcommand: GitCommands,
    },
    /// 管理 dtools 自带的 Agent skill。
    #[command(about = "Manage the bundled dtools Agent skill")]
    Skill {
        #[command(subcommand)]
        subcommand: SkillCommands,
    },
}

#[derive(Debug, Subcommand)]
enum GitCommands {
    /// 清理 upstream 已消失且内容已经合入 base 的本地分支。
    #[command(about = "Delete obsolete local branches whose upstream is gone")]
    Trim(TrimArguments),
}

#[derive(Debug, Args)]
struct TrimArguments {
    /// 只预览，不删除分支
    #[arg(long, help = "Preview results without deleting branches")]
    dry_run: bool,
    /// 删除前更新并 prune 远端 refs
    #[arg(long, help = "Fetch and prune all remotes before checking branches")]
    update: bool,
    /// 跳过删除确认
    #[arg(short, long, help = "Delete listed branches without confirmation")]
    yes: bool,
    /// 用于判断内容是否已合入的目标分支
    #[arg(
        long,
        action = clap::ArgAction::Append,
        help = "Base branch used to determine whether changes were merged"
    )]
    base: Vec<String>,
    /// 本次运行额外排除的 branch name 或 glob
    #[arg(
        long,
        action = clap::ArgAction::Append,
        help = "Branch name or glob to exclude for this run"
    )]
    exclude: Vec<String>,
    #[command(subcommand)]
    subcommand: Option<TrimCommands>,
}

#[derive(Debug, Subcommand)]
enum TrimCommands {
    /// 管理当前仓库的持久化 exclude 规则。
    #[command(about = "Manage repository-local branch exclusion rules")]
    Exclude {
        #[command(subcommand)]
        subcommand: ExcludeCommands,
    },
}

#[derive(Debug, Subcommand)]
enum ExcludeCommands {
    /// 添加 branch name 或 glob。
    #[command(about = "Add branch names or globs to the exclusion list")]
    Add {
        /// 要添加的 branch name 或 glob
        #[arg(help = "Branch names or globs to add")]
        patterns: Vec<String>,
        /// 添加当前 branch
        #[arg(long, help = "Add the currently checked-out branch")]
        current: bool,
    },
    /// 移除 branch name 或 glob。
    #[command(about = "Remove branch names or globs from the exclusion list")]
    Remove {
        /// 要移除的 branch name 或 glob
        #[arg(required = true, help = "Branch names or globs to remove")]
        patterns: Vec<String>,
    },
    /// 列出所有规则。
    #[command(about = "List repository-local branch exclusion rules")]
    List,
}

#[derive(Debug, Subcommand)]
enum SkillCommands {
    /// 安装、修复或手动更新内置 skill。
    #[command(about = "Install or repair the bundled dtools skill")]
    Install {
        /// 备份并接管非托管的同名 skill
        #[arg(long, help = "Back up and replace an unmanaged skill directory")]
        force: bool,
    },
    /// 查看安装状态。
    #[command(about = "Show whether the bundled dtools skill is current")]
    Status,
    /// 输出实际安装路径。
    #[command(about = "Print the resolved installation path")]
    Path,
    /// 卸载由 dtools 管理的文件。
    #[command(about = "Remove files managed by dtools from the installed skill")]
    Uninstall,
}

fn main() -> AnyResult {
    clap_complete::CompleteEnv::with_factory(Command::command).complete();

    let parsed = Command::parse();
    if let Some(shell) = parsed.install_completion {
        return completion::install(shell);
    }
    let command = parsed
        .subcommand
        .expect("clap requires either a subcommand or --install-completion");
    if !matches!(command, Commands::Skill { .. }) {
        if let Err(error) = skill::auto_update() {
            eprintln!("warning: failed to update dtools-cli skill: {error:#}");
        }
    }

    match command {
        Commands::Sort {
            path,
            size_limit,
            uniq,
            trim_whitespace,
        } => sort_file(&path, size_limit.bytes() as u64, uniq, trim_whitespace),
        Commands::Uniq { count } => {
            uniq_any_order(io::stdin().lock(), io::BufWriter::new(io::stdout()), count)?;
            Ok(())
        }
        Commands::Git {
            subcommand: GitCommands::Trim(arguments),
        } => match arguments.subcommand {
            Some(TrimCommands::Exclude { subcommand }) => match subcommand {
                ExcludeCommands::Add { patterns, current } => git::exclude_add(&patterns, current),
                ExcludeCommands::Remove { patterns } => git::exclude_remove(&patterns),
                ExcludeCommands::List => git::exclude_list(),
            },
            None => git::trim(
                &arguments.base,
                &arguments.exclude,
                arguments.dry_run,
                arguments.update,
                arguments.yes,
            ),
        },
        Commands::Skill { subcommand } => match subcommand {
            SkillCommands::Install { force } => skill::install(force),
            SkillCommands::Status => {
                if !skill::status()? {
                    std::process::exit(1);
                }
                Ok(())
            }
            SkillCommands::Path => skill::print_path(),
            SkillCommands::Uninstall => skill::uninstall(),
        },
    }
}
