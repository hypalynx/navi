use serde_json::Value;
use std::fs;
use std::path::Path;

const MAX_OUTPUT_LINES: usize = 500;
const MAX_OUTPUT_CONTEXT: usize = 50;
const MAX_LINE_WIDTH: usize = 2000;

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args: serde_json::Map<String, Value>,
}

pub fn get_tool_definitions() -> Vec<Value> {
    let json_str = include_str!("tool_definitions.json");
    serde_json::from_str(json_str).expect("Failed to parse tool_definitions.json")
}

pub async fn execute_tool(tool: &ToolCall) -> (String, String) {
    match tool.name.as_str() {
        "Read" => execute_read(&tool.args),
        "Glob" => execute_glob(&tool.args),
        "Grep" => execute_grep(&tool.args),
        "Webfetch" => execute_webfetch(&tool.args).await,
        "Bash" => execute_bash(&tool.args),
        _ => {
            let error = format!("Unknown tool: {}", tool.name);
            (error.clone(), error)
        }
    }
}

fn execute_read(args: &serde_json::Map<String, Value>) -> (String, String) {
    let path = match args.get("filePath").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            let error = "Error: 'filePath' parameter is required and must be a string".to_string();
            return (error.clone(), error);
        }
    };

    let offset = args
        .get("offset")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(1)
        .saturating_sub(1);

    let full_path = if Path::new(path).is_absolute() {
        path.to_string()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => {
                let full = cwd.join(path);
                match full.to_str() {
                    Some(p) => p.to_string(),
                    None => {
                        return ("Error: invalid path".to_string(), String::new());
                    }
                }
            }
            Err(e) => return (format!("Error: {}", e), String::new()),
        }
    };

    let filename = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path);

    match fs::read_to_string(&full_path) {
        Ok(content) => {
            let all_lines: Vec<&str> = content.lines().collect();
            let total_lines = all_lines.len();

            let lines_from_offset: Vec<&str> = all_lines.iter().skip(offset).cloned().collect();
            let lines_from_offset_count = lines_from_offset.len();

            let summary = if lines_from_offset_count > MAX_OUTPUT_LINES {
                let head_lines = MAX_OUTPUT_LINES - MAX_OUTPUT_CONTEXT;
                let lines_shown = head_lines + MAX_OUTPUT_CONTEXT;
                let end_line = offset + lines_shown;
                if offset > 0 {
                    format!(
                        "Reading {} (lines {}-{} of {})",
                        filename,
                        offset + 1,
                        end_line,
                        total_lines
                    )
                } else {
                    format!(
                        "Reading {} (lines 1-{} of {})",
                        filename, lines_shown, total_lines
                    )
                }
            } else {
                let end_line = offset + lines_from_offset_count;
                if offset > 0 {
                    format!(
                        "Reading {} (lines {}-{} of {})",
                        filename,
                        offset + 1,
                        end_line,
                        total_lines
                    )
                } else if total_lines == 1 {
                    format!("Reading {} (1 line)", filename)
                } else {
                    format!("Reading {} (lines 1-{})", filename, lines_from_offset_count)
                }
            };

            let result = if lines_from_offset_count > MAX_OUTPUT_LINES {
                let head_lines = MAX_OUTPUT_LINES - MAX_OUTPUT_CONTEXT;
                let head: Vec<String> = lines_from_offset
                    .iter()
                    .take(head_lines)
                    .map(|s| {
                        if s.len() > MAX_LINE_WIDTH {
                            format!("{}...", &s[..MAX_LINE_WIDTH])
                        } else {
                            s.to_string()
                        }
                    })
                    .collect();
                let tail: Vec<String> = lines_from_offset
                    .iter()
                    .skip(lines_from_offset_count - MAX_OUTPUT_CONTEXT)
                    .map(|s| {
                        if s.len() > MAX_LINE_WIDTH {
                            format!("{}...", &s[..MAX_LINE_WIDTH])
                        } else {
                            s.to_string()
                        }
                    })
                    .collect();
                let skipped = lines_from_offset_count - head_lines - MAX_OUTPUT_CONTEXT;
                let offset_note = if offset > 0 {
                    format!(" (starting from line {})", offset + 1)
                } else {
                    String::new()
                };
                format!(
                    "{}\n\n[... {} lines truncated{} ...]\n\n{}",
                    head.join("\n"),
                    skipped,
                    offset_note,
                    tail.join("\n")
                )
            } else {
                let lines_str = lines_from_offset
                    .iter()
                    .map(|s| {
                        if s.len() > MAX_LINE_WIDTH {
                            format!("{}...", &s[..MAX_LINE_WIDTH])
                        } else {
                            s.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if offset > 0 {
                    format!(
                        "{}\n\n[Read from line {} to {} of {} total]",
                        lines_str,
                        offset + 1,
                        offset + lines_from_offset_count,
                        total_lines
                    )
                } else {
                    lines_str
                }
            };

            (summary, result)
        }
        Err(e) => {
            let error = format!("Error reading file '{}': {}", path, e);
            (error.clone(), error)
        }
    }
}

fn execute_glob(args: &serde_json::Map<String, Value>) -> (String, String) {
    let pattern = match args.get("pattern").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            let error = "Error: 'pattern' parameter is required".to_string();
            return (error.clone(), String::new());
        }
    };

    match glob::glob(pattern) {
        Ok(paths) => {
            let mut matches = Vec::new();
            for entry in paths.flatten() {
                if let Some(path_str) = entry.to_str() {
                    matches.push(path_str.trim_start_matches("./").to_string());
                }
            }
            matches.sort();

            if matches.is_empty() {
                let result = format!("No files match pattern: {}", pattern);
                (result.clone(), result)
            } else {
                let total_matches = matches.len();
                let summary = format!("Found {} files matching '{}'", total_matches, pattern);

                let result = if total_matches > MAX_OUTPUT_LINES {
                    let truncated: Vec<String> = matches
                        .iter()
                        .take(MAX_OUTPUT_LINES)
                        .map(|s| {
                            if s.len() > MAX_LINE_WIDTH {
                                format!("{}...", &s[..MAX_LINE_WIDTH])
                            } else {
                                s.clone()
                            }
                        })
                        .collect();
                    let skipped = total_matches - MAX_OUTPUT_LINES;
                    format!(
                        "{}\n\n[... {} more files ...]",
                        truncated.join("\n"),
                        skipped
                    )
                } else {
                    matches
                        .iter()
                        .map(|s| {
                            if s.len() > MAX_LINE_WIDTH {
                                format!("{}...", &s[..MAX_LINE_WIDTH])
                            } else {
                                s.clone()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };

                (summary, result)
            }
        }
        Err(e) => {
            let error = format!("Error with glob pattern '{}': {}", pattern, e);
            (error, String::new())
        }
    }
}

fn execute_grep(args: &serde_json::Map<String, Value>) -> (String, String) {
    let pattern = match args.get("pattern").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            let error = "Error: 'pattern' parameter is required".to_string();
            return (error, String::new());
        }
    };

    let files_pattern = match args.get("files").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            let error = "Error: 'files' parameter is required".to_string();
            return (error, String::new());
        }
    };

    let regex = match regex::Regex::new(pattern) {
        Ok(r) => r,
        Err(e) => {
            let error = format!("Invalid regex pattern: {}", e);
            return (error.clone(), String::new());
        }
    };

    let file_paths = match glob::glob(files_pattern) {
        Ok(paths) => paths.flatten().collect::<Vec<_>>(),
        Err(e) => {
            let error = format!("Error with glob pattern '{}': {}", files_pattern, e);
            return (error, String::new());
        }
    };

    if file_paths.is_empty() {
        return (
            format!("No files match pattern: {}", files_pattern),
            String::new(),
        );
    }

    let mut matches = Vec::new();
    let file_count = file_paths.len();

    for file_path in file_paths {
        if !file_path.is_file() {
            continue;
        }

        if let Ok(content) = fs::read_to_string(&file_path) {
            for (line_num, line) in content.lines().enumerate() {
                if regex.is_match(line) {
                    let path_str = file_path.to_string_lossy();
                    matches.push(format!("{}:{}: {}", path_str, line_num + 1, line));
                }
            }
        }
    }

    if matches.is_empty() {
        let result = format!("No matches found for pattern: {}", pattern);
        (result.clone(), result)
    } else {
        let total_matches = matches.len();
        let summary = format!("Found {} matches in {} files", total_matches, file_count);

        let result = if total_matches > MAX_OUTPUT_LINES {
            let truncated: Vec<String> = matches
                .iter()
                .take(MAX_OUTPUT_LINES)
                .map(|s| {
                    if s.len() > MAX_LINE_WIDTH {
                        format!("{}...", &s[..MAX_LINE_WIDTH])
                    } else {
                        s.clone()
                    }
                })
                .collect();
            let skipped = total_matches - MAX_OUTPUT_LINES;
            format!(
                "{}\n\n[... {} matches truncated ...]",
                truncated.join("\n"),
                skipped
            )
        } else {
            matches
                .iter()
                .map(|s| {
                    if s.len() > MAX_LINE_WIDTH {
                        format!("{}...", &s[..MAX_LINE_WIDTH])
                    } else {
                        s.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        (summary, result)
    }
}

async fn execute_webfetch(args: &serde_json::Map<String, Value>) -> (String, String) {
    const MAX_CONTENT: usize = 32768;

    let url = match args.get("url").and_then(|v| v.as_str()) {
        Some(u) => u,
        None => {
            let error = "Error: 'url' parameter is required".to_string();
            return (error.clone(), error);
        }
    };

    let domain = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(url);

    let client = reqwest::Client::new();
    match client
        .get(url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
    {
        Ok(response) => match response.text().await {
            Ok(content) => {
                let text = strip_html_tags(&content);
                let result = if text.len() > MAX_CONTENT {
                    format!("{}\n[Content truncated at 32KB]", &text[..MAX_CONTENT])
                } else {
                    text
                };

                let summary = format!("Fetching {}", domain);
                (summary, result)
            }
            Err(e) => {
                let error = format!("Error reading response: {}", e);
                (error.clone(), error)
            }
        },
        Err(e) => {
            let error = format!("HTTP request failed: {}", e);
            (error.clone(), error)
        }
    }
}

fn strip_html_tags(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;

    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    let lines: Vec<&str> = result.lines().filter(|l| !l.trim().is_empty()).collect();
    lines.join("\n")
}

fn validate_bash_command(command: &str) -> Result<(), String> {
    let cmd_lower = command.to_lowercase();

    // Blocked commands - all dangerous operations are denied
    let blocked = [
        ("dd", "disk write operations (data destruction risk)"),
        ("mkfs", "filesystem formatting (irreversible)"),
        ("reboot", "system reboot (would interrupt session)"),
        ("shutdown", "system shutdown (would interrupt session)"),
        ("rm ", "file deletion (data loss risk)"),
        ("rm\t", "file deletion (data loss risk)"),
        ("mv ", "file move/rename (could overwrite data)"),
        ("truncate", "file truncation (destructive)"),
        ("git push --force", "force git push (overwrites history)"),
        ("git push -f", "force git push (overwrites history)"),
        (" | bash", "pipe to bash (code injection risk)"),
        (" | sh", "pipe to shell (code injection risk)"),
    ];

    for (pattern, reason) in &blocked {
        if is_command_match(&cmd_lower, pattern) {
            return Err(format!("Command blocked for safety: {}", reason));
        }
    }

    Ok(())
}

fn is_command_match(command: &str, pattern: &str) -> bool {
    // Check if pattern appears as a command (beginning of string or after operators)
    if command.starts_with(pattern) {
        return true;
    }

    // Check after common operators: ;, |, &, $(), ``, etc
    for operator in &["; ", "| ", "& ", "$( ", "` ", "\t", "\n"] {
        if let Some(pos) = command.find(operator) {
            let after = &command[pos + operator.len()..];
            if after.starts_with(pattern) {
                return true;
            }
        }
    }

    false
}

fn execute_bash(args: &serde_json::Map<String, Value>) -> (String, String) {
    let command = match args.get("command").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            let error = "Error: 'command' parameter is required".to_string();
            return (error.clone(), error);
        }
    };

    // Validate command for dangerous operations
    if let Err(reason) = validate_bash_command(command) {
        return (reason.clone(), reason);
    }

    // Execute the command
    match std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            let result = if output.status.success() {
                stdout
            } else if !stderr.is_empty() {
                stderr
            } else {
                format!("Command exited with status: {}", output.status)
            };

            let output_lines: Vec<&str> = result.lines().collect();
            let total_lines = output_lines.len();

            let summary = if total_lines > MAX_OUTPUT_LINES {
                format!(
                    "Command output ({} lines, showing first {})",
                    total_lines,
                    MAX_OUTPUT_LINES - MAX_OUTPUT_CONTEXT
                )
            } else {
                format!("Command output ({} lines)", total_lines)
            };

            let result_text = if total_lines > MAX_OUTPUT_LINES {
                let head_lines = MAX_OUTPUT_LINES - MAX_OUTPUT_CONTEXT;
                let head: Vec<String> = output_lines
                    .iter()
                    .take(head_lines)
                    .map(|s| {
                        if s.len() > MAX_LINE_WIDTH {
                            format!("{}...", &s[..MAX_LINE_WIDTH])
                        } else {
                            s.to_string()
                        }
                    })
                    .collect();
                let tail: Vec<String> = output_lines
                    .iter()
                    .skip(total_lines - MAX_OUTPUT_CONTEXT)
                    .map(|s| {
                        if s.len() > MAX_LINE_WIDTH {
                            format!("{}...", &s[..MAX_LINE_WIDTH])
                        } else {
                            s.to_string()
                        }
                    })
                    .collect();
                let skipped = total_lines - head_lines - MAX_OUTPUT_CONTEXT;
                format!(
                    "{}\n\n[... {} lines truncated ...]\n\n{}",
                    head.join("\n"),
                    skipped,
                    tail.join("\n")
                )
            } else {
                output_lines
                    .iter()
                    .map(|s| {
                        if s.len() > MAX_LINE_WIDTH {
                            format!("{}...", &s[..MAX_LINE_WIDTH])
                        } else {
                            s.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };

            (summary, result_text)
        }
        Err(e) => {
            let error = format!("Error executing command: {}", e);
            (error.clone(), error)
        }
    }
}
