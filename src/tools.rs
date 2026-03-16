use serde_json::Value;
use std::fs;
use std::path::Path;

const MAX_OUTPUT_LINES: usize = 500;
const MAX_OUTPUT_CONTEXT: usize = 50;
const MAX_LINE_WIDTH: usize = 2000;

// Helper: Truncate a single line to MAX_LINE_WIDTH
fn truncate_line(line: &str) -> String {
    if line.len() > MAX_LINE_WIDTH {
        format!("{}...", &line[..MAX_LINE_WIDTH])
    } else {
        line.to_string()
    }
}

// Helper: Format output lines, handling truncation when count exceeds MAX_OUTPUT_LINES
fn format_output_lines(lines: Vec<&str>) -> String {
    if lines.len() > MAX_OUTPUT_LINES {
        let head_lines = MAX_OUTPUT_LINES - MAX_OUTPUT_CONTEXT;
        let head = lines
            .iter()
            .take(head_lines)
            .map(|line| truncate_line(line))
            .collect::<Vec<_>>();
        let tail = lines
            .iter()
            .skip(lines.len() - MAX_OUTPUT_CONTEXT)
            .map(|line| truncate_line(line))
            .collect::<Vec<_>>();
        let skipped = lines.len() - head_lines - MAX_OUTPUT_CONTEXT;
        format!(
            "{}\n\n[... {} lines truncated ...]\n\n{}",
            head.join("\n"),
            skipped,
            tail.join("\n")
        )
    } else {
        lines
            .iter()
            .map(|line| truncate_line(line))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

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

pub fn execute_tool(tool: &ToolCall) -> (String, String) {
    match tool.name.as_str() {
        "Read" => execute_read(&tool.args),
        "Glob" => execute_glob(&tool.args),
        "Grep" => execute_grep(&tool.args),
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

            let mut result = format_output_lines(lines_from_offset);
            if offset > 0 {
                result.push_str(&format!(
                    "\n\n[Read from line {} to {} of {} total]",
                    offset + 1,
                    offset + lines_from_offset_count,
                    total_lines
                ));
            }

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
                let result = format_output_lines(matches.iter().map(|s| s.as_str()).collect());

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
        let result = format_output_lines(matches.iter().map(|s| s.as_str()).collect());

        (summary, result)
    }
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

            let result_text = format_output_lines(output_lines);

            (summary, result_text)
        }
        Err(e) => {
            let error = format!("Error executing command: {}", e);
            (error.clone(), error)
        }
    }
}
