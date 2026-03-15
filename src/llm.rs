use crate::render::{ContentType, Renderer};
use crate::tools::ToolCall;
use futures::TryStreamExt;
use owo_colors::OwoColorize;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::mpsc;

const BRAILLE_SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const CONTEXT_WINDOW_LIMIT: usize = 64_000;

fn format_tool_call(name: &str, args: &serde_json::Map<String, Value>) -> String {
    match name {
        "Read" => {
            let path = args.get("filePath")
                .and_then(|v| v.as_str())
                .unwrap_or("<path>");
            format!("Read {}", path)
        }
        "Glob" => {
            let pattern = args.get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("<pattern>");
            format!("Glob {}", pattern)
        }
        "Grep" => {
            let pattern = args.get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("<pattern>");
            format!("Grep {}", pattern)
        }
        "Bash" => {
            let command = args.get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("<command>");
            format!("Bash {}", command)
        }
        "Webfetch" => {
            let url = args.get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("<url>");
            format!("Webfetch {}", url)
        }
        _ => format!("{} {}", name, serde_json::to_string(args).unwrap_or_default()),
    }
}

pub enum StreamEvent {
    Content(String),
    Thinking(String),
    ToolCalls(Vec<crate::tools::ToolCall>),
    Usage {
        total_tokens: usize,
    },
    ContextExceeded,
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
    context_usage: Arc<AtomicUsize>,
) -> anyhow::Result<bool> {
    history.push(Message {
        role: "user".to_string(),
        content: Some(input.to_string()),
        thinking: None,
        tool_calls: None,
        tool_call_id: None,
    });

    // Check context limit before making request
    let current_usage = context_usage.load(Ordering::Relaxed);
    if current_usage >= CONTEXT_WINDOW_LIMIT {
        eprintln!(
            "\n[Context limit exceeded: {} / {} tokens]",
            current_usage, CONTEXT_WINDOW_LIMIT
        );
        eprintln!("Session stopped. Please start a new session.");
        return Ok(false);
    }

    let mut last_tool_calls: Option<Vec<(String, serde_json::Map<String, serde_json::Value>)>> =
        None;
    let mut duplicate_count = 0;
    let mut should_stop = false;

    loop {
        if should_stop {
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
                let spinner_done = Arc::new(AtomicBool::new(false));
                let spinner_active_clone = spinner_active.clone();
                let spinner_done_clone = spinner_done.clone();

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
                    spinner_done_clone.store(true, Ordering::Release);
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
                                    // Wait for spinner to finish clearing
                                    while !spinner_done.load(Ordering::Acquire) {
                                        tokio::task::yield_now().await;
                                    }
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
                                    StreamEvent::Usage { total_tokens } => {
                                        context_usage.store(total_tokens, Ordering::Relaxed);
                                    }
                                    StreamEvent::ContextExceeded => {
                                        spinner_active.store(false, Ordering::Relaxed);
                                        print!("\x1b[?25h");
                                        let _ = std::io::stdout().flush();
                                        eprintln!("\n[Context limit exceeded during generation]");
                                        should_stop = true;
                                        break;
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
                    return Ok(true);
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
                    return Ok(true);
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
                            format_tool_call(&tc.name, &tc.args)
                                .italic()
                                .bright_black()
                                .on_black()
                        );

                        let (summary, result) = crate::tools::execute_tool(tc).await;
                        println!("{}", summary.bright_blue());
                        history.push(Message {
                            role: "tool".to_string(),
                            content: Some(result),
                            thinking: None,
                            tool_calls: None,
                            tool_call_id: Some(tc.id.clone()),
                        });
                    }

                    // Check for duplicate tool calls
                    let current_calls: Vec<_> = tool_calls
                        .iter()
                        .map(|tc| (tc.name.clone(), tc.args.clone()))
                        .collect();

                    if let Some(ref last_calls) = last_tool_calls {
                        if current_calls == *last_calls {
                            duplicate_count += 1;
                            if duplicate_count >= 3 {
                                eprintln!("Same tool calls repeated 3 times, stopping");
                                should_stop = true;
                            }
                        } else {
                            duplicate_count = 0;
                        }
                    }
                    last_tool_calls = Some(current_calls);
                    // Loop continues to next llm_request
                }
            }
            Err(e) => {
                eprintln!("Could not communicate with LLM: {}", e);
                return Ok(true);
            }
        }
    }

    Ok(!should_stop)
}

// TODO get api_key if needed
// TODO get hostname from config, default to localhost
// TODO need to pass client config in here so it's configurable/testable.
fn format_messages_for_api(messages: &[Message]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|m| {
            if m.role == "tool" {
                // Tool results: content as structured array, with tool_call_id
                let mut msg = serde_json::json!({
                    "role": "tool",
                    "content": [{"type": "text", "text": m.content.as_deref().unwrap_or("")}]
                });
                if let Some(tool_call_id) = &m.tool_call_id {
                    msg["tool_call_id"] = serde_json::json!(tool_call_id);
                }
                msg
            } else if m.role == "assistant" && m.tool_calls.is_some() {
                // Assistant message with tool_calls
                let mut msg = serde_json::json!({
                    "role": "assistant",
                    "tool_calls": m.tool_calls.clone().unwrap_or_default()
                });
                if let Some(content) = &m.content {
                    if !content.trim().is_empty() {
                        msg["content"] = serde_json::json!([{"type": "text", "text": content}]);
                    } else {
                        msg["content"] = serde_json::Value::Null;
                    }
                } else {
                    msg["content"] = serde_json::Value::Null;
                }
                msg
            } else {
                // Regular messages: array format content
                if let Some(content) = &m.content {
                    serde_json::json!({
                        "role": m.role,
                        "content": [{"type": "text", "text": content}]
                    })
                } else {
                    serde_json::json!({
                        "role": m.role,
                        "content": serde_json::Value::Null
                    })
                }
            }
        })
        .collect()
}

