use term_painter::Color::*;
use term_painter::ToStyle;

use std::convert::From;

use step::Step;

use reqwest;

use hostname;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StepResult {
    name: String,
    description: Option<String>,
    pass: bool,
    output: String,
    duration: f32
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WebHook {
    hostname: String,
    has_errors: bool,
    tests: Vec<StepResult>
}

pub fn submit_webhook(results: &Vec<StepResult>, url: &str, hostname: Option<&str>) -> Result<(),reqwest::Error> {

    let hostname = match hostname {
        Some(hostname) => String::from(hostname),
        None => hostname::get_hostname().unwrap_or_else(||String::from(""))
    };

    let has_errors = results.iter().any(|result| result.pass == false);

    let client = reqwest::Client::new()?;

    let payload = WebHook {
        hostname: String::from(hostname),
        has_errors: has_errors,
        tests: results.clone()
    };


    let _ = client.post(url)?
        .json(&payload)?
        .send()?;

    Ok(())
}

impl StepResult {
    pub fn terminal_print(&self, colours: &bool) {

        let style = match self.pass {
            true => Green.bold(),
            false => Red.bold()
        };

        let mut message = format!("- name: {}\n", self.name);

        if let Some(ref description) = self.description {
            message.push_str(&format!("  description: {}\n", description))
        }

        message.push_str(&format!("  pass: {}\n", self.pass));

        if self.output != "" {

            if self.output.contains("\n") {
                message.push_str(&format!("  output: |\n    {}\n", self.output.replace("\n", "\n    ")));
            } else {
                message.push_str(&format!("  output: {}\n", self.output));
            }
        }

        message.push_str(&format!("  duration: {}ms\n", self.duration));

        if *colours {
            println!("{}", style.paint(message));
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

        let (pass, output) = match step.outcome {
            Some(outcome) => {
                match outcome.result {
                    Ok(result) => {
                        (true, result)
                    },
                    Err(result) => {
                        (false, result)
                    }
                }

            },
            None => (false, String::from("Not finished"))
        };

        StepResult {
            name: name,
            duration: duration,
            description: description,
            pass: pass,
            output: output
        }
    }

}
