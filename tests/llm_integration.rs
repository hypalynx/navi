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
                StreamEvent::Done => break,
            }
        }

        (thinking_content, response_content)
    });

    // Parse all lines from the fixture
    let mut tool_calls_acc = HashMap::new();
    for line in fixture.lines() {
        let _ = parse_line(line, &tx, &mut tool_calls_acc).await;
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
