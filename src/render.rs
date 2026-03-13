use std::io::Write;

enum Mode {
    Normal,
    CodeBlock { lang: String },
}

pub struct Renderer<T: Write> {
    writer: T,
    mode: Mode,
    buffer: String, // Accumulates SSE chunks until we have complete lines
    width: usize,
    needs_spacing: bool,
    had_thinking: bool,
}

impl<T: Write> Renderer<T> {
    pub fn new(width: usize, writer: T) -> Self {
        Self {
            writer,
            mode: Mode::Normal,
            buffer: String::new(),
            width,
            needs_spacing: false,
            had_thinking: false,
        }
    }

    pub fn set_had_thinking(&mut self, had_thinking: bool) {
        self.had_thinking = had_thinking;
    }

    pub fn push(&mut self, token: &str) {
        self.buffer.push_str(token);

        while let Some(newline_pos) = self.buffer.find('\n') {
            let line_str = self.buffer[..newline_pos].to_string();
            let rest = self.buffer[newline_pos + 1..].to_string();
            self.buffer = rest;
            self.process_line(&line_str);
        }
    }

    pub fn flush(&mut self) {
        if !self.buffer.is_empty() {
            let line = self.buffer.clone();
            self.process_line(&line);
            self.buffer.clear();
        }
    }

    fn process_line(&mut self, line: &str) {
        let in_code_block = match &self.mode {
            Mode::CodeBlock { lang } => Some(lang.clone()),
            Mode::Normal => None,
        };

        let next_mode = if let Some(lang) = in_code_block {
            if line.trim_start().starts_with("```") {
                // end of code block, add new line and go back to normal
                let _ = writeln!(self.writer);
                Mode::Normal
            } else {
                let _ = writeln!(self.writer, "\x1b[2m{}\x1b[0m", line);
                Mode::CodeBlock { lang }
            }
        } else {
            if line.trim_start().starts_with("```") {
                if self.needs_spacing && !line.is_empty() {
                    let _ = writeln!(self.writer);
                }
                let lang = line
                    .trim_start()
                    .strip_prefix("```")
                    .unwrap_or("")
                    .trim()
                    .to_string();
                self.needs_spacing = false;
                Mode::CodeBlock { lang }
            } else if line.starts_with("#") {
                self.render_heading(line);
                self.needs_spacing = true;
                Mode::Normal
            } else if !line.is_empty() {
                if self.had_thinking {
                    let _ = writeln!(self.writer);
                    self.had_thinking = false;
                }
                self.render_text(line);
                self.needs_spacing = true;
                Mode::Normal
            } else {
                Mode::Normal
            }
        };
        self.mode = next_mode;
    }

    fn render_heading(&mut self, line: &str) {
        let trimmed = line.trim_start_matches('#').trim();
        let _ = writeln!(self.writer, "\x1b[1m{}\x1b[0m", trimmed);
        let _ = writeln!(self.writer);
    }

    fn render_text(&mut self, line: &str) {
        let text = self.parse_inline_markdown(line);
        self.word_wrap_and_print(&text);
    }

    fn parse_inline_markdown(&self, text: &str) -> String {
        let mut result = String::new();
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '*' {
                if chars.peek() == Some(&'*') {
                    chars.next();
                    result.push_str("\x1b[1m");
                    let mut inner = String::new();
                    while let Some(inner_ch) = chars.next() {
                        if inner_ch == '*' && chars.peek() == Some(&'*') {
                            chars.next();
                            break;
                        }
                        inner.push(inner_ch);
                    }
                    result.push_str(&inner);
                    result.push_str("\x1b[0m");
                } else {
                    result.push_str("\x1b[3m");
                    let mut inner = String::new();
                    for inner_ch in chars.by_ref() {
                        if inner_ch == '*' {
                            break;
                        }
                        inner.push(inner_ch);
                    }
                    result.push_str(&inner);
                    result.push_str("\x1b[0m");
                }
            } else if ch == '_' && chars.peek() == Some(&'_') {
                chars.next();
                result.push_str("\x1b[1m");
                let mut inner = String::new();
                while let Some(inner_ch) = chars.next() {
                    if inner_ch == '_' && chars.peek() == Some(&'_') {
                        chars.next();
                        break;
                    }
                    inner.push(inner_ch);
                }
                result.push_str(&inner);
                result.push_str("\x1b[0m");
            } else if ch == '`' {
                result.push_str("\x1b[1m");
                let mut inner = String::new();
                for inner_ch in chars.by_ref() {
                    if inner_ch == '`' {
                        break;
                    }
                    inner.push(inner_ch);
                }
                result.push_str(&inner);
                result.push_str("\x1b[0m");
            } else {
                result.push(ch);
            }
        }

        result
    }

    fn word_wrap_and_print(&mut self, text: &str) {
        let mut current_line = String::new();
        let mut words = Vec::new();
        let mut current_word = String::new();
        let mut in_ansi = false;

        for ch in text.chars() {
            if ch == '\x1b' {
                in_ansi = true;
                current_word.push(ch);
            } else if in_ansi {
                current_word.push(ch);
                if ch == 'm' {
                    in_ansi = false;
                }
            } else if ch.is_whitespace() {
                if !current_word.is_empty() {
                    words.push(current_word.clone());
                    current_word.clear();
                }
                if ch == '\n' {
                    words.push("\n".to_string());
                }
            } else {
                current_word.push(ch);
            }
        }
        if !current_word.is_empty() {
            words.push(current_word);
        }

        for word in words {
            if word == "\n" {
                let _ = writeln!(self.writer, "{}", current_line);
                current_line.clear();
            } else {
                let word_display_len = self.display_len(&word);
                let current_display_len = self.display_len(&current_line);

                if current_display_len == 0 {
                    current_line.push_str(&word);
                } else if current_display_len + 1 + word_display_len <= self.width {
                    current_line.push(' ');
                    current_line.push_str(&word);
                } else {
                    let _ = writeln!(self.writer, "{}", current_line);
                    current_line = word;
                }
            }
        }

        if !current_line.is_empty() {
            let _ = writeln!(self.writer, "{}", current_line);
        }
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
