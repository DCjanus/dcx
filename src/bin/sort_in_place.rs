use clap::Parser;
use dtools::{AnyResult, sort_file};
use size::Size;
use std::path::PathBuf;

/// 原地排序 UTF-8 文本文件。
#[derive(Debug, Parser)]
#[command(about = "Sort a UTF-8 text file in place")]
struct Command {
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
}

fn main() -> AnyResult {
    let command = Command::parse();
    sort_file(
        &command.path,
        command.size_limit.bytes() as u64,
        command.uniq,
        command.trim_whitespace,
    )
}
