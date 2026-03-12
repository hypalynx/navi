use crate::Message;
use crate::execute;
use owo_colors::OwoColorize;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;

pub async fn prompt(version: &str, history: &mut Vec<Message>, port: u16) -> anyhow::Result<()> {
    println!(
        "navi ({}), type /help for more information and /quit or Ctrl + C to exit.",
        version
    );

    let mut rl = DefaultEditor::new()?;

    loop {
        match rl.readline("> ") {
            Ok(line) => {
                // TODO add history here
                print_user(&line);
                execute(&line, history, port).await?;
            }
            Err(ReadlineError::Interrupted) => {
                break Ok(());
            }
            Err(ReadlineError::Eof) => {
                break Ok(());
            }
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break Ok(());
            }
        }
    }
}

fn print_user(input: &str) {
    println!("{}", input.on_black().white());
}
