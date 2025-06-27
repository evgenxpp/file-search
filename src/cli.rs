use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Cli {
    #[arg(long, default_value = "D:\\tmp")]
    pub path: String,
}