async fn llm_request(
    messages: &[Message],
    port: u16,
    thinking_enabled: bool,
) -> anyhow::Result<(mpsc::Receiver<StreamEvent>, tokio::task::JoinHandle<()>)> {
    let (tx, rx) = mpsc::channel(100);
    let messages = messages.to_vec();

    let handle = tokio::spawn(async move {
        let client = reqwest::Client::new();

        let formatted_messages = format_messages_for_api(&messages);
        let mut body = serde_json::json!({
            "model": "qwen3.5-2b",
            "messages": formatted_messages,
            "stream": true,
            "stream_options": {
                "include_usage": true
            },
            "tools": crate::tools::get_tool_definitions(),
            "chat_template_kwargs": {
                "enable_thinking": thinking_enabled
            }
        });

        // Apply parameters based on thinking mode
        if thinking_enabled {
            // Thinking profile: encourage exploration and reasoning
            body["temperature"] = serde_json::json!(0.6);
            body["top_p"] = serde_json::json!(0.95);
            body["top_k"] = serde_json::json!(20);
            body["presence_penalty"] = serde_json::json!(0.0);
        } else {
            // Non-thinking profile: focus on coherence
            body["temperature"] = serde_json::json!(0.7);
            body["top_p"] = serde_json::json!(0.8);
            body["top_k"] = serde_json::json!(20);
            body["presence_penalty"] = serde_json::json!(1.5);
        }

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

                let mut full_content = String::new();

                while let Ok(Some(bytes)) = stream.try_next().await {
                    buffer.push_str(&String::from_utf8_lossy(&bytes));

                    // Process complete lines, keep incomplete ones in buffer
                    let lines: Vec<&str> = buffer.split('\n').collect();
                    for line in &lines[..lines.len() - 1] {
                        let _ = parse_line(line, &tx, &mut tool_calls_acc).await;
                        full_content.push_str(line);
                        full_content.push('\n');
                    }
                    // Keep the last (possibly incomplete) line
                    buffer = lines.last().unwrap_or(&"").to_string();
                }

                // Handle any remaining buffer
                if !buffer.is_empty() {
                    let _ = parse_line(&buffer, &tx, &mut tool_calls_acc).await;
                    full_content.push_str(&buffer);
                }

                // Determine format and parse accordingly
                // Qwen sends either JSON or XML format, not both
                let tool_calls = if has_xml_tool_calls(&full_content) {
                    // XML format detected, use XML parser
                    parse_xml_tool_calls(&full_content)
                } else if !tool_calls_acc.is_empty() {
                    // JSON format detected from streaming deltas
                    let mut sorted_calls: Vec<_> = tool_calls_acc.into_iter().collect();
                    sorted_calls.sort_by_key(|&(idx, _)| idx);

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
                        .collect()
                } else {
                    Vec::new()
                };

                // Send tool calls if any were found
                if !tool_calls.is_empty() {
                    let _ = tx.send(StreamEvent::ToolCalls(tool_calls)).await;
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

/// Detect if content contains XML-formatted tool calls
fn has_xml_tool_calls(content: &str) -> bool {
    content.contains("<tool_call>") && content.contains("</tool_call>")
}

/// Parse XML-formatted tool calls like:
/// <tool_call><function=Bash><parameter=command>...</parameter></function></tool_call>
pub fn parse_xml_tool_calls(content: &str) -> Vec<ToolCall> {
    let mut tool_calls = Vec::new();

    // Compile regexes once outside loops for efficiency
    let tool_call_re = match Regex::new(r"<tool_call>(.+?)</tool_call>") {
        Ok(re) => re,
        Err(_) => return tool_calls,
    };

    let func_re = match Regex::new(r"<function=(\w+)>") {
        Ok(re) => re,
        Err(_) => return tool_calls,
    };

    let param_re = match Regex::new(r"<parameter=(\w+)>(.+?)</parameter>") {
        Ok(re) => re,
        Err(_) => return tool_calls,
    };

    // Find all <tool_call>...</tool_call> blocks
    for cap in tool_call_re.captures_iter(content) {
        if let Some(block) = cap.get(1) {
            let block_text = block.as_str();

            // Extract function name from <function=Name>
            if let Some(func_cap) = func_re.captures(block_text)
                && let Some(func_name) = func_cap.get(1)
            {
                let name = func_name.as_str().to_string();

                // Extract parameters from <parameter=key>value</parameter>
                let mut args = serde_json::Map::new();
                for param_cap in param_re.captures_iter(block_text) {
                    if let (Some(key_match), Some(value_match)) =
                        (param_cap.get(1), param_cap.get(2))
                    {
                        let key = key_match.as_str().to_string();
                        let value = value_match.as_str().to_string();
                        args.insert(key, Value::String(value));
                    }
                }

                // Generate a stable ID (use hash of content)
                let id = format!("call_{}", tool_calls.len());

                tool_calls.push(ToolCall { id, name, args });
            }
        }
    }

    tool_calls
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
    {
        // Check for usage information (comes at the end of the stream)
        if let Some(usage) = json.get("usage") {
            let total_tokens = usage
                .get("total_tokens")
                .and_then(|t| t.as_u64())
                .unwrap_or(0) as usize;

            if total_tokens > 0 {
                tx.send(StreamEvent::Usage { total_tokens }).await?;
            }
            return Ok(());
        }

        let Some(delta) = json
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("delta"))
        else {
            return Ok(());
        };
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
