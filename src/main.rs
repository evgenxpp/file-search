mod cli;
mod search;
mod shell;

use std::{error::Error, path::Path};

use clap::Parser;

use crate::{cli::Cli, search::FileSearch, shell::Shell};

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let path = Path::new(&cli.path);
    let searcher = FileSearch::create(path)?;
    let mut stdin_handler = Shell::new(searcher);

    stdin_handler.watch();

    Ok(())
}
