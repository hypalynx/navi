use navi::{ContentType, Renderer};

fn strip_ansi(s: &str) -> String {
    let mut result = String::new();
    let mut in_ansi = false;

    for ch in s.chars() {
        if ch == '\x1b' {
            in_ansi = true;
        } else if in_ansi {
            if ch == 'm' {
                in_ansi = false;
            }
        } else {
            result.push(ch);
        }
    }

    result
}

#[test]
fn test_render_simple_markdown() {
    let fixture = "# Hello\n\nSome **bold** text\n\nMore content";
    let mut output = Vec::new();

    let mut renderer = Renderer::new(80, &mut output);

    // Process the fixture
    for line in fixture.lines() {
        renderer.push(line, ContentType::Normal);
        renderer.push("\n", ContentType::Normal);
    }
    renderer.flush();

    // Get the rendered output as a string
    let output_str = String::from_utf8(output).expect("output should be valid UTF-8");

    // Print so we can see what the renderer produces
    println!("=== RENDERED OUTPUT ===");
    println!("{}", output_str);
    println!("=== END OUTPUT ===");

    // Strip ANSI codes and verify markdown syntax is gone
    let clean = strip_ansi(&output_str);
    assert!(!output_str.is_empty(), "output should not be empty");
    assert!(
        !clean.contains('#'),
        "heading # should be stripped from output"
    );
    assert!(clean.contains("Hello"), "heading text should be present");
    assert!(
        !clean.contains("**"),
        "bold ** should be stripped from output"
    );
    assert!(clean.contains("bold"), "bold text should be present");
    assert!(
        output_str.contains("\x1b[1;33m"),
        "heading should have bold yellow ANSI code"
    );
    assert!(
        output_str.contains("\x1b[1m"),
        "bold text should have bold ANSI code"
    );
}

#[test]
fn test_render_streaming() {
    // Test with token-sized chunks to see streaming behavior
    let tokens = vec![
        "Hello", " ", "world", " ", "this", " ", "is", " ", "a", " ", "test",
    ];
    let mut output = Vec::new();

    let mut renderer = Renderer::new(80, &mut output);

    // Push tokens one at a time
    for token in tokens {
        renderer.push(token, ContentType::Normal);
    }
    renderer.push("\n", ContentType::Normal);
    renderer.flush();

    let output_str = String::from_utf8(output).expect("output should be valid UTF-8");
    println!("=== STREAMING OUTPUT ===");
    println!("{}", output_str);
    println!("=== END OUTPUT ===");

    let clean = strip_ansi(&output_str);
    assert_eq!(clean.trim(), "Hello world this is a test");
}

#[test]
fn test_heading_levels() {
    let mut output = Vec::new();
    let mut renderer = Renderer::new(80, &mut output);

    renderer.push("# H1 Heading", ContentType::Normal);
    renderer.push("\n", ContentType::Normal);
    renderer.push("## H2 Heading", ContentType::Normal);
    renderer.push("\n", ContentType::Normal);
    renderer.push("### H3 Heading", ContentType::Normal);
    renderer.push("\n", ContentType::Normal);
    renderer.flush();

    let output_str = String::from_utf8(output).expect("output should be valid UTF-8");
    let clean = strip_ansi(&output_str);

    // Verify # stripped
    assert!(!clean.contains('#'), "# should be stripped");

    // Verify heading text present
    assert!(clean.contains("H1 Heading"), "H1 text should be present");
    assert!(clean.contains("H2 Heading"), "H2 text should be present");
    assert!(clean.contains("H3 Heading"), "H3 text should be present");

    // Verify correct ANSI codes
    assert!(
        output_str.contains("\x1b[1;33m"),
        "H1 should be bold yellow"
    );
    assert!(output_str.contains("\x1b[1;36m"), "H2 should be bold cyan");
    assert!(
        output_str.contains("\x1b[1;35m"),
        "H3 should be bold magenta"
    );
}

