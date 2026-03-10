use clap::Parser;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

#[derive(Parser)]
#[command(name = "navi")]
struct Cli {
    #[arg(short, long)]
    exec: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if let Some(cmd) = cli.exec {
        execute(&cmd)?;
        return Ok(());
    }

    println!(
        "navi ({}), type /help for more information and /quit or Ctrl + C to exit.",
        env!("CARGO_PKG_VERSION")
    );

    let mut rl = DefaultEditor::new()?;

    loop {
        match rl.readline("> ") {
            Ok(line) => {
                // TODO add history here
                execute(&line)?;
            }
            Err(ReadlineError::Interrupted) => {
                break;
            }
            Err(ReadlineError::Eof) => {
                break;
            }
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            }
        }
    }

    Ok(())
}

fn execute(input: &str) -> anyhow::Result<()> {
    println!("Got: {}", input);
    Ok(())
}
