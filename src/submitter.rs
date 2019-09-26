use colored::*;
use reqwest::IntoUrl;
use serde_derive::{Deserialize, Serialize};

use std::convert::From;

use crate::step::Step;

use reqwest;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StepResult {
    pub name: String,
    pub description: Option<String>,
    pub pass: bool,
    pub output: String,
    pub error: Option<String>,
    pub duration: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WebHook {
    hostname: String,
    has_errors: bool,
    tests: Vec<StepResult>,
}

pub fn submit_webhook<U: IntoUrl, I: Into<String>>(
    results: &Vec<StepResult>,
    url: U,
    hostname: I,
) -> Result<(), reqwest::Error> {
    let has_errors = results.iter().any(|result| result.pass == false);

    let payload = WebHook {
        hostname: hostname.into(),
        has_errors: has_errors,
        tests: results.clone(),
    };

    let client = reqwest::Client::new();

    let builder = client.post(url);

    let builder = builder.json(&payload);

    builder.send()?;

    Ok(())
}

impl StepResult {
    pub fn terminal_print(&self, colours: &bool) {
        let mut message = format!("- name: {}\n", self.name);

        if let Some(ref description) = self.description {
            message.push_str(&format!("  description: {}\n", description))
        }

        message.push_str(&format!("  pass: {}\n", self.pass));

        if self.output != "" {
            if self.output.contains("\n") {
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

        let (pass, output, error) = match step.outcome {
            Some(outcome) => {
                let output = match step.do_output {
                    true => outcome.output.unwrap_or_default(),
                    false => String::new(),
                };

                (outcome.error.is_none(), output, outcome.error)
            }
            None => (false, String::new(), Some(String::from("Not finished"))),
        };

        StepResult {
            name,
            duration,
            description,
            pass,
            output,
            error,
        }
    }
}
