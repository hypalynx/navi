use crate::Message;
use std::io::Write;
use tokio::sync::mpsc;
use owo_colors::OwoColorize;

pub enum StreamEvent {
    Content(String),
    Thinking(String),
    Done,
}

pub async fn execute(input: &str, history: &mut Vec<Message>) -> anyhow::Result<()> {
    history.push(Message {
        role: "user".to_string(),
        content: input.to_string(),
        thinking: None,
    });

    match llm_request(history).await {
        Ok(mut rx) => {
            let mut content = String::new();
            let mut thinking = String::new();

            while let Some(event) = rx.recv().await {
                match event {
                    StreamEvent::Content(text) => {
                        print!("{}", text);
                        let _ = std::io::stdout().flush();
                        content.push_str(&text);
                    }
                    StreamEvent::Thinking(text) => {
                        print!("{}", text.bright_black().italic());
                        let _ = std::io::stdout().flush();
                        thinking.push_str(&text);
                    }
                    StreamEvent::Done => break,
                }
            }

            history.push(Message {
                role: "assistant".to_string(),
                content,
                thinking: if thinking.is_empty() { None } else { Some(thinking) },
            });
        }
        Err(e) => eprintln!("Could not communicate with LLM: {}", e),
    };

    Ok(())
}

// TODO get api_key if needed
// TODO get hostname from config, default to localhost
// TODO need to pass client config in here so it's configurable/testable.
async fn llm_request(messages: &[Message]) -> anyhow::Result<mpsc::Receiver<StreamEvent>> {
    let (tx, rx) = mpsc::channel(100);
    let messages = messages.to_vec();

    tokio::spawn(async move {
        let client = reqwest::Client::new();

        let body = serde_json::json!({
            "model": "qwen3.5-2b",
            "messages": messages,
            "stream": true,
        });

        match client
            .post("http://127.0.0.1:7777/v1/chat/completions")
            //.header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
        {
            Ok(response) => {
                if let Ok(text) = response.text().await {
                    let _ = parse_events(text, tx).await;
                }
            }
            Err(_) => {
                let _ = tx.send(StreamEvent::Done).await;
            }
        }
    });

    Ok(rx)
}

async fn parse_events(text: String, tx: mpsc::Sender<StreamEvent>) -> anyhow::Result<()> {
    for line in text.lines() {
        if line == "data: [DONE]" {
            break;
        }

        if let Some(data) = line.strip_prefix("data: ") {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                if let Some(delta) = json
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("delta"))
                {
                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                        tx.send(StreamEvent::Content(content.to_string())).await?;
                    }
                    if let Some(thinking) = delta.get("reasoning_content").and_then(|t| t.as_str())
                    {
                        tx.send(StreamEvent::Thinking(thinking.to_string())).await?;
                    }
                }
            }
        }
    }

    tx.send(StreamEvent::Done).await?;
    Ok(())
}
