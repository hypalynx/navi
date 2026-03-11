use clap::Parser;
use owo_colors::OwoColorize;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use serde::Serialize;

#[derive(Parser)]
#[command(name = "navi")]
struct Cli {
    #[arg(short, long)]
    exec: Option<String>,

    #[arg(short, long)]
    version: bool,
}

const NAVI_VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let mut history: Vec<Message> = Vec::new();

    if cli.version {
        println!("navi v{}", NAVI_VERSION);
        return Ok(());
    }

    if let Some(cmd) = cli.exec {
        execute(&cmd, &mut history).await?;
        return Ok(());
    }

    println!(
        "navi ({}), type /help for more information and /quit or Ctrl + C to exit.",
        NAVI_VERSION
    );

    let mut rl = DefaultEditor::new()?;

    loop {
        match rl.readline("> ") {
            Ok(line) => {
                // TODO add history here
                execute(&line, &mut history).await?;
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

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
}

#[derive(Serialize, Clone)]
struct Message {
    role: String,
    content: String,
}

async fn llm_request(messages: &[Message]) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    // TODO get api_key if needed
    // TODO get hostname from config, default to localhost

    let response = client
        .post("http://127.0.0.1:7777/v1/chat/completions")
        //.header("Authorization", format!("Bearer {}", api_key))
        .json(&ChatRequest {
            model: "qwen3.5-9b".to_string(),
            messages: messages.to_vec(),
        })
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    Ok(response["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string())
}

fn print_user(input: &str) {
    println!("{}", input.on_black().white());
}

async fn execute(input: &str, history: &mut Vec<Message>) -> anyhow::Result<()> {
    print_user(input);

    history.push(Message {
        role: "user".to_string(),
        content: input.to_string(),
    });

    match llm_request(history).await {
        Ok(response) => {
            println!("{}", response);

            history.push(Message {
                role: "assistant".to_string(),
                content: response,
            });
        }
        Err(e) => eprintln!("Could not communicate with LLM: {}", e),
    };

    Ok(())
}
