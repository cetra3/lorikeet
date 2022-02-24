use colored::*;
use reqwest::IntoUrl;
use serde::{Deserialize, Serialize};
use serde_json::json;

use std::convert::From;

use crate::step::Step;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StepResult {
    pub name: String,
    pub description: Option<String>,
    pub pass: bool,
    pub output: String,
    pub error: Option<String>,
    pub on_fail_output: Option<String>,
    pub on_fail_error: Option<String>,
    pub duration: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WebHook {
    hostname: String,
    has_errors: bool,
    tests: Vec<StepResult>,
}

pub async fn submit_slack<U: IntoUrl, I: Into<String>>(
    results: &[StepResult],
    url: U,
    hostname: I,
) -> Result<(), reqwest::Error> {
    let num_errors = results.iter().filter(|result| !result.pass).count();

    if num_errors == 0 {
        return Ok(());
    }

    let mut blocks = vec![];

    let title = format!(
        "{} Error{} from `{}`",
        num_errors,
        if num_errors == 1 { "" } else { "s" },
        hostname.into()
    );

    blocks.push(json!({
        "type": "header",
        "text": {
            "type": "plain_text",
            "text": &title,
            "emoji": true
        }
    }));

    for result in results.iter().filter(|result| !result.pass) {
        let mut text = format!("*Name*: {}", result.name);

        if let Some(ref val) = result.description {
            text.push_str(&format!(", *Description*: {}\n\n", val));
        } else {
            text.push_str("\n\n")
        }

        if let Some(ref val) = result.error {
            text.push_str(&format!("*Error*: {}\n\n", val));
        }

        if result.output.is_empty() {
            text.push_str(&format!("*Duration*: ({:.2}ms)\n\n", result.duration));
        } else {
            text.push_str(&format!("*Output*: ({:.2}ms)\n\n", result.duration));
        }

        blocks.push(json!({
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": truncate(&text, 3000)
            }
        }));

        if !result.output.is_empty() {
            blocks.push(json!({
                "type": "rich_text",
                "elements": [
                  {
                    "type": "rich_text_preformatted",
                    "elements": [
                      {
                        "type": "text",
                        "text": truncate(&result.output, 3000)
                      }
                    ]
                  }
                ]

            }));
        }
    }

    let payload = json!(
    {
        "text": &title,
        "blocks": blocks
    }
    );

    let client = reqwest::Client::new();

    let builder = client.post(url);

    let builder = builder.json(&payload);

    let response = builder.send().await?;

    if !response.status().is_success() {
        eprintln!("Error submitting slack webhook:");
        eprintln!("Status: {}", response.status());
        let val = response.text().await?;
        eprintln!("Body: {}", val);
    }

    Ok(())
}

pub async fn submit_webhook<U: IntoUrl, I: Into<String>>(
    results: &[StepResult],
    url: U,
    hostname: I,
) -> Result<(), reqwest::Error> {
    let has_errors = results.iter().any(|result| !result.pass);

    let payload = WebHook {
        hostname: hostname.into(),
        has_errors,
        tests: results.to_vec(),
    };

    let client = reqwest::Client::new();

    let builder = client.post(url);

    let builder = builder.json(&payload);

    let response = builder.send().await?;

    if !response.status().is_success() {
        eprintln!("Error submitting webhook:");
        eprintln!("Status: {}", response.status());
        let val = response.text().await?;
        eprintln!("Body: {}", val);
    }

    Ok(())
}

impl StepResult {
    pub fn terminal_print(&self, colours: &bool) {
        let mut message = format!("- name: {}\n", self.name);

        if let Some(ref description) = self.description {
            message.push_str(&format!("  description: {}\n", description))
        }

        message.push_str(&format!("  pass: {}\n", self.pass));

        if !self.output.is_empty() {
            if self.output.contains('\n') {
                message.push_str(&format!(
                    "  output: |\n    {}\n",
                    self.output.replace("\n", "\n    ")
                ));
            } else {
                message.push_str(&format!("  output: {}\n", self.output));
            }
        }
        if let Some(ref error) = self.error {
            message.push_str(&format!("  error: {}\n", error));
        }

        if let Some(ref output) = self.on_fail_output {
            if !output.trim().is_empty() {
                message.push_str(&format!("  on_fail_output: {}\n", output));
            }
        }

        if let Some(ref error) = self.on_fail_error {
            message.push_str(&format!("  on_fail_error: {}\n", error));
        }

        message.push_str(&format!("  duration: {}ms\n", self.duration));

        if *colours {
            match self.pass {
                true => {
                    println!("{}", message.green().bold());
                }
                false => {
                    println!("{}", message.red().bold());
                }
            }
        } else {
            println!("{}", message);
        }
    }
}

impl From<Step> for StepResult {
    fn from(step: Step) -> Self {
        let duration = step.get_duration_ms();
        let name = step.name;
        let description = step.description;

        let (pass, output, error, on_fail_output, on_fail_error) = match step.outcome {
            Some(outcome) => {
                let output = match step.do_output {
                    true => outcome.output.unwrap_or_default(),
                    false => String::new(),
                };

                (
                    outcome.error.is_none(),
                    output,
                    outcome.error,
                    outcome.on_fail_output,
                    outcome.on_fail_error,
                )
            }
            None => (
                false,
                String::new(),
                Some(String::from("Not finished")),
                None,
                None,
            ),
        };

        StepResult {
            name,
            duration,
            description,
            pass,
            output,
            on_fail_output,
            on_fail_error,
            error,
        }
    }
}

pub fn truncate(input: &str, len: usize) -> String {
    if input.len() <= len {
        return input.to_string();
    }

    let mut end_idx = len + 1;

    while !input.is_char_boundary(end_idx) {
        end_idx -= 1;
    }

    let slice = &input[0..end_idx];

    let mut end_idx = len;

    if let Some(val) = slice.rfind(char::is_whitespace) {
        end_idx = val;
    }

    return format!("{}...", &input[0..end_idx]);
}
