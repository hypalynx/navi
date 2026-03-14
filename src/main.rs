use clap::Parser;
use navi::{create_initial_history, execute, repl};

#[derive(Parser)]
#[command(name = "navi")]
struct Cli {
    #[arg(short, long)]
    exec: Option<String>,

    #[arg(short, long)]
    version: bool,

    #[arg(short, long, default_value = "7777")]
    port: u16,
}

const NAVI_VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let mut history = create_initial_history();

    if cli.version {
        println!("navi v{}", NAVI_VERSION);
        return Ok(());
    }

    if let Some(cmd) = cli.exec {
        execute(&cmd, &mut history, cli.port, false).await?;
        return Ok(());
    }

    repl::prompt(NAVI_VERSION, &mut history, cli.port).await?;

    Ok(())
}
