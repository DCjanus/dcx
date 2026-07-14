use std::io;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use dtools::{AnyResult, sort_file, uniq_any_order};
use size::Size;

/// 文本文件和流处理工具集。
#[derive(Debug, Parser)]
#[command(about = "Tools for processing text files and streams")]
struct Command {
    #[command(subcommand)]
    subcommand: Commands,
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
}

fn main() -> AnyResult {
    match Command::parse().subcommand {
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
    }
}
