use std::process::Command;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Status {
    InProgress,
    Outstanding,
    Completed(Result<String,String>)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub name: String,
    pub run: RunType,
    pub require: Vec<String>,
    pub required_by: Vec<String>
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Requirement {
    Some(String),
    Many(Vec<String>)
}

impl Requirement {
    pub fn to_vec(&self) -> Vec<String> {
        match *self {
            Requirement::Some(ref string) => vec![string.clone()],
            Requirement::Many(ref vec) => vec.clone()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RunType {
    #[serde(rename = "is_true")]
    IsTrue(String),
    #[serde(rename = "bash")]
    Bash(String)
}

impl RunType {
    pub fn execute(&self) -> Status {
        match *self {
            RunType::IsTrue(ref val) => {
                match &(val.to_lowercase()) as &str {
                    "1" | "true" => {
                        return Status::Completed(Ok(String::from("Matched")));
                    },
                    _ => {
                        return Status::Completed(Err(String::from("Not a truthy value")));
                    }
                };
            },
            RunType::Bash(ref val) => {
                match Command::new("bash").arg("-c").arg(val).output() {
                    Ok(output) => {
                        if output.status.success() {
                            return Status::Completed(Ok(format!("{}", String::from_utf8_lossy(&output.stdout))));
                        } else {
                            return Status::Completed(Err(format!("Exit Code:{}, StdErr:{}, StdOut:{}", output.status.code().unwrap_or(1), String::from_utf8_lossy(&output.stderr), String::from_utf8_lossy(&output.stdout))));
                        }
                    },
                    Err(err) => {
                        return Status::Completed(Err(format!("Err:{:?}", err)));
                    }
                }
            }
        }
    }
}
