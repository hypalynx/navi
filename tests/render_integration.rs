use navi::{ContentType, Renderer};

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

    assert!(!output_str.is_empty(), "output should not be empty");
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

    assert_eq!(output_str.trim(), "Hello world this is a test");
}

#[test]
fn test_render_hello_response() {
    let fixture = include_str!("fixtures/hello_response.log");
    let mut output = Vec::new();

    let mut renderer = Renderer::new(80, &mut output);
    for line in fixture.lines() {
        renderer.push(line);
    }
    renderer.flush();

    let output_str = String::from_utf8(output).expect("output should be valid UTF-8");
    println!(
        "=== RENDERED OUTPUT ===\n{}\n=== END OUTPUT ===",
        output_str
    );
}
