use crate::Message;
use serde::Serialize;

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
}

pub async fn execute(input: &str, history: &mut Vec<Message>) -> anyhow::Result<()> {
    history.push(Message {
        role: "user".to_string(),
        content: input.to_string(),
    });

    match llm_request(history).await {
        Ok(response) => {
            println!("{}", response);

            history.push(Message {
                role: "assistant".to_string(),
                content: response,
            });
        }
        Err(e) => eprintln!("Could not communicate with LLM: {}", e),
    };

    Ok(())
}

async fn llm_request(messages: &[Message]) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    // TODO get api_key if needed
    // TODO get hostname from config, default to localhost

    let response = client
        .post("http://127.0.0.1:7777/v1/chat/completions")
        //.header("Authorization", format!("Bearer {}", api_key))
        .json(&ChatRequest {
            model: "qwen3.5-2b".to_string(),
            messages: messages.to_vec(),
        })
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    Ok(response["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string())
}
