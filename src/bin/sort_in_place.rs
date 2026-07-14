use clap::Parser;
use dtools::{AnyResult, sort_file};
use size::Size;
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Command {
    /// Sort the file in place
    path: PathBuf,
    /// The maximum size of the file to be sorted in memory
    #[arg(long, default_value = "1GB")]
    size_limit: Size,
    /// Only output unique lines
    #[arg(short, long)]
    uniq: bool,
    /// Trim whitespace from the lines
    #[arg(short, long)]
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
