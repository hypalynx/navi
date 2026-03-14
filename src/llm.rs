use crate::render::{ContentType, Renderer};
use crate::tools::ToolCall;
use futures::TryStreamExt;
use owo_colors::OwoColorize;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

const BRAILLE_SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub enum StreamEvent {
    Content(String),
    Thinking(String),
    ToolCalls(Vec<crate::tools::ToolCall>),
    Error(String),
    Done,
}

#[derive(Serialize, Clone)]
pub struct Message {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

pub async fn execute(
    input: &str,
    history: &mut Vec<Message>,
    port: u16,
    thinking_enabled: bool,
) -> anyhow::Result<()> {
    history.push(Message {
        role: "user".to_string(),
        content: Some(input.to_string()),
        thinking: None,
        tool_calls: None,
        tool_call_id: None,
    });

    const MAX_ITERATIONS: usize = 10;
    let mut iteration = 0;

    loop {
        iteration += 1;
        if iteration > MAX_ITERATIONS {
            eprintln!("Max tool iterations reached");
            break;
        }

        match llm_request(history, port, thinking_enabled).await {
            Ok((mut rx, llm_handle)) => {
                let mut content = String::new();
                let mut thinking = String::new();
                let mut tool_calls = Vec::new();
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
                let mut interrupted = false;
                print!("\x1b[?25l");
                let _ = std::io::stdout().flush();

                loop {
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {
                            interrupted = true;
                            llm_handle.abort();
                            spinner_active.store(false, Ordering::Relaxed);
                            print!("\x1b[?25h");
                            let _ = std::io::stdout().flush();
                            println!("\n");
                            break;
                        }
                        result = rx.recv() => {
                            if let Some(event) = result {
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
                                    StreamEvent::ToolCalls(calls) => {
                                        tool_calls = calls;
                                    }
                                    StreamEvent::Error(err) => {
                                        spinner_active.store(false, Ordering::Relaxed);
                                        print!("\x1b[?25h");
                                        let _ = std::io::stdout().flush();
                                        eprintln!("\nLLM server error: {}", err);
                                        break;
                                    }
                                    StreamEvent::Done => {
                                        renderer.flush();
                                        print!("\x1b[?25h");
                                        let _ = std::io::stdout().flush();
                                        break;
                                    }
                                }
                            } else {
                                break;
                            }
                        }
                    }
                }

                // Wait for spinner task to finish
                let _ = spinner_handle.await;

                if interrupted {
                    break;
                }

                if tool_calls.is_empty() {
                    // No tool calls, save final assistant message and exit loop
                    history.push(Message {
                        role: "assistant".to_string(),
                        content: Some(content),
                        thinking: if thinking.is_empty() {
                            None
                        } else {
                            Some(thinking)
                        },
                        tool_calls: None,
                        tool_call_id: None,
                    });
                    break;
                } else {
                    // Tool calls exist, push assistant message with tool_calls
                    let tool_calls_json: Vec<serde_json::Value> = tool_calls
                        .iter()
                        .map(|tc| {
                            serde_json::json!({
                                "type": "function",
                                "id": tc.id,
                                "function": {
                                    "name": tc.name,
                                    "arguments": serde_json::to_string(&tc.args).unwrap_or_default()
                                }
                            })
                        })
                        .collect();

                    history.push(Message {
                        role: "assistant".to_string(),
                        content: None,
                        thinking: if thinking.is_empty() {
                            None
                        } else {
                            Some(thinking)
                        },
                        tool_calls: Some(tool_calls_json),
                        tool_call_id: None,
                    });

                    // Execute tools and push results
                    for tc in &tool_calls {
                        println!(
                            "\n{}\n",
                            format!("{} {:?}", tc.name, tc.args)
                                .italic()
                                .bright_black()
                                .on_black()
                        );

                        let (_summary, result) = crate::tools::execute_tool(tc).await;
                        history.push(Message {
                            role: "tool".to_string(),
                            content: Some(result),
                            thinking: None,
                            tool_calls: None,
                            tool_call_id: Some(tc.id.clone()),
                        });
                    }
                    // Loop continues to next llm_request
                }
            }
            Err(e) => {
                eprintln!("Could not communicate with LLM: {}", e);
                break;
            }
        }
    }

    Ok(())
}

