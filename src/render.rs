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

#[derive(PartialEq)]
enum Mode {
    Normal,
    CodeBlock,
}

#[derive(Clone)]
struct Segment {
    text: String,
    bold: bool,
    italic: bool,
    code_span: bool,
    strikethrough: bool,
}

pub struct Renderer<T: Write> {
    writer: T,
    mode: Mode,
    current_line_length: usize,
    width: usize,
    at_line_start: bool,
    heading_level: u8,
    bold: bool,
    italic: bool,
    code_span: bool,
    strikethrough: bool,
    blockquote: bool,
    marker_buf: String,
}

impl<T: Write> Renderer<T> {
    pub fn new(width: usize, writer: T) -> Self {
        Self {
            writer,
            mode: Mode::Normal,
            current_line_length: 0,
            width,
            at_line_start: true,
            heading_level: 0,
            bold: false,
            italic: false,
            code_span: false,
            strikethrough: false,
            blockquote: false,
            marker_buf: String::new(),
        }
    }

    fn flush_segment(&self, current_segment: &mut Segment) {
        // Create a fresh segment with current formatting state
        let new_segment = Segment {
            text: String::new(),
            bold: self.bold,
            italic: self.italic,
            code_span: self.code_span,
            strikethrough: self.strikethrough,
        };
        *current_segment = new_segment;
    }

    fn render_inline(&mut self, text: &str) -> Vec<Segment> {
        let mut segments = Vec::new();
        let mut current_segment = Segment {
            text: String::new(),
            bold: self.bold,
            italic: self.italic,
            code_span: self.code_span,
            strikethrough: self.strikethrough,
        };

        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            // Inside code span, only backtick is special
            if self.code_span {
                if i < chars.len() && chars[i] == '`' {
                    // Flush current segment if it has content
                    if !current_segment.text.is_empty() {
                        segments.push(current_segment.clone());
                        self.flush_segment(&mut current_segment);
                    }
                    // Toggle code_span
                    self.code_span = false;
                    current_segment.code_span = self.code_span;
                    i += 1;
                } else {
                    current_segment.text.push(chars[i]);
                    i += 1;
                }
                continue;
            }

            // Not in code span, check for delimiters (longer patterns first)
            if i + 3 <= chars.len() && chars[i] == '*' && chars[i + 1] == '*' && chars[i + 2] == '*'
            {
                // Flush current segment
                if !current_segment.text.is_empty() {
                    segments.push(current_segment.clone());
                    self.flush_segment(&mut current_segment);
                }
                // Toggle bold and italic
                self.bold = !self.bold;
                self.italic = !self.italic;
                current_segment.bold = self.bold;
                current_segment.italic = self.italic;
                i += 3;
            } else if i + 2 <= chars.len()
                && ((chars[i] == '*' && chars[i + 1] == '*')
                    || (chars[i] == '_' && chars[i + 1] == '_'))
            {
                // Flush current segment
                if !current_segment.text.is_empty() {
                    segments.push(current_segment.clone());
                    self.flush_segment(&mut current_segment);
                }
                // Toggle bold
                self.bold = !self.bold;
                current_segment.bold = self.bold;
                i += 2;
            } else if i < chars.len() && (chars[i] == '*' || chars[i] == '_') {
                // Flush current segment
                if !current_segment.text.is_empty() {
                    segments.push(current_segment.clone());
                    self.flush_segment(&mut current_segment);
                }
                // Toggle italic
                self.italic = !self.italic;
                current_segment.italic = self.italic;
                i += 1;
            } else if i < chars.len() && chars[i] == '`' {
                // Flush current segment
                if !current_segment.text.is_empty() {
                    segments.push(current_segment.clone());
                    self.flush_segment(&mut current_segment);
                }
                // Toggle code_span
                self.code_span = !self.code_span;
                current_segment.code_span = self.code_span;
                i += 1;
            } else if i + 2 <= chars.len() && chars[i] == '~' && chars[i + 1] == '~' {
                // Flush current segment
                if !current_segment.text.is_empty() {
                    segments.push(current_segment.clone());
                    self.flush_segment(&mut current_segment);
                }
                // Toggle strikethrough
                self.strikethrough = !self.strikethrough;
                current_segment.strikethrough = self.strikethrough;
                i += 2;
            } else {
                // Regular character
                current_segment.text.push(chars[i]);
                i += 1;
            }
        }

