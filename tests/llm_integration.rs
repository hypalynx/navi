use navi::{StreamEvent, parse_line};
use std::collections::HashMap;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_parse_hello_response_streaming() {
    let fixture = include_str!("fixtures/hello_response.log");
    let (tx, mut rx) = mpsc::channel(1000); // Larger buffer to avoid blocking

    // Spawn a task to collect events while we parse
    let collect_task = tokio::spawn(async move {
        let mut thinking_content = String::new();
        let mut response_content = String::new();

        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Thinking(text) => thinking_content.push_str(&text),
                StreamEvent::Content(text) => response_content.push_str(&text),
                StreamEvent::ToolCalls(_) => {
                    // Ignore tool calls in this test
                }
                StreamEvent::Usage { .. } => {
                    // Ignore usage data in this test
                }
                StreamEvent::ContextExceeded => {
                    // Ignore context exceeded in this test
                }
                StreamEvent::Error(_) => {
                    // Ignore errors in this test
                }
                StreamEvent::Done => break,
            }
        }

        (thinking_content, response_content)
    });

    // Parse all lines from the fixture
    let mut tool_calls_acc = HashMap::new();
    let mut thinking_content = String::new();
    for line in fixture.lines() {
        let _ = parse_line(line, &tx, &mut tool_calls_acc, &mut thinking_content).await;
    }

    // Send Done to signal completion
    let _ = tx.send(StreamEvent::Done).await;

    // Wait for collection task to finish
    let (thinking_content, response_content) = collect_task.await.unwrap();

    // Verify thinking content was captured
    assert!(
        !thinking_content.is_empty(),
        "should have captured thinking content"
    );
    assert!(
        thinking_content.contains("user is greeting"),
        "thinking should contain reasoning"
    );
    assert!(
        thinking_content.contains("AI assistant"),
        "thinking should mention being an AI"
    );

    // Verify response content was captured
    assert!(
        !response_content.is_empty(),
        "should have captured response content"
    );
    assert_eq!(
        response_content,
        "Hello! 👋 How can I assist you today? Feel free to ask me anything!"
    );
}

#[tokio::test]
async fn test_parse_xml_tool_calls_in_reasoning_content() {
    let fixture = include_str!("fixtures/xml_toolcall_in_thinking.log");
    let (tx, mut rx) = mpsc::channel(1000);

    // Spawn a task to collect thinking content
    let collect_task = tokio::spawn(async move {
        let mut thinking_content = String::new();

        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Thinking(text) => thinking_content.push_str(&text),
                StreamEvent::Done => break,
                _ => {}
            }
        }

        thinking_content
    });

    // Parse all lines from the fixture
    let mut tool_calls_acc = HashMap::new();
    let mut thinking_content = String::new();
    for line in fixture.lines() {
        let _ = parse_line(line, &tx, &mut tool_calls_acc, &mut thinking_content).await;
    }

    // Send Done to signal completion
    let _ = tx.send(StreamEvent::Done).await;

    // Wait for collection task to finish
    let _collected_thinking = collect_task.await.unwrap();

    // Verify tool calls are in the accumulated thinking content
    assert!(
        thinking_content.contains("<tool_call>"),
        "thinking should contain XML tool calls"
    );

    // Verify we can parse the tool calls from the thinking content
    let tool_calls = navi::parse_xml_tool_calls(&thinking_content);
    assert_eq!(
        tool_calls.len(),
        1,
        "should parse one tool call from reasoning"
    );
    assert_eq!(tool_calls[0].name, "Read", "should have Read function");
    assert_eq!(
        tool_calls[0].args.get("filePath").and_then(|v| v.as_str()),
        Some("./src/tools.rs"),
        "should extract file path without extra whitespace"
    );
    assert_eq!(
        tool_calls[0].args.get("offset").and_then(|v| v.as_str()),
        Some("560"),
        "should extract offset parameter"
    );
}

#[test]
fn test_parse_xml_tool_calls() {
    // Test basic XML tool call parsing
    let xml_content = r#"
    Some content here
    <tool_call><function=Bash><parameter=command>ls -la</parameter><parameter=description>List directory contents</parameter></function></tool_call>
    More content
    "#;

    let tool_calls = navi::parse_xml_tool_calls(xml_content);

    assert_eq!(tool_calls.len(), 1, "should parse one tool call");
    assert_eq!(
        tool_calls[0].name, "Bash",
        "should have correct function name"
    );
    assert_eq!(
        tool_calls[0].args.get("command").and_then(|v| v.as_str()),
        Some("ls -la"),
        "should extract command parameter"
    );
    assert_eq!(
        tool_calls[0]
            .args
            .get("description")
            .and_then(|v| v.as_str()),
        Some("List directory contents"),
        "should extract description parameter"
    );
}

#[test]
fn test_parse_multiple_xml_tool_calls() {
    // Test parsing multiple XML tool calls
    let xml_content = r#"
    <tool_call><function=Read><parameter=filePath>/etc/hosts</parameter></function></tool_call>
    <tool_call><function=Glob><parameter=pattern>*.rs</parameter></function></tool_call>
    "#;

    let tool_calls = navi::parse_xml_tool_calls(xml_content);

    assert_eq!(tool_calls.len(), 2, "should parse two tool calls");

    assert_eq!(tool_calls[0].name, "Read");
    assert_eq!(
        tool_calls[0].args.get("filePath").and_then(|v| v.as_str()),
        Some("/etc/hosts")
    );

    assert_eq!(tool_calls[1].name, "Glob");
    assert_eq!(
        tool_calls[1].args.get("pattern").and_then(|v| v.as_str()),
        Some("*.rs")
    );
}
