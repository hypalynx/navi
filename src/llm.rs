use crate::render::{ContentType, Renderer};
use futures::TryStreamExt;
use serde::Serialize;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

const BRAILLE_SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub enum StreamEvent {
    Content(String),
    Thinking(String),
    Done,
}

#[derive(Serialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
}

pub async fn execute(input: &str, history: &mut Vec<Message>, port: u16) -> anyhow::Result<()> {
    history.push(Message {
        role: "user".to_string(),
        content: input.to_string(),
        thinking: None,
    });

    match llm_request(history, port).await {
        Ok(mut rx) => {
            let mut content = String::new();
            let mut thinking = String::new();
            let mut renderer = Renderer::new(80, std::io::stdout());

            // Spinner state
            let spinner_active = Arc::new(AtomicBool::new(true));
            let spinner_active_clone = spinner_active.clone();

            // Start spinner task - only shows during initial wait
            let spinner_handle = tokio::spawn(async move {
                let mut frame = 0;
                while spinner_active_clone.load(Ordering::Relaxed) {
                    print!("\r{} ", BRAILLE_SPINNER[frame % BRAILLE_SPINNER.len()]);
                    let _ = std::io::stdout().flush();
                    tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;
                    frame += 1;
                }
                // Clear spinner line
                print!("\r\x1b[K");
                let _ = std::io::stdout().flush();
            });

            let mut first_event = true;

            while let Some(event) = rx.recv().await {
                if first_event {
                    spinner_active.store(false, Ordering::Relaxed);
                }
                first_event = false;
                match event {
                    StreamEvent::Content(text) => {
                        renderer.push(&text, ContentType::Normal);
                        content.push_str(&text);
                    }
                    StreamEvent::Thinking(text) => {
                        renderer.push(&text, ContentType::Thinking);
                        thinking.push_str(&text);
                    }
                    StreamEvent::Done => {
                        renderer.flush();
                        break;
                    }
                }
            }

            // Wait for spinner task to finish
            let _ = spinner_handle.await;

            history.push(Message {
                role: "assistant".to_string(),
                content,
                thinking: if thinking.is_empty() {
                    None
                } else {
                    Some(thinking)
                },
            });
        }
        Err(e) => eprintln!("Could not communicate with LLM: {}", e),
    };

    Ok(())
}

// TODO get api_key if needed
// TODO get hostname from config, default to localhost
// TODO need to pass client config in here so it's configurable/testable.
async fn llm_request(
    messages: &[Message],
    port: u16,
) -> anyhow::Result<mpsc::Receiver<StreamEvent>> {
    let (tx, rx) = mpsc::channel(100);
    let messages = messages.to_vec();

    tokio::spawn(async move {
        let client = reqwest::Client::new();

        let body = serde_json::json!({
            "model": "qwen3.5-2b",
            "messages": messages,
            "stream": true,
        });

        let url = format!("http://127.0.0.1:{}/v1/chat/completions", port);
        match client
            .post(&url)
            //.header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
        {
            Ok(response) => {
                let mut stream = response.bytes_stream();
                let mut buffer = String::new();

                while let Ok(Some(bytes)) = stream.try_next().await {
                    buffer.push_str(&String::from_utf8_lossy(&bytes));

                    // Process complete lines, keep incomplete ones in buffer
                    let lines: Vec<&str> = buffer.split('\n').collect();
                    for line in &lines[..lines.len() - 1] {
                        let _ = parse_line(line, &tx).await;
                    }
                    // Keep the last (possibly incomplete) line
                    buffer = lines.last().unwrap_or(&"").to_string();
                }

                // Handle any remaining buffer
                if !buffer.is_empty() {
                    let _ = parse_line(&buffer, &tx).await;
                }

                let _ = tx.send(StreamEvent::Done).await;
            }
            Err(_) => {
                let _ = tx.send(StreamEvent::Done).await;
            }
        }
    });

    Ok(rx)
}

async fn parse_line(line: &str, tx: &mpsc::Sender<StreamEvent>) -> anyhow::Result<()> {
    if line == "data: [DONE]" {
        return Ok(());
    }

    if let Some(data) = line.strip_prefix("data: ")
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(data)
        && let Some(delta) = json
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("delta"))
    {
        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
            tx.send(StreamEvent::Content(content.to_string())).await?;
        }
        if let Some(thinking) = delta.get("reasoning_content").and_then(|t| t.as_str()) {
            tx.send(StreamEvent::Thinking(thinking.to_string())).await?;
        }
    }

    Ok(())
}
