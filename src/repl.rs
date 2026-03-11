use crate::Message;
use crate::execute;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;

pub async fn prompt(version: &str, history: &mut Vec<Message>) -> anyhow::Result<()> {
    println!(
        "navi ({}), type /help for more information and /quit or Ctrl + C to exit.",
        version
    );

    let mut rl = DefaultEditor::new()?;

    loop {
        match rl.readline("> ") {
            Ok(line) => {
                // TODO add history here
                execute(&line, history).await?;
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
