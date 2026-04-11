use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};

use crate::models::{AiSettings, CommitSuggestion, RemoteModelOption};

#[derive(Default)]
pub struct AiClient;

impl AiClient {
    pub fn new() -> Self {
        Self
    }

    pub fn generate_commit_message(
        &self,
        settings: &AiSettings,
        diff: &str,
    ) -> Result<CommitSuggestion> {
        let api_key = settings.api_key.trim();
        if api_key.is_empty() {
            bail!("AI API key is missing. Add one in settings before generating commit messages.");
        }

        let endpoint = settings.provider.default_endpoint();
        if endpoint.is_empty() {
            bail!("AI endpoint is missing.");
        }

        let model = settings.model.trim();
        if model.is_empty() {
            bail!("AI model is missing.");
        }

        let trimmed_diff = diff.trim();
        if trimmed_diff.is_empty() {
            bail!("No diff is available to summarize.");
        }

        let diff_payload = truncate_diff(trimmed_diff, 32_000);
        let payload = json!({
            "model": model,
            "temperature": 0.2,
            "messages": [
                {
                    "role": "system",
                    "content": settings.system_prompt.trim(),
                },
                {
                    "role": "user",
                    "content": format!(
                        "Generate a commit message for this git diff. Respond with JSON only. Do not include markdown bold markers like **. Do not include fenced code blocks like ```. Plain sentences and normal '-' or '.' list items in the body are allowed.\n\n{}",
                        diff_payload
                    ),
                }
            ]
        });

        let agent = ureq::Agent::new_with_config(
            ureq::config::Config::builder()
                .http_status_as_error(false)
                .build(),
        );

        let response = agent
            .post(endpoint)
            .header("Authorization", &format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "https://github.com/jacobsamro/gitspark")
            .header("X-Title", "GitSpark")
            .send_json(payload)
            .map_err(|error| anyhow!("AI request failed: {error}"))?;

        let status = response.status();

        let body = response
            .into_body()
            .read_to_string()
            .context("failed to read AI response body")?;

        if status.as_u16() >= 400 {
            // Try to extract error message from JSON response
            let error_msg = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| {
                    v.get("error")
                        .and_then(|e| e.get("message").or(Some(e)))
                        .and_then(Value::as_str)
                        .map(String::from)
                })
                .unwrap_or_else(|| body.chars().take(200).collect());
            bail!("AI request failed (HTTP {}): {}", status.as_u16(), error_msg);
        }

        let value: Value =
            serde_json::from_str(&body).context("failed to parse AI response as JSON")?;

        let content = extract_message_content(&value)
            .ok_or_else(|| anyhow!("AI response did not include a message choice"))?;

        parse_commit_suggestion(&content)
    }

    pub fn fetch_openrouter_models(&self) -> Result<Vec<RemoteModelOption>> {
        let response = ureq::get("https://openrouter.ai/api/v1/models")
            .header("Accept", "application/json")
            .call()
            .map_err(|error| anyhow!("OpenRouter models request failed: {error}"))?;

        let body = response
            .into_body()
            .read_to_string()
            .context("failed to read OpenRouter models response body")?;

        let value: Value =
            serde_json::from_str(&body).context("failed to parse OpenRouter models JSON")?;

        let models = value
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("OpenRouter models response did not include a data array"))?;

        let mut options = models
            .iter()
            .filter_map(|model| {
                let id = model.get("id").and_then(Value::as_str)?.trim();
                if id.is_empty() {
                    return None;
                }

                let name = model
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .unwrap_or(id);

                Some(RemoteModelOption {
                    id: id.to_string(),
                    name: name.to_string(),
                })
            })
            .collect::<Vec<_>>();

        options.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        options.dedup_by(|a, b| a.id == b.id);

        if options.is_empty() {
            bail!("OpenRouter models response did not include any usable models.");
        }

        Ok(options)
    }
}

fn truncate_diff(diff: &str, max_chars: usize) -> String {
    let mut truncated = String::new();
    for ch in diff.chars().take(max_chars) {
        truncated.push(ch);
    }

    if diff.chars().count() > max_chars {
        truncated.push_str("\n\n[diff truncated]");
    }

    truncated
}

fn extract_message_content(value: &Value) -> Option<String> {
    let content = value
        .get("choices")?
        .as_array()?
        .first()?
        .get("message")?
        .get("content")?;

    match content {
        Value::String(s) => Some(s.clone()),
        Value::Array(parts) => {
            let combined = parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("");
            if combined.is_empty() {
                None
            } else {
                Some(combined)
            }
        }
        _ => None,
    }
}

fn parse_commit_suggestion(content: &str) -> Result<CommitSuggestion> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        bail!("AI response was empty.");
    }

    if let Ok(suggestion) = parse_suggestion_json(trimmed) {
        return Ok(suggestion);
    }

    if let Some(json_block) = extract_json_block(trimmed) {
        if let Ok(suggestion) = parse_suggestion_json(json_block) {
            return Ok(suggestion);
        }
    }

    let mut lines = trimmed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let subject = lines
        .next()
        .map(clean_subject_line)
        .filter(|line| !line.is_empty())
        .ok_or_else(|| anyhow!("AI response did not include a commit subject"))?;
    let body = lines.collect::<Vec<_>>().join("\n");

    Ok(CommitSuggestion {
        subject: sanitize_commit_text(&subject),
        body: sanitize_commit_text(&body),
        raw: trimmed.to_string(),
    })
}

fn parse_suggestion_json(input: &str) -> Result<CommitSuggestion> {
    let value: Value = serde_json::from_str(input)?;

    if let Some(commit_message) = value
        .get("commit_message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty())
    {
        let (subject, body) = split_commit_message(commit_message)?;
        return Ok(CommitSuggestion {
            subject: sanitize_commit_text(&subject),
            body: sanitize_commit_text(&body),
            raw: input.trim().to_string(),
        });
    }

    let subject = value
        .get("subject")
        .or_else(|| value.get("title"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("missing subject field in AI JSON response"))?
        .to_string();
    let body = value
        .get("body")
        .or_else(|| value.get("description"))
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();

    Ok(CommitSuggestion {
        subject: sanitize_commit_text(&subject),
        body: sanitize_commit_text(&body),
        raw: input.trim().to_string(),
    })
}

fn split_commit_message(message: &str) -> Result<(String, String)> {
    let mut lines = message.lines().map(str::trim);
    let subject = lines
        .find(|line| !line.is_empty())
        .map(clean_subject_line)
        .filter(|line| !line.is_empty())
        .ok_or_else(|| anyhow!("AI response did not include a commit subject"))?;

    let remaining = lines.collect::<Vec<_>>().join("\n");
    Ok((subject, sanitize_commit_text(remaining.trim())))
}

fn sanitize_commit_text(input: &str) -> String {
    let mut text = input.trim().replace("```json", "").replace("```", "");
    text = text.replace("**", "");

    text.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn extract_json_block(input: &str) -> Option<&str> {
    if let Some(stripped) = input
        .strip_prefix("```json")
        .and_then(|s| s.strip_suffix("```"))
        .map(str::trim)
    {
        return Some(stripped);
    }

    let start = input.find('{')?;
    let end = input.rfind('}')?;
    if end <= start {
        return None;
    }

    Some(input[start..=end].trim())
}

fn clean_subject_line(line: &str) -> String {
    let line = line.trim_start_matches('-').trim_start_matches('*').trim();
    line.strip_prefix("subject:")
        .or_else(|| line.strip_prefix("Subject:"))
        .map(str::trim)
        .unwrap_or(line)
        .trim_matches('"')
        .to_string()
}