// TODO get api_key if needed
// TODO get hostname from config, default to localhost
// TODO need to pass client config in here so it's configurable/testable.
async fn llm_request(
    messages: &[Message],
    port: u16,
    thinking_enabled: bool,
) -> anyhow::Result<(mpsc::Receiver<StreamEvent>, tokio::task::JoinHandle<()>)> {
    let (tx, rx) = mpsc::channel(100);
    let messages = messages.to_vec();

    let handle = tokio::spawn(async move {
        let client = reqwest::Client::new();

        let body = serde_json::json!({
            "model": "qwen3.5-2b",
            "messages": messages,
            "stream": true,
            "tools": crate::tools::get_tool_definitions(),
            "chat_template_kwargs": {
                "enable_thinking": thinking_enabled
            }
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
                // Accumulate tool calls: index -> (id, name, args_so_far)
                let mut tool_calls_acc: HashMap<usize, (String, String, String)> = HashMap::new();

                while let Ok(Some(bytes)) = stream.try_next().await {
                    buffer.push_str(&String::from_utf8_lossy(&bytes));

                    // Process complete lines, keep incomplete ones in buffer
                    let lines: Vec<&str> = buffer.split('\n').collect();
                    for line in &lines[..lines.len() - 1] {
                        let _ = parse_line(line, &tx, &mut tool_calls_acc).await;
                    }
                    // Keep the last (possibly incomplete) line
                    buffer = lines.last().unwrap_or(&"").to_string();
                }

                // Handle any remaining buffer
                if !buffer.is_empty() {
                    let _ = parse_line(&buffer, &tx, &mut tool_calls_acc).await;
                }

                // Convert accumulated tool calls to ToolCall structs
                if !tool_calls_acc.is_empty() {
                    let mut sorted_calls: Vec<_> = tool_calls_acc.into_iter().collect();
                    sorted_calls.sort_by_key(|&(idx, _)| idx);

                    let tool_calls: Vec<ToolCall> =
                        sorted_calls
                            .into_iter()
                            .filter_map(|(_, (id, name, args_str))| {
                                match serde_json::from_str::<
                                    serde_json::Map<String, serde_json::Value>,
                                >(&args_str)
                                {
                                    Ok(args) => Some(ToolCall { id, name, args }),
                                    Err(_) => None,
                                }
                            })
                            .collect();

                    if !tool_calls.is_empty() {
                        let _ = tx.send(StreamEvent::ToolCalls(tool_calls)).await;
                    }
                }

                let _ = tx.send(StreamEvent::Done).await;
            }
            Err(e) => {
                let _ = tx.send(StreamEvent::Error(e.to_string())).await;
                let _ = tx.send(StreamEvent::Done).await;
            }
        }
    });

    Ok((rx, handle))
}

pub async fn parse_line(
    line: &str,
    tx: &mpsc::Sender<StreamEvent>,
    tool_calls_acc: &mut HashMap<usize, (String, String, String)>,
) -> anyhow::Result<()> {
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

        // Handle tool_calls streaming
        if let Some(tool_calls_array) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
            for tool_call in tool_calls_array {
                if let Some(index) = tool_call.get("index").and_then(|i| i.as_u64()) {
                    let idx = index as usize;
                    let entry = tool_calls_acc
                        .entry(idx)
                        .or_insert_with(|| (String::new(), String::new(), String::new()));

                    if let Some(id) = tool_call.get("id").and_then(|i| i.as_str()) {
                        entry.0 = id.to_string();
                    }
                    if let Some(name) = tool_call
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                    {
                        entry.1 = name.to_string();
                    }
                    if let Some(args) = tool_call
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|a| a.as_str())
                    {
                        entry.2.push_str(args);
                    }
                }
            }
        }
    }

    Ok(())
}
