use std::io::Write;

enum Mode {
    Normal,
    CodeBlock { lang: String },
}

#[derive(Copy, Clone, PartialEq)]
pub enum ContentType {
    Normal,
    Thinking,
}

#[derive(Clone)]
struct BufferChunk {
    content: String,
    content_type: ContentType,
}

pub struct Renderer<T: Write> {
    writer: T,
    mode: Mode,
    buffer: Vec<BufferChunk>,
    width: usize,
    needs_spacing: bool,
    had_thinking: bool,
}

impl<T: Write> Renderer<T> {
    pub fn new(width: usize, writer: T) -> Self {
        Self {
            writer,
            mode: Mode::Normal,
            buffer: Vec::new(),
            width,
            needs_spacing: false,
            had_thinking: false,
        }
    }

    pub fn set_had_thinking(&mut self, had_thinking: bool) {
        self.had_thinking = had_thinking;
    }

    pub fn push(&mut self, token: &str, content_type: ContentType) {
        // Add to last chunk if same type, otherwise create new chunk
        if let Some(last_chunk) = self.buffer.last_mut() {
            if last_chunk.content_type == content_type {
                last_chunk.content.push_str(token);
            } else {
                self.buffer.push(BufferChunk {
                    content: token.to_string(),
                    content_type,
                });
            }
        } else {
            self.buffer.push(BufferChunk {
                content: token.to_string(),
                content_type,
            });
        }

        // Process complete lines
        while let Some(newline_pos) = self.find_newline() {
            let line_chunks = self.extract_line(newline_pos);
            self.process_line(line_chunks);
        }
    }

    fn find_newline(&self) -> Option<usize> {
        let mut pos = 0;
        for chunk in &self.buffer {
            for ch in chunk.content.chars() {
                if ch == '\n' {
                    return Some(pos);
                }
                pos += 1;
            }
        }
        None
    }

    fn extract_line(&mut self, newline_pos: usize) -> Vec<BufferChunk> {
        let mut line_chunks = Vec::new();
        let mut remaining_pos = newline_pos;

        while let Some(chunk) = self.buffer.first_mut() {
            let chunk_len = chunk.content.len();
            if remaining_pos >= chunk_len {
                // This chunk is completely in the line
                line_chunks.push(chunk.clone());
                remaining_pos -= chunk_len;
                self.buffer.remove(0);
                // Skip the newline character
                if remaining_pos == 0 && !self.buffer.is_empty() {
                    // The newline is after this chunk, handle it in next iteration
                    break;
                }
            } else {
                // The newline is in this chunk
                let (before_newline, after_newline) = chunk.content.split_at(remaining_pos);
                line_chunks.push(BufferChunk {
                    content: before_newline.to_string(),
                    content_type: chunk.content_type,
                });
                // Keep the part after the newline (skip the \n itself)
                chunk.content = after_newline.strip_prefix('\n').unwrap_or("").to_string();
                if chunk.content.is_empty() {
                    self.buffer.remove(0);
                }
                break;
            }
        }

        line_chunks
    }

    pub fn flush(&mut self) {
        if !self.buffer.is_empty() {
            let line = self.buffer.drain(..).collect();
            self.process_line(line);
        }
    }

    fn process_line(&mut self, line_chunks: Vec<BufferChunk>) {
        // Reconstruct the line as a string for markdown parsing
        let line: String = line_chunks
            .iter()
            .map(|c| c.content.as_str())
            .collect::<Vec<_>>()
            .join("");

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
                self.render_heading(&line);
                self.needs_spacing = true;
                Mode::Normal
            } else if !line.is_empty() {
                if self.had_thinking {
                    let _ = writeln!(self.writer);
                    self.had_thinking = false;
                }
                self.render_text_chunked(line_chunks);
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

    fn render_text_chunked(&mut self, chunks: Vec<BufferChunk>) {
        self.word_wrap_and_print_chunked(chunks);
    }

    fn word_wrap_and_print_chunked(&mut self, chunks: Vec<BufferChunk>) {
        // Build styled words from chunks, preserving chunk boundaries
        let mut styled_words: Vec<(String, ContentType)> = Vec::new();
        let mut current_word = String::new();
        let mut current_type = if let Some(first) = chunks.first() {
            first.content_type
        } else {
            ContentType::Normal
        };
        let mut in_ansi = false;

        for chunk in &chunks {
            // If chunk type changed, flush current word
            if chunk.content_type != current_type && !current_word.is_empty() {
                styled_words.push((current_word.clone(), current_type));
                current_word.clear();
                current_type = chunk.content_type;
            } else if chunk.content_type != current_type {
                current_type = chunk.content_type;
            }

            for ch in chunk.content.chars() {
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
                        styled_words.push((current_word.clone(), current_type));
                        current_word.clear();
                    }
                    if ch == '\n' {
                        styled_words.push(("\n".to_string(), current_type));
                    }
                } else {
                    current_word.push(ch);
                }
            }
        }

        if !current_word.is_empty() {
            styled_words.push((current_word, current_type));
        }

        let mut current_line = String::new();
        let mut current_line_type = ContentType::Normal;

        for (word, word_type) in styled_words {
            if word == "\n" {
                self.print_line(&current_line, current_line_type);
                current_line.clear();
                current_line_type = ContentType::Normal;
            } else {
                let word_display_len = self.display_len(&word);
                let current_display_len = self.display_len(&current_line);

                if current_display_len == 0 {
                    current_line.push_str(&word);
                    current_line_type = word_type;
                } else if current_display_len + 1 + word_display_len <= self.width {
                    current_line.push(' ');
                    current_line.push_str(&word);
                } else {
                    self.print_line(&current_line, current_line_type);
                    current_line = word;
                    current_line_type = word_type;
                }
            }
        }

        if !current_line.is_empty() {
            self.print_line(&current_line, current_line_type);
        }
    }

    fn print_line(&mut self, line: &str, content_type: ContentType) {
        let (start_code, end_code) = match content_type {
            ContentType::Normal => ("", ""),
            ContentType::Thinking => ("\x1b[90m\x1b[3m", "\x1b[0m"),
        };
        let _ = writeln!(self.writer, "{}{}{}", start_code, line, end_code);
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
