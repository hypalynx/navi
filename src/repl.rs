use crate::Message;
use crate::execute;
use owo_colors::OwoColorize;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::{Cmd, Completer, Editor, EventHandler, Helper, Hinter, Validator};
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Helper, Completer, Hinter, Validator)]
struct ReplHelper(Arc<AtomicBool>);

impl Highlighter for ReplHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> Cow<'b, str> {
        if self.0.load(Ordering::Relaxed) {
            Cow::Owned(format!("\x1b[35m{}\x1b[0m", prompt))
        } else {
            Cow::Borrowed(prompt)
        }
    }
}

pub async fn prompt(version: &str, history: &mut Vec<Message>, port: u16) -> anyhow::Result<()> {
    println!(
        "navi ({}), Ctrl+T to toggle thinking, Ctrl+C to exit.",
        version
    );

    let thinking_enabled = Arc::new(AtomicBool::new(false));
    let helper = ReplHelper(thinking_enabled.clone());
    let mut rl = Editor::new()?;
    rl.set_helper(Some(helper));

    let history_file = if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".navi_history")
    } else {
        PathBuf::from(".navi_history")
    };
    let _ = rl.load_history(&history_file);

    struct ToggleHandler(Arc<AtomicBool>);
    impl rustyline::ConditionalEventHandler for ToggleHandler {
        fn handle(
            &self,
            _evt: &rustyline::Event,
            _n: rustyline::RepeatCount,
            _positive: bool,
            _ctx: &rustyline::EventContext,
        ) -> Option<Cmd> {
            self.0.fetch_xor(true, Ordering::Relaxed);
            Some(Cmd::Repaint)
        }
    }

    rl.bind_sequence(
        rustyline::KeyEvent(rustyline::KeyCode::Char('T'), rustyline::Modifiers::CTRL),
        EventHandler::Conditional(Box::new(ToggleHandler(thinking_enabled.clone()))),
    );
    rl.bind_sequence(
        rustyline::KeyEvent(rustyline::KeyCode::Char('J'), rustyline::Modifiers::CTRL),
        Cmd::Newline,
    );

    let result = loop {
        match rl.readline("> ") {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                rl.add_history_entry(line)?;
                print_user(line);
                execute(
                    line,
                    history,
                    port,
                    thinking_enabled.load(Ordering::Relaxed),
                )
                .await?;
            }
            Err(ReadlineError::Interrupted) => break Ok(()),
            Err(ReadlineError::Eof) => break Ok(()),
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break Ok(());
            }
        }
    };

    let _ = rl.save_history(&history_file);
    result
}

fn print_user(input: &str) {
    println!("{}", input.on_black().white());
}
