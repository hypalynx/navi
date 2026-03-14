pub mod llm;
pub mod render;
pub mod repl;
pub mod tools;
pub use llm::{Message, StreamEvent, execute, parse_line};
pub use render::{ContentType, Renderer};
pub use tools::ToolCall;

const SYSTEM_PROMPT: &str = include_str!("system_prompt.md");

pub fn create_initial_history() -> Vec<Message> {
    vec![Message {
        role: "system".to_string(),
        content: Some(SYSTEM_PROMPT.to_string()),
        thinking: None,
        tool_calls: None,
        tool_call_id: None,
    }]
}
