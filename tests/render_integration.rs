use navi::Renderer;

#[test]
fn test_render_simple_markdown() {
    let fixture = "# Hello\n\nSome **bold** text\n\nMore content";
    let mut output = Vec::new();

    let mut renderer = Renderer::new(80, &mut output);

    // Process the fixture
    for line in fixture.lines() {
        renderer.push(line);
        renderer.push("\n");
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

// Placeholder for fixture-based test once you provide hello_response.log
// #[test]
// fn test_render_hello_response() {
//     let fixture = include_str!("fixtures/hello_response.log");
//     let mut output = Vec::new();
//
//     let mut renderer = Renderer::new(80, &mut output);
//     for line in fixture.lines() {
//         renderer.push(line);
//     }
//     renderer.flush();
//
//     let output_str = String::from_utf8(output).expect("output should be valid UTF-8");
//     println!("=== RENDERED OUTPUT ===\n{}\n=== END OUTPUT ===", output_str);
// }
