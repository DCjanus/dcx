use std::io;

use clap::Parser;

use dtools::{AnyResult, uniq_any_order};

#[derive(Debug, Parser)]
struct Command {
    /// prefix lines by the number of occurrences
    #[arg(short, long)]
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
