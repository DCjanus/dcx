use std::io;
use std::path::PathBuf;

use clap::{Args, CommandFactory, Parser, Subcommand};
use dcx::{AnyResult, cargo, completion, git, jwt, skill, sort_file, uniq_any_order};
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
    /// 管理 Cargo 项目。
    #[command(about = "Manage Cargo projects")]
    Cargo {
        #[command(subcommand)]
        subcommand: CargoCommands,
    },
    /// 查看 JSON Web Token 的内容。
    #[command(about = "Inspect JSON Web Tokens")]
    Jwt {
        #[command(subcommand)]
        subcommand: JwtCommands,
    },
    /// 管理 dcx 自带的 Agent skill。
    #[command(about = "Manage the bundled dcx Agent skill")]
    Skill {
        #[command(subcommand)]
        subcommand: SkillCommands,
    },
}

#[derive(Debug, Subcommand)]
enum GitCommands {
    /// 在交互式 TUI 中审计并批量删除本地分支。
    #[command(about = "Audit and delete local branches in an interactive TUI")]
    Cleanup(CleanupArguments),
}

#[derive(Debug, Subcommand)]
enum CargoCommands {
    /// 审计并清理当前 workspace 的低复用概率构建缓存。
    #[command(about = "Audit and clean stale build cache in the current workspace")]
    Cleanup(CargoCleanupArguments),
}

#[derive(Debug, Subcommand)]
enum JwtCommands {
    /// 解码 JWT header 与 claims，但不验证签名。
    #[command(about = "Decode a JWT without verifying its signature")]
    Inspect {
        /// JWT 文件；省略或使用 - 时从 stdin 读取
        #[arg(help = "JWT file, or - to read from standard input")]
        path: Option<PathBuf>,
    },
}

#[derive(Debug, Args)]
struct CleanupArguments {
    /// 删除前更新并 prune 远端 refs
    #[arg(long, help = "Fetch and prune all remotes before checking branches")]
    update: bool,
    #[command(subcommand)]
    subcommand: Option<CleanupCommands>,
}

#[derive(Debug, Args)]
struct CargoCleanupArguments {
    /// 将超过此天数未修改的 incremental cache 视为过期
    #[arg(
        long,
        default_value_t = 30,
        value_name = "DAYS",
        help = "Treat incremental caches older than this many days as stale"
    )]
    days: u64,
    /// 只展示候选项，不删除文件
    #[arg(
        long,
        conflicts_with = "yes",
        help = "Preview cleanup without removing files"
    )]
    dry_run: bool,
    /// 跳过交互选择并删除全部候选项
    #[arg(
        short,
        long,
        conflicts_with = "dry_run",
        help = "Remove every reported candidate without opening the selector"
    )]
    yes: bool,
}

#[derive(Debug, Subcommand)]
enum CleanupCommands {
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
    #[command(about = "Install or repair the bundled dcx skill")]
    Install {
        /// 备份并接管非托管的同名 skill
        #[arg(long, help = "Back up and replace an unmanaged skill directory")]
        force: bool,
    },
    /// 查看安装状态。
    #[command(about = "Show whether the bundled dcx skill is current")]
    Status,
    /// 输出实际安装路径。
    #[command(about = "Print the resolved installation path")]
    Path,
    /// 卸载由 dcx 管理的文件。
    #[command(about = "Remove files managed by dcx from the installed skill")]
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
    if !matches!(command, Commands::Skill { .. })
        && let Err(error) = skill::auto_update()
    {
        eprintln!("warning: failed to update dcx-cli skill: {error:#}");
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
            subcommand: GitCommands::Cleanup(arguments),
        } => match arguments.subcommand {
            Some(CleanupCommands::Exclude { subcommand }) => match subcommand {
                ExcludeCommands::Add { patterns, current } => git::exclude_add(&patterns, current),
                ExcludeCommands::Remove { patterns } => git::exclude_remove(&patterns),
                ExcludeCommands::List => git::exclude_list(),
            },
            None => git::cleanup(arguments.update),
        },
        Commands::Cargo {
            subcommand: CargoCommands::Cleanup(arguments),
        } => cargo::cleanup(arguments.days, arguments.dry_run, arguments.yes),
        Commands::Jwt {
            subcommand: JwtCommands::Inspect { path },
        } => match path.as_deref() {
            Some(path) if path.as_os_str() != "-" => {
                if path
                    .to_str()
                    .is_some_and(|value| value.split('.').count() == 3)
                {
                    anyhow::bail!(
                        "expected a JWT file path, not a token; use standard input to avoid exposing the token"
                    );
                }
                let file = std::fs::File::open(path).map_err(|error| {
                    anyhow::anyhow!("failed to open {}: {error}", path.display())
                })?;
                jwt::inspect(file, io::BufWriter::new(io::stdout()))
            }
            _ => jwt::inspect(io::stdin().lock(), io::BufWriter::new(io::stdout())),
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
