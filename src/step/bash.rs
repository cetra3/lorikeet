use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BashVariant {
    CmdOnly(String),
    Options(BashOptions),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BashOptions {
    cmd: String,
    full_error: bool,
}

use std::process::Command;

impl BashVariant {
    pub async fn run(&self) -> Result<String, String> {
        let bashopts = match *self {
            BashVariant::CmdOnly(ref val) => BashOptions {
                cmd: val.clone(),
                full_error: false,
            },
            BashVariant::Options(ref opts) => opts.clone(),
        };

        tokio::task::spawn_blocking(move || {
            match Command::new("bash").arg("-c").arg(bashopts.cmd).output() {
                Ok(output) => {
                    if output.status.success() {
                        Ok(format!("{}", String::from_utf8_lossy(&output.stdout)))
                    } else {
                        if bashopts.full_error {
                            Err(format!(
                                "Status Code:{}\nError:{}\nOutput:{}",
                                output.status.code().unwrap_or(1),
                                String::from_utf8_lossy(&output.stderr),
                                String::from_utf8_lossy(&output.stdout)
                            ))
                        } else {
                            Err(String::from_utf8_lossy(&output.stderr).to_string())
                        }
                    }
                }
                Err(err) => Err(format!("Err:{:?}", err)),
            }
        })
        .await
        .map_err(|err| format!("{}", err))?
    }
}
