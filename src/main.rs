use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = boxr::cli::Cli::parse();
    let code = boxr::run_cli(cli).await?;
    std::process::exit(code);
}
