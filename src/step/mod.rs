mod bash;
mod disk;
mod http;
mod system;

pub use bash::BashVariant;
pub use disk::DiskVariant;
pub use http::HttpVariant;
pub use system::SystemVariant;

use regex::Regex;

use serde::{Deserialize, Serialize};
use std::{
    sync::LazyLock,
    time::{Duration, Instant},
};
use tokio::time::sleep;

use tera::{Context, Tera};

use std::{borrow::Cow, collections::HashMap};

use jmespath::{self, Variable};

use log::debug;

use chashmap::CHashMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    pub output: Option<String>,
    pub error: Option<String>,
    pub on_fail_output: Option<String>,
    pub on_fail_error: Option<String>,
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
    pub on_fail: Option<RunType>,
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
    Disk(DiskVariant),
}

pub static STEP_OUTPUT: LazyLock<CHashMap<String, String>> = LazyLock::new(CHashMap::new);
static REGEX_OUTPUT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("\\$\\{(step_output.[^}]+)\\}").unwrap());

impl RunType {
    pub async fn execute(
        &self,
        expect: ExpectType,
        filters: Vec<FilterType>,
        retry: RetryPolicy,
        on_fail: Option<RunType>,
    ) -> Outcome {
        let start = Instant::now();

        if retry.initial_delay_ms > 0 {
            debug!("Initially Sleeping for {} ms", retry.initial_delay_ms);
            let delay = Duration::from_millis(retry.initial_delay_ms as u64);
            sleep(delay).await;
        }

        let try_count = retry.retry_count + 1;

        let mut output = String::new();
        let mut error = String::new();
        let mut on_fail_output = None;
        let mut on_fail_error = None;
        let mut successful = false;

        'retry: for count in 0..try_count {
            //If this is a retry, sleep first before trying again
            if count > 0 {
                debug!("Retry {} of {}", count + 1, try_count - 1);

                if retry.retry_delay_ms > 0 {
                    debug!("Sleeping for {} ms", retry.retry_delay_ms);
                    let delay = Duration::from_millis(retry.retry_delay_ms as u64);
                    sleep(delay).await;
                }
            }

            output = String::new();
            error = String::new();
            on_fail_output = None;
            on_fail_error = None;

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

            if !successful && let Some(ref on_fail_runner) = on_fail {
                match on_fail_runner.run().await {
                    Ok(val) => {
                        on_fail_output = Some(val);
                    }
                    Err(val) => on_fail_error = Some(val),
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
        Outcome {
            output: output_opt,
            error: error_opt,
            duration: start.elapsed(),
            on_fail_output,
            on_fail_error,
        }
    }

    async fn run(&self) -> Result<String, String> {
        match *self {
            RunType::Step(ref val) => match STEP_OUTPUT.get(val) {
                Some(val) => Ok(val.to_string()),
                None => Err(format!("Step {} could not be found", val)),
            },
            RunType::Value(ref val) => Ok(val.clone()),
            RunType::Bash(ref val) => val.run().await,
            RunType::Http(ref val) => val.run().await,
            RunType::System(ref val) => val.run().await,
            RunType::Disk(ref val) => val.run().await,
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
#[derive(Default)]
pub enum ExpectType {
    #[default]
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

                let data = Variable::from_json(val)
                    .map_err(|err| format!("Could not format as json:{}", err))?;

                let result = expr
                    .search(data)
                    .map_err(|err| format!("Could not find jmes expression:{}", err))?;

                let output = match &*result {
                    Variable::String(val) => val.clone(),
                    other => other.to_string(),
                };

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
                    RegexVariant::MatchOnly(string) => RegexOptions {
                        matches: string.clone(),
                        group: "0".into(),
                    },
                    RegexVariant::Options(opts) => opts.clone(),
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
                    Ok(num) => captures
                        .get(num)
                        .map(|val| val.as_str().into())
                        .ok_or_else(|| {
                            format!(
                                "Could not find group number `{}` in regex `{}`",
                                opts.group, opts.matches
                            )
                        }),
                    Err(_) => captures
                        .name(&opts.group)
                        .map(|val| val.as_str().into())
                        .ok_or_else(|| {
                            format!(
                                "Could not find group name `{}` in regex `{}`",
                                opts.group, opts.matches
                            )
                        }),
                }
            }
        }
    }
}

fn output_renderer(input: &str) -> Result<String, String> {
    let cow_body = REGEX_OUTPUT.replace_all(input, "{{$1}}");

    match cow_body {
        Cow::Borrowed(_) => Ok(input.to_string()),
        Cow::Owned(cow_body) => {
            let mut tera = Tera::default();

            tera.add_raw_template("step_body", &cow_body)
                .map_err(|err| format!("Template Error: {}", err))?;

            let step_output: HashMap<String, String> = STEP_OUTPUT.clone().into_iter().collect();

            let mut context = HashMap::new();
            context.insert("step_output", step_output);

            let body_rendered = tera
                .render(
                    "step_body",
                    &Context::from_serialize(&context)
                        .map_err(|err| format!("Context Error: {}", err))?,
                )
                .map_err(|err| format!("Template Rendering Error: {:?}", err))?;

            Ok(body_rendered)
        }
    }
}

static NUMBER_FILTER: LazyLock<Regex> = LazyLock::new(|| Regex::new("[^-0-9.,]").unwrap());

impl ExpectType {
    fn check(&self, val: &str) -> Result<(), String> {
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
                match NUMBER_FILTER.replace_all(val, "").parse::<f64>() {
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
                match NUMBER_FILTER.replace_all(val, "").parse::<f64>() {
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
