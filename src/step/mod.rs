mod bash;
mod http;
mod system;

pub use bash::BashVariant;
pub use http::HttpVariant;
pub use system::SystemVariant;

use regex::Regex;

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::time::delay_for;

use jmespath;

use lazy_static::lazy_static;
use log::debug;

use chashmap::CHashMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    pub output: Option<String>,
    pub error: Option<String>,
    pub duration: Duration,
}

#[derive(Default, Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub retry_count: usize,
    pub retry_delay_ms: usize,
    pub initial_delay_ms: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub name: String,
    pub description: Option<String>,
    pub run: RunType,
    pub filters: Vec<FilterType>,
    pub expect: ExpectType,
    pub do_output: bool,
    pub outcome: Option<Outcome>,
    pub retry: RetryPolicy,
    pub require: Vec<String>,
    pub required_by: Vec<String>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Requirement {
    Some(String),
    Many(Vec<String>),
}

impl Requirement {
    pub fn to_vec(&self) -> Vec<String> {
        match *self {
            Requirement::Some(ref string) => vec![string.clone()],
            Requirement::Many(ref vec) => vec.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunType {
    Step(String),
    Value(String),
    Bash(BashVariant),
    Http(HttpVariant),
    System(SystemVariant),
}

lazy_static! {
    pub static ref STEP_OUTPUT: CHashMap<String, String> = CHashMap::new();
}

impl RunType {
    pub async fn execute(
        &self,
        expect: ExpectType,
        filters: Vec<FilterType>,
        retry: RetryPolicy,
    ) -> Outcome {
        let start = Instant::now();

        if retry.initial_delay_ms > 0 {
            debug!("Initially Sleeping for {} ms", retry.initial_delay_ms);
            let delay = Duration::from_millis(retry.initial_delay_ms as u64);
            delay_for(delay).await;
        }

        let try_count = retry.retry_count + 1;

        let mut output = String::new();
        let mut error = String::new();
        let mut successful = false;

        'retry: for count in 0..try_count {
            //If this is a retry, sleep first before trying again
            if count > 0 {
                debug!("Retry {} of {}", count + 1, try_count - 1);

                if retry.retry_delay_ms > 0 {
                    debug!("Sleeping for {} ms", retry.retry_delay_ms);
                    let delay = Duration::from_millis(retry.retry_delay_ms as u64);
                    delay_for(delay).await;
                }
            }

            output = String::new();
            error = String::new();

            //Run the runner first
            match self.run().await {
                Ok(run_out) => {
                    output = run_out;
                    successful = true;
                }
                Err(run_err) => {
                    error = run_err;
                    successful = false;
                }
            }

            //If it's successful, run the filters, changing the output each iteration
            if successful {
                'filter: for filter in filters.iter() {
                    match filter.filter(&output) {
                        Ok(filter_out) => {
                            output = filter_out;
                        }
                        Err(filter_err) => {
                            error = filter_err;
                            successful = false;
                            break 'filter;
                        }
                    };
                }
            }

            //If it's still successful, do the check
            if successful {
                if let Err(check_err) = expect.check(&output) {
                    error = check_err;
                    successful = false;
                } else {
                    break 'retry;
                }
            }
        }

        let output_opt = match output.as_ref() {
            "" => None,
            _ => Some(output),
        };

        let error_opt = match successful {
            true => None,
            false => Some(error),
        };

        //Default Return
        return Outcome {
            output: output_opt,
            error: error_opt,
            duration: start.elapsed(),
        };
    }

    async fn run(&self) -> Result<String, String> {
        match *self {
            RunType::Step(ref val) => match STEP_OUTPUT.get(val) {
                Some(val) => Ok(val.to_string()),
                None => return Err(format!("Step {} could not be found", val)),
            },
            RunType::Value(ref val) => Ok(val.clone()),
            RunType::Bash(ref val) => val.run().await,
            RunType::Http(ref val) => val.run().await,
            RunType::System(ref val) => val.run().await,
        }
    }
}

impl Step {
    pub fn get_duration_ms(&self) -> f32 {
        match self.outcome {
            Some(ref outcome) => {
                let nanos = outcome.duration.subsec_nanos() as f32;
                (1000000000f32 * outcome.duration.as_secs() as f32 + nanos) / (1000000f32)
            }
            None => 0f32,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterType {
    NoOutput,
    Regex(RegexVariant),
    JmesPath(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RegexVariant {
    MatchOnly(String),
    Options(RegexOptions),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RegexOptions {
    matches: String,
    group: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExpectType {
    Anything,
    Matches(String),
    MatchesNot(String),
    GreaterThan(f64),
    LessThan(f64),
}

impl FilterType {
    fn filter(&self, val: &str) -> Result<String, String> {
        match *self {
            FilterType::NoOutput => Ok(String::from("")),
            FilterType::JmesPath(ref jmes) => {
                let expr = jmespath::compile(jmes)
                    .map_err(|err| format!("Could not compile jmespath:{}", err))?;

                let data = jmespath::Variable::from_json(val)
                    .map_err(|err| format!("Could not format as json:{}", err))?;

                let result = expr
                    .search(data)
                    .map_err(|err| format!("Could not find jmes expression:{}", err))?;

                let output = (*result).to_string();

                if output != "null" {
                    Ok(output)
                } else {
                    Err(format!(
                        "Could not find jmespath expression `{}` in output",
                        expr
                    ))
                }
            }
            FilterType::Regex(ref regex_var) => {
                let opts = match regex_var {
                    &RegexVariant::MatchOnly(ref string) => RegexOptions {
                        matches: string.clone(),
                        group: "0".into(),
                    },
                    &RegexVariant::Options(ref opts) => opts.clone(),
                };

                let regex = Regex::new(&opts.matches).map_err(|err| {
                    format!(
                        "Could not create regex from `{}`.  Error is:{:?}",
                        &opts.matches, err
                    )
                })?;

                let captures = regex
                    .captures(val)
                    .ok_or_else(|| format!("Could not find `{}` in output", &opts.matches))?;

                match opts.group.parse::<usize>() {
                    Ok(num) => {
                        return captures
                            .get(num)
                            .map(|val| val.as_str().into())
                            .ok_or_else(|| {
                                format!(
                                    "Could not find group number `{}` in regex `{}`",
                                    opts.group, opts.matches
                                )
                            });
                    }
                    Err(_) => {
                        return captures
                            .name(&opts.group)
                            .map(|val| val.as_str().into())
                            .ok_or_else(|| {
                                format!(
                                    "Could not find group name `{}` in regex `{}`",
                                    opts.group, opts.matches
                                )
                            });
                    }
                }
            }
        }
    }
}

impl ExpectType {
    fn check(&self, val: &str) -> Result<(), String> {
        let number_filter = Regex::new("[^-0-9.,]").unwrap();

        match *self {
            ExpectType::Anything => Ok(()),
            ExpectType::MatchesNot(ref match_string) => {
                let regex = Regex::new(match_string).map_err(|err| {
                    format!(
                        "Could not create regex from `{}`.  Error is:{:?}",
                        match_string, err
                    )
                })?;

                if !regex.is_match(val) {
                    Ok(())
                } else {
                    Err(format!("Matched against `{}`", match_string))
                }
            }
            ExpectType::Matches(ref match_string) => {
                let regex = Regex::new(match_string).map_err(|err| {
                    format!(
                        "Could not create regex from `{}`.  Error is:{:?}",
                        match_string, err
                    )
                })?;

                if regex.is_match(val) {
                    Ok(())
                } else {
                    Err(format!("Not matched against `{}`", match_string))
                }
            }
            ExpectType::GreaterThan(ref num) => {
                match number_filter.replace_all(val, "").parse::<f64>() {
                    Ok(compare) => {
                        if compare > *num {
                            Ok(())
                        } else {
                            Err(format!(
                                "The value `{}` is not greater than `{}`",
                                compare, num
                            ))
                        }
                    }
                    Err(_) => Err(format!("Could not parse `{}` as a number", val)),
                }
            }
            ExpectType::LessThan(ref num) => {
                match number_filter.replace_all(val, "").parse::<f64>() {
                    Ok(compare) => {
                        if compare < *num {
                            Ok(())
                        } else {
                            Err(format!(
                                "The value `{}` is not less than `{}`",
                                compare, num
                            ))
                        }
                    }
                    Err(_) => Err(format!("Could not parse `{}` as a number", num)),
                }
            }
        }
    }
}

impl Default for ExpectType {
    fn default() -> Self {
        ExpectType::Anything
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expect_negative_numbers() {
        let expect = ExpectType::LessThan(0.0);
        assert_eq!(expect.check("-1"), Ok(()));
        assert_eq!(expect.check("-1.0"), Ok(()));
        assert_eq!(expect.check("-.01"), Ok(()));
        assert_eq!(expect.check("-0.01"), Ok(()));

        let expect = ExpectType::GreaterThan(-2.0);
        assert_eq!(expect.check("-1"), Ok(()));
        assert_eq!(expect.check("-1.0"), Ok(()));
        assert_eq!(expect.check("-.01"), Ok(()));
        assert_eq!(expect.check("-0.01"), Ok(()));
    }
}