        // Flush final segment
        if !current_segment.text.is_empty() {
            segments.push(current_segment);
        }

        segments
    }

    pub fn push(&mut self, token: &str, content_type: ContentType) {
        // Handle newlines
        for (i, part) in token.split('\n').enumerate() {
            if i > 0 {
                // Flush marker_buf through render_inline so formatting is processed
                if !self.marker_buf.is_empty() {
                    let marker_content = self.marker_buf.clone();
                    let segments = self.render_inline(&marker_content);
                    for segment in segments {
                        self.print_segment(&segment, content_type);
                    }
                    self.marker_buf.clear();
                }

                let _ = writeln!(self.writer);
                if self.heading_level > 0 {
                    let _ = writeln!(self.writer);
                    self.heading_level = 0;
                }
                self.current_line_length = 0;
                self.at_line_start = true;
                self.blockquote = false;
            }

            if !part.is_empty() {
                let mut processed_part = part.to_string();

                // Skip language tags that come as separate tokens (e.g., ``` then python)
                if self.mode == Mode::CodeBlock && !self.at_line_start && !part.trim().is_empty() {
                    let trimmed = part.trim();
                    if trimmed
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                    {
                        continue;
                    }
                }

                if self.at_line_start {
                    // Check for code block fence
                    if part.trim_start().starts_with("```") {
                        // Skip language identifier if present (e.g., `rust`, `js`)
                        let after_fence = part.trim_start()[3..].trim();
                        if after_fence.is_empty()
                            || after_fence
                                .chars()
                                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                        {
                            self.mode = match &self.mode {
                                Mode::CodeBlock => Mode::Normal,
                                Mode::Normal => Mode::CodeBlock,
                            };
                            self.at_line_start = false;
                            continue;
                        }
                    }

                    // Check for heading (with marker_buf support for split delimiters)
                    if part.starts_with("#") || !self.marker_buf.is_empty() {
                        let combined = format!("{}{}", self.marker_buf, part);

                        if combined.starts_with("#") {
                            let hash_count = combined.chars().take_while(|&c| c == '#').count();
                            if hash_count <= 3 && hash_count < combined.len() {
                                if let Some(pos) = combined.find(' ')
                                    && pos == hash_count
                                {
                                    // Valid heading
                                    self.heading_level = hash_count as u8;
                                    // Skip past the hashes and space we consumed
                                    // Use character-aware indexing instead of byte indexing
                                    let chars_to_skip =
                                        (pos + 1).saturating_sub(self.marker_buf.len());
                                    processed_part =
                                        part.chars().skip(chars_to_skip).collect::<String>();
                                    self.at_line_start = false;
                                    self.marker_buf.clear();
                                }
                            } else if hash_count == combined.len() {
                                // Only # chars, save to marker_buf
                                self.marker_buf = combined.clone();
                                continue;
                            }
                        } else {
                            // Not a heading, clear marker_buf if it was set
                            self.marker_buf.clear();
                        }
                    }

                    // Check for blockquote
                    if processed_part.starts_with("> ") {
                        self.blockquote = true;
                        processed_part = processed_part.chars().skip(2).collect::<String>();
                        self.at_line_start = false;
                    }

                    // Check for list items
                    if (processed_part.starts_with("- ") || processed_part.starts_with("* "))
                        && self.heading_level == 0
                        && !self.blockquote
                    {
                        self.print_segment(
                            &Segment {
                                text: "• ".to_string(),
                                bold: false,
                                italic: false,
                                code_span: false,
                                strikethrough: false,
                            },
                            content_type,
                        );
                        processed_part = processed_part.chars().skip(2).collect::<String>();
                        self.current_line_length += 2; // "• " is 2 chars
                        self.at_line_start = false;
                    }
                }

                if !processed_part.is_empty() {
                    let segments = self.render_inline(&processed_part);

                    // Calculate total display length
                    let total_len: usize = segments.iter().map(|s| self.display_len(&s.text)).sum();

                    if self.current_line_length == 0 {
                        // Start of line
                        for segment in segments {
                            self.print_segment(&segment, content_type);
                        }
                        self.current_line_length = total_len;
                    } else if self.current_line_length + total_len <= self.width {
                        // Fits on current line
                        for segment in segments {
                            self.print_segment(&segment, content_type);
                        }
                        self.current_line_length += total_len;
                    } else if processed_part.starts_with(' ') {
                        // Token is whitespace: wrap then skip the space
                        let _ = writeln!(self.writer);
                        self.current_line_length = 0;
                        self.at_line_start = false;

                        // If there's non-space content after the space, process it on the new line
                        let remainder = processed_part.trim_start();
                        if !remainder.is_empty() {
                            let remainder_segments = self.render_inline(remainder);
                            let remainder_len: usize = remainder_segments
                                .iter()
                                .map(|s| self.display_len(&s.text))
                                .sum();
                            for segment in remainder_segments {
                                self.print_segment(&segment, content_type);
                            }
                            self.current_line_length = remainder_len;
                        }
                    } else if self.is_trailing_punctuation(&processed_part) {
                        // Token is trailing punctuation: append it anyway
                        for segment in segments {
                            self.print_segment(&segment, content_type);
                        }
                        self.current_line_length += total_len;
                    } else {
                        // Token doesn't fit: wrap line first
                        let _ = writeln!(self.writer);
                        for segment in segments {
                            self.print_segment(&segment, content_type);
                        }
                        self.current_line_length = total_len;
                        self.at_line_start = false;
                    }
                }
            }
        }
    }

    pub fn flush(&mut self) {
        // Flush any remaining marker_buf
        if !self.marker_buf.is_empty() {
            let segments = vec![Segment {
                text: self.marker_buf.clone(),
                bold: false,
                italic: false,
                code_span: false,
                strikethrough: false,
            }];
            for segment in segments {
                self.print_segment(&segment, ContentType::Normal);
            }
            self.marker_buf.clear();
        }

        if self.current_line_length > 0 {
            let _ = writeln!(self.writer);
        }
    }

    fn print_segment(&mut self, segment: &Segment, content_type: ContentType) {
        let start_code = if self.mode == Mode::CodeBlock {
            "\x1b[33m".to_string() // yellow
        } else if self.heading_level == 1 {
            "\x1b[1;33m".to_string() // bold yellow
        } else if self.heading_level == 2 {
            "\x1b[1;36m".to_string() // bold cyan
        } else if self.heading_level == 3 {
            "\x1b[1;35m".to_string() // bold magenta
        } else if content_type == ContentType::Thinking {
            "\x1b[90;3m".to_string() // dark gray italic
        } else if self.blockquote {
            "\x1b[90m".to_string() // dark gray
        } else {
            // Compose from inline flags
            let mut parts = Vec::new();
            if segment.bold {
                parts.push("1");
            }
            if segment.italic {
                parts.push("3");
            }
            if segment.strikethrough {
                parts.push("9");
            }

            if segment.code_span {
                "\x1b[33m".to_string() // yellow for code spans
            } else if !parts.is_empty() {
                format!("\x1b[{}m", parts.join(";"))
            } else {
                String::new()
            }
        };

        let end_code = if start_code.is_empty() {
            String::new()
        } else {
            "\x1b[0m".to_string()
        };

        let _ = write!(self.writer, "{}{}{}", start_code, segment.text, end_code);
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

    fn is_trailing_punctuation(&self, s: &str) -> bool {
        // Check if string is only trailing punctuation (optionally with leading space)
        let trimmed = s.trim_start();
        if trimmed.is_empty() {
            return false;
        }

        // Punctuation that commonly trails words
        trimmed.chars().all(|c| {
            matches!(
                c,
                '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '"' | '\'' | '>' | '-'
            )
        })
    }
}
