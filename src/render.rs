use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use std::io::Write;

enum Mode {
    Normal,
    CodeBlock { lang: String, buf: String },
}

pub struct Renderer {
    mode: Mode,
    line_buf: String,
    width: usize,
    ss: SyntaxSet,
    ts: ThemeSet,
    needs_spacing: bool, // track if we should add spacing before next section
    had_thinking: bool,  // track if we just printed thinking output
}

impl Renderer {
    pub fn new(width: usize) -> Self {
        Self {
            mode: Mode::Normal,
            line_buf: String::new(),
            width,
            ss: SyntaxSet::load_defaults_nonewlines(),
            ts: ThemeSet::load_defaults(),
            needs_spacing: false,
            had_thinking: false,
        }
    }

    pub fn set_had_thinking(&mut self, had_thinking: bool) {
        self.had_thinking = had_thinking;
    }

    pub fn push(&mut self, token: &str) {
        self.line_buf.push_str(token);

        // Process complete lines
        loop {
            match self.line_buf.find('\n') {
                Some(newline_pos) => {
                    let line_str = self.line_buf[..newline_pos].to_string();
                    let rest = self.line_buf[newline_pos + 1..].to_string();
                    self.line_buf = rest;
                    self.process_line(&line_str);
                }
                None => break,
            }
        }
    }

    pub fn flush(&mut self) {
        // Process remaining content in line buffer
        if !self.line_buf.is_empty() {
            let line = self.line_buf.clone();
            self.process_line(&line);
            self.line_buf.clear();
        }

        // If in code block mode, highlight and print remaining buffer
        if let Mode::CodeBlock { lang, buf } = &self.mode {
            if !buf.is_empty() {
                self.highlight_code_block(lang, buf);
            }
        }
    }

    fn process_line(&mut self, line: &str) {
        let next_mode = match &self.mode {
            Mode::CodeBlock { lang, buf } => {
                if line.trim_start().starts_with("```") {
                    // End of code block
                    self.highlight_code_block(lang, buf);
                    println!(); // blank line after code block
                    self.needs_spacing = true;
                    Mode::Normal
                } else {
                    // Accumulate code
                    let mut new_buf = buf.clone();
                    new_buf.push_str(line);
                    new_buf.push('\n');
                    Mode::CodeBlock {
                        lang: lang.clone(),
                        buf: new_buf,
                    }
                }
            }
            Mode::Normal => {
                if line.trim_start().starts_with("```") {
                    // Start of code block - add spacing before if needed
                    if self.needs_spacing && !line.is_empty() {
                        println!();
                    }
                    let lang = line
                        .trim_start()
                        .strip_prefix("```")
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    self.needs_spacing = false;
                    Mode::CodeBlock {
                        lang,
                        buf: String::new(),
                    }
                } else if line.starts_with("#") {
                    // Heading
                    self.render_heading(line);
                    self.needs_spacing = true;
                    Mode::Normal
                } else if !line.is_empty() {
                    // Add spacing before first content if we had thinking
                    if self.had_thinking {
                        println!();
                        self.had_thinking = false;
                    }
                    // Normal text with inline markdown
                    self.render_text(line);
                    self.needs_spacing = true;
                    Mode::Normal
                } else {
                    Mode::Normal
                }
            }
        };
        self.mode = next_mode;
    }

    fn render_heading(&self, line: &str) {
        let trimmed = line.trim_start_matches('#').trim();
        println!("\x1b[1m{}\x1b[0m", trimmed);
        println!(); // blank line after heading
    }

    fn render_text(&self, line: &str) {
        let text = self.parse_inline_markdown(line);
        self.word_wrap_and_print(&text);
    }

    fn parse_inline_markdown(&self, text: &str) -> String {
        let mut result = String::new();
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '*' {
                // Check for bold (**) or italic (*)
                if chars.peek() == Some(&'*') {
                    chars.next(); // consume second *
                    result.push_str("\x1b[1m"); // bold
                    let mut inner = String::new();
                    while let Some(inner_ch) = chars.next() {
                        if inner_ch == '*' && chars.peek() == Some(&'*') {
                            chars.next(); // consume second *
                            break;
                        }
                        inner.push(inner_ch);
                    }
                    result.push_str(&inner);
                    result.push_str("\x1b[0m");
                } else {
                    // italic
                    result.push_str("\x1b[3m");
                    let mut inner = String::new();
                    while let Some(inner_ch) = chars.next() {
                        if inner_ch == '*' {
                            break;
                        }
                        inner.push(inner_ch);
                    }
                    result.push_str(&inner);
                    result.push_str("\x1b[0m");
                }
            } else if ch == '_' && chars.peek() == Some(&'_') {
                // bold with __
                chars.next(); // consume second _
                result.push_str("\x1b[1m");
                let mut inner = String::new();
                while let Some(inner_ch) = chars.next() {
                    if inner_ch == '_' && chars.peek() == Some(&'_') {
                        chars.next(); // consume second _
                        break;
                    }
                    inner.push(inner_ch);
                }
                result.push_str(&inner);
                result.push_str("\x1b[0m");
            } else if ch == '`' {
                // inline code
                result.push_str("\x1b[1m");
                let mut inner = String::new();
                while let Some(inner_ch) = chars.next() {
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

    fn word_wrap_and_print(&self, text: &str) {
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
                println!("{}", current_line);
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
                    println!("{}", current_line);
                    current_line = word;
                }
            }
        }

        if !current_line.is_empty() {
            println!("{}", current_line);
        }
    }

    fn display_len(&self, s: &str) -> usize {
        // Count visible characters, excluding ANSI escape sequences
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

    fn highlight_code_block(&self, lang: &str, code: &str) {
        // Try to syntax highlight, fall back to plain if lang not found
        let syntax = self.ss.find_syntax_by_token(lang);

        if let Some(syntax) = syntax {
            let mut highlighter = HighlightLines::new(syntax, &self.ts.themes["base16-ocean.dark"]);
            for line in code.lines() {
                if let Ok(highlighted) = highlighter.highlight_line(line, &self.ss) {
                    // Print ANSI colored output directly
                    for (style, text) in highlighted {
                        let fg = style.foreground;
                        let ansi_code = format!(
                            "\x1b[38;2;{};{};{}m",
                            fg.r, fg.g, fg.b
                        );
                        print!("{}{}", ansi_code, text);
                    }
                    println!("\x1b[0m");
                }
            }
        } else {
            // Fallback: print plain text
            print!("{}", code);
        }
        let _ = std::io::stdout().flush();
    }
}