#[test]
fn test_inline_bold() {
    let mut output = Vec::new();
    let mut renderer = Renderer::new(80, &mut output);

    renderer.push("This is **bold** text", ContentType::Normal);
    renderer.push("\n", ContentType::Normal);
    renderer.flush();

    let output_str = String::from_utf8(output).expect("output should be valid UTF-8");
    let clean = strip_ansi(&output_str);

    // Verify ** stripped
    assert!(!clean.contains("**"), "** should be stripped");

    // Verify text present
    assert!(clean.contains("bold"), "bold text should be present");
    assert!(
        clean.contains("This is"),
        "surrounding text should be present"
    );

    // Verify ANSI code
    assert!(
        output_str.contains("\x1b[1m"),
        "bold should have bold ANSI code"
    );
}

#[test]
fn test_inline_italic() {
    let mut output = Vec::new();
    let mut renderer = Renderer::new(80, &mut output);

    renderer.push("This is *italic* text", ContentType::Normal);
    renderer.push("\n", ContentType::Normal);
    renderer.flush();

    let output_str = String::from_utf8(output).expect("output should be valid UTF-8");
    let clean = strip_ansi(&output_str);

    // Verify * stripped from content (not from "is")
    assert!(
        !clean.contains("*italic*"),
        "* should be stripped around italic"
    );

    // Verify text present
    assert!(clean.contains("italic"), "italic text should be present");

    // Verify ANSI code
    assert!(
        output_str.contains("\x1b[3m"),
        "italic should have italic ANSI code"
    );
}

#[test]
fn test_inline_code() {
    let mut output = Vec::new();
    let mut renderer = Renderer::new(80, &mut output);

    renderer.push("Use `code` in text", ContentType::Normal);
    renderer.push("\n", ContentType::Normal);
    renderer.flush();

    let output_str = String::from_utf8(output).expect("output should be valid UTF-8");
    let clean = strip_ansi(&output_str);

    // Verify backticks stripped
    assert!(!clean.contains('`'), "backticks should be stripped");

    // Verify text present
    assert!(clean.contains("code"), "code text should be present");
    assert!(clean.contains("Use"), "surrounding text should be present");

    // Verify ANSI code (yellow)
    assert!(
        output_str.contains("\x1b[33m"),
        "code span should be yellow"
    );
}

#[test]
fn test_delimiter_split_across_tokens() {
    let mut output = Vec::new();
    let mut renderer = Renderer::new(80, &mut output);

    // Split ** across two tokens
    renderer.push("This is ", ContentType::Normal);
    renderer.push("**", ContentType::Normal);
    renderer.push("bold", ContentType::Normal);
    renderer.push("**", ContentType::Normal);
    renderer.push(" text", ContentType::Normal);
    renderer.push("\n", ContentType::Normal);
    renderer.flush();

    let output_str = String::from_utf8(output).expect("output should be valid UTF-8");
    let clean = strip_ansi(&output_str);

    // Verify ** stripped
    assert!(
        !clean.contains("**"),
        "** should be stripped even when split"
    );

    // Verify bold text present
    assert!(clean.contains("bold"), "bold text should be present");

    // Verify ANSI code
    assert!(
        output_str.contains("\x1b[1m"),
        "split bold should have ANSI code"
    );
}

#[test]
fn test_marker_buf_flush_on_newline() {
    let mut output = Vec::new();
    let mut renderer = Renderer::new(80, &mut output);

    // Single # at EOL with space in next token tests marker_buf
    renderer.push("Some text", ContentType::Normal);
    renderer.push("\n", ContentType::Normal);
    renderer.push("#", ContentType::Normal);
    renderer.push(" ", ContentType::Normal);
    renderer.push("Heading text", ContentType::Normal);
    renderer.push("\n", ContentType::Normal);
    renderer.flush();

    let output_str = String::from_utf8(output).expect("output should be valid UTF-8");
    let clean = strip_ansi(&output_str);

    // Heading # should be stripped
    assert!(
        !clean.contains('#'),
        "# should be stripped even when split across tokens"
    );
    assert!(
        clean.contains("Heading text"),
        "heading text should be present"
    );

    // Verify heading ANSI code applied
    assert!(
        output_str.contains("\x1b[1;33m"),
        "heading should have ANSI code"
    );
}

