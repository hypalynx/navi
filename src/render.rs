use std::io::Write;

// TODO
//
// - remove first_event from llm.rs
// - extract event handling to fn in llm.rs
// - add integration test there (for llm integration)
// - make render unit tests inside render.rs

#[derive(Copy, Clone, PartialEq)]
pub enum ContentType {
    Normal,
    Thinking,
}

enum Mode {
    Normal,
    CodeBlock,
}

pub struct Renderer<T: Write> {
    writer: T,
    mode: Mode,
    current_line_length: usize,
    width: usize,
    at_line_start: bool,
    is_heading: bool,
}

impl<T: Write> Renderer<T> {
    pub fn new(width: usize, writer: T) -> Self {
        Self {
            writer,
            mode: Mode::Normal,
            current_line_length: 0,
            width,
            at_line_start: true,
            is_heading: false,
        }
    }

    pub fn push(&mut self, token: &str, content_type: ContentType) {
        // Handle newlines
        for (i, part) in token.split('\n').enumerate() {
            if i > 0 {
                let _ = writeln!(self.writer);
                if self.is_heading {
                    let _ = writeln!(self.writer);
                    self.is_heading = false;
                }
                self.current_line_length = 0;
                self.at_line_start = true;
            }

            if !part.is_empty() {
                // At start of a logical line, check for markdown
                if self.at_line_start {
                    if part.trim_start().starts_with("```") {
                        self.mode = match &self.mode {
                            Mode::CodeBlock => Mode::Normal,
                            Mode::Normal => Mode::CodeBlock,
                        };
                    } else if part.trim_start().starts_with("#") {
                        self.is_heading = true;
                    }
                    self.at_line_start = false;
                }

                // Word wrapping: check if token fits on current line
                let part_len = self.display_len(part);
                if self.current_line_length == 0 {
                    // Start of line
                    self.print_word(part, content_type);
                    self.current_line_length = part_len;
                } else if self.current_line_length + part_len <= self.width {
                    // Fits on current line
                    self.print_word(part, content_type);
                    self.current_line_length += part_len;
                } else if part.starts_with(' ') {
                    // Token is whitespace: wrap then skip the space
                    let _ = writeln!(self.writer);
                    self.current_line_length = 0;
                    self.at_line_start = false;
                } else {
                    // Token doesn't fit: wrap line first
                    let _ = writeln!(self.writer);
                    self.print_word(part, content_type);
                    self.current_line_length = part_len;
                    self.at_line_start = false;
                }
            }
        }
    }

    pub fn flush(&mut self) {
        if self.current_line_length > 0 {
            let _ = writeln!(self.writer);
        }
    }

    fn print_word(&mut self, word: &str, word_type: ContentType) {
        let (start_code, end_code) = match (&self.mode, self.is_heading, word_type) {
            (Mode::CodeBlock, _, _) => ("\x1b[2m", "\x1b[0m"),
            (_, true, _) => ("\x1b[1m", "\x1b[0m"),
            (_, _, ContentType::Thinking) => ("\x1b[90m\x1b[3m", "\x1b[0m"),
            _ => ("", ""),
        };
        let _ = write!(self.writer, "{}{}{}", start_code, word, end_code);
        let _ = self.writer.flush();
    }

    fn display_len(&self, s: &str) -> usize {
        let mut len = 0;
        let mut in_ansi = false;

        for ch in s.chars() {
            if ch == '\x1b' {
                in_ansi = true;
            } else if in_ansi {
                if ch == 'm' {
                    in_ansi = false;
                }
            } else {
                len += 1;
            }
        }

        len
    }
}
