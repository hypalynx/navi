use crate::Message;
use crate::execute;
use owo_colors::OwoColorize;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::{Cmd, Editor, EventHandler, Helper, Hinter, Validator};
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Helper, Hinter, Validator)]
struct ReplHelper(Arc<AtomicBool>);

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        _pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        // Extract the last word from the line for completion
        let last_word_start = line
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        let prefix = &line[last_word_start..];

        let matches = find_matching_files(prefix);
        Ok((last_word_start, matches))
    }
}

fn find_matching_files(prefix: &str) -> Vec<Pair> {
    let mut matches = Vec::new();
    find_files_recursive(".", prefix, &mut matches);

    // Sort by display name for consistent order
    matches.sort_by(|a, b| a.display.cmp(&b.display));
    matches
}

fn find_files_recursive(dir: &str, prefix: &str, matches: &mut Vec<Pair>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata()
                && let Some(path) = entry.file_name().to_str()
            {
                let full_path = if dir == "." {
                    format!("./{}", path)
                } else {
                    format!("{}/{}", dir, path)
                };

                // Check if filename or full path matches prefix
                if path.starts_with(prefix) || full_path.contains(prefix) {
                    matches.push(Pair {
                        display: full_path.clone(),
                        replacement: full_path.clone(),
                    });
                }

                // Recursively search subdirectories, but limit depth and skip common folders
                if metadata.is_dir() && should_recurse(&full_path) {
                    find_files_recursive(&full_path, prefix, matches);
                }
            }
        }
    }
}

fn should_recurse(path: &str) -> bool {
    // Skip these directories to avoid slow searches
    ![
        "target",
        ".git",
        "node_modules",
        ".venv",
        "venv",
        ".next",
        "dist",
        "build",
        ".idea",
    ]
    .iter()
    .any(|dir| path.contains(dir))
}

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
    let context_usage = Arc::new(AtomicUsize::new(0));
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
                let should_continue = execute(
                    line,
                    history,
                    port,
                    thinking_enabled.load(Ordering::Relaxed),
                    context_usage.clone(),
                )
                .await?;

                // Show context usage
                let usage = context_usage.load(Ordering::Relaxed);
                let percentage = (usage as f64 / 64_000.0 * 100.0) as usize;
                if percentage >= 75 {
                    println!(
                        "{}",
                        format!("\n[Context: {} tokens ({}%)]", usage, percentage).yellow()
                    );
                }

                if !should_continue {
                    break Ok(());
                }
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