#[test]
fn test_code_span_disables_formatting() {
    let mut output = Vec::new();
    let mut renderer = Renderer::new(80, &mut output);

    renderer.push("Use `*literal*` not italic", ContentType::Normal);
    renderer.push("\n", ContentType::Normal);
    renderer.flush();

    let output_str = String::from_utf8(output).expect("output should be valid UTF-8");
    let clean = strip_ansi(&output_str);

    // Backticks should be stripped
    assert!(!clean.contains('`'), "backticks should be stripped");

    // * inside code span should be literal
    assert!(
        clean.contains("*literal*"),
        "* inside code span should be literal"
    );

    // Should NOT have italic ANSI code
    // (only code span yellow ANSI)
    let code_part = output_str.split("\x1b[33m").nth(1).unwrap_or("");
    assert!(
        !code_part.starts_with("\x1b[3m"),
        "* inside code span should not be italic"
    );
}

#[test]
fn test_blockquote() {
    let mut output = Vec::new();
    let mut renderer = Renderer::new(80, &mut output);

    renderer.push("> This is a quote", ContentType::Normal);
    renderer.push("\n", ContentType::Normal);
    renderer.flush();

    let output_str = String::from_utf8(output).expect("output should be valid UTF-8");
    let clean = strip_ansi(&output_str);

    // Verify > stripped
    assert!(!clean.contains('>'), "> should be stripped");

    // Verify text present
    assert!(
        clean.contains("This is a quote"),
        "quote text should be present"
    );

    // Verify ANSI code (dark gray)
    assert!(
        output_str.contains("\x1b[90m"),
        "blockquote should be dark gray"
    );
}

#[test]
fn test_list_item() {
    let mut output = Vec::new();
    let mut renderer = Renderer::new(80, &mut output);

    renderer.push("- First item", ContentType::Normal);
    renderer.push("\n", ContentType::Normal);
    renderer.push("* Second item", ContentType::Normal);
    renderer.push("\n", ContentType::Normal);
    renderer.flush();

    let output_str = String::from_utf8(output).expect("output should be valid UTF-8");
    let clean = strip_ansi(&output_str);

    // Verify - and * stripped, • added
    assert!(!clean.contains('-'), "- should be stripped");
    assert!(clean.contains("First item"), "item text should be present");
    assert!(clean.contains("•"), "bullet • should be present");
}

#[test]
fn test_trailing_punctuation_stays_on_line() {
    // Test that trailing punctuation doesn't wrap to its own line
    let mut output = Vec::new();
    let mut renderer = Renderer::new(20, &mut output); // Very narrow width to force wrapping

    // Send tokens that will fill the line and then send punctuation
    let tokens = vec!["Hello", " ", "world", " ", "test", "."];
    for token in tokens {
        renderer.push(token, ContentType::Normal);
    }
    renderer.push("\n", ContentType::Normal);
    renderer.flush();

    let output_str = String::from_utf8(output).expect("output should be valid UTF-8");
    let clean = strip_ansi(&output_str);

    // The period should be on the same line as "test", not on its own line
    let lines: Vec<&str> = clean.lines().collect();
    assert!(lines.len() >= 1, "should have at least one line");

    // The last non-empty line should end with a period
    if let Some(last_line) = lines.iter().rev().find(|l| !l.is_empty()) {
        assert!(
            last_line.ends_with('.'),
            "punctuation should be at end of line, not on its own: {:?}",
            lines
        );
    }
}

#[test]
fn test_various_punctuation_stay_on_line() {
    // Test various types of trailing punctuation
    let test_cases = vec![
        ("word", ".", "word."),
        ("word", ",", "word,"),
        ("word", ";", "word;"),
        ("word", "!", "word!"),
        ("word", "?", "word?"),
        ("word", ")", "word)"),
        ("word", "]", "word]"),
        ("word", "}", "word}"),
        ("word", "\"", "word\""),
    ];

    for (word, punct, expected) in test_cases {
        let mut output = Vec::new();
        let mut renderer = Renderer::new(10, &mut output);

        renderer.push(word, ContentType::Normal);
        renderer.push(punct, ContentType::Normal);
        renderer.push("\n", ContentType::Normal);
        renderer.flush();

        let output_str = String::from_utf8(output).expect("output should be valid UTF-8");
        let clean = strip_ansi(&output_str);

        // All on one line without wrap
        assert_eq!(clean.trim(), expected, "for punctuation {}", punct);
    }
}
