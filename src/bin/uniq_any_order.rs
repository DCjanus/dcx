use std::io;

use clap::Parser;

use dtools::{AnyResult, uniq_any_order};

/// 无需预先排序即可对输入行去重或计数。
#[derive(Debug, Parser)]
#[command(about = "Deduplicate or count input lines without sorting first")]
struct Command {
    /// 在每行前输出出现次数
    #[arg(short, long, help = "Prefix each line with its occurrence count")]
    count: bool,
}

fn main() -> AnyResult {
    let command = Command::parse();
    uniq_any_order(
        io::stdin().lock(),
        io::BufWriter::new(io::stdout()),
        command.count,
    )?;
    Ok(())
}
