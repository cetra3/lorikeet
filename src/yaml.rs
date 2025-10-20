use crate::step::FilterType;
use crate::step::RegexVariant;
use std::fs::File;

use log::debug;
use serde::{Deserialize, Serialize};

use serde_yaml::{self, Value};
use tera::{Context, Tera};

use std::path::Path;

use anyhow::{Error, anyhow};
use std::io::Read;

use crate::step::{
    BashVariant, DiskVariant, ExpectType, HttpVariant, Requirement, RetryPolicy, RunType, Step,
    SystemVariant,
};
use linked_hash_map::LinkedHashMap;

#[derive(Debug, PartialEq, Deserialize)]
struct StepYaml {
    description: Option<String>,
    value: Option<String>,
    bash: Option<BashVariant>,
    step: Option<String>,
    http: Option<HttpVariant>,
    system: Option<SystemVariant>,
    disk: Option<DiskVariant>,
    matches: Option<String>,
    matches_not: Option<String>,
    #[serde(default)]
    filters: Vec<FilterType>,
    jmespath: Option<String>,
    regex: Option<RegexVariant>,
    do_output: Option<bool>,
    less_than: Option<String>,
    greater_than: Option<String>,
    retry_count: Option<usize>,
    retry_delay_ms: Option<usize>,
    delay_ms: Option<usize>,
    on_fail: Option<RunType>,
    require: Option<Requirement>,
    required_by: Option<Requirement>,
}

fn get_retry_policy(step: &StepYaml) -> RetryPolicy {
    let retry_delay_ms = step.retry_delay_ms.unwrap_or_default();
    let retry_count = step.retry_count.unwrap_or_default();
    let initial_delay_ms = step.delay_ms.unwrap_or_default();

    RetryPolicy {
        retry_count,
        retry_delay_ms,
        initial_delay_ms,
    }
}

fn get_runtype(step: &StepYaml) -> RunType {
    if let Some(ref step) = step.step {
        return RunType::Step(step.clone());
    }

    if let Some(ref variant) = step.bash {
        return RunType::Bash(variant.clone());
    }

    if let Some(ref variant) = step.http {
        return RunType::Http(variant.clone());
    }

    if let Some(ref variant) = step.system {
        return RunType::System(variant.clone());
    }

    if let Some(ref variant) = step.disk {
        return RunType::Disk(variant.clone());
    }

    RunType::Value(step.value.clone().unwrap_or_default())
}

fn get_expecttype(step: &StepYaml) -> ExpectType {
    if let Some(ref string) = step.matches {
        return ExpectType::Matches(string.clone());
    }

    if let Some(ref string) = step.matches_not {
        return ExpectType::MatchesNot(string.clone());
    }

    if let Some(ref string) = step.greater_than {
        return ExpectType::GreaterThan(string.parse().expect("Could not parse number"));
    }

    if let Some(ref string) = step.less_than {
        return ExpectType::LessThan(string.parse().expect("Could not parse number"));
    }

    ExpectType::Anything
}

fn get_filters(step: &StepYaml) -> Vec<FilterType> {
    let mut filters: Vec<FilterType> = step.filters.clone();

    if let Some(ref jmespath) = step.jmespath {
        filters.push(FilterType::JmesPath(jmespath.clone()))
    };

    if let Some(ref variant) = step.regex {
        filters.push(FilterType::Regex(variant.clone()))
    };

    filters
}

pub fn get_steps_raw<T: Serialize>(yaml_contents: &str, context: &T) -> Result<Vec<Step>, Error> {
    let mut tera = Tera::default();

    tera.add_raw_template("test_plan", yaml_contents)?;

    let test_plan_yaml = tera.render("test_plan", &Context::from_serialize(context)?)?;

    debug!("YAML output:\n{}", test_plan_yaml);

    let input_steps: LinkedHashMap<String, StepYaml> = serde_yaml::from_str(&test_plan_yaml)?;
    let mut steps: Vec<Step> = Vec::new();

    for (name, step) in input_steps {
        let run = get_runtype(&step);

        let expect = get_expecttype(&step);

        let filters = get_filters(&step);

        let retry_policy = get_retry_policy(&step);

        steps.push(Step {
            name,
            run,
            on_fail: step.on_fail,
            do_output: step.do_output.unwrap_or(true),
            expect,
            description: step.description,
            filters,
            retry: retry_policy,
            outcome: None,
            require: step
                .require
                .map(|require| require.to_vec())
                .unwrap_or_default(),
            required_by: step
                .required_by
                .map(|require| require.to_vec())
                .unwrap_or_default(),
        });
    }

    Ok(steps)
}

//We use P & Q here so that when specialising file path and config path can be different types, i.e, a &str & Option<String> for instance..
pub fn get_steps<P: AsRef<Path>, Q: AsRef<Path>>(
    file_path: P,
    config_path: &Option<Q>,
) -> Result<Vec<Step>, Error> {
    let mut file_contents = String::new();

    let path_ref = file_path.as_ref();

    let mut f = File::open(path_ref)
        .map_err(|err| anyhow!("Could not open file {:?}: {}", path_ref, err))?;

    f.read_to_string(&mut file_contents)?;

    match *config_path {
        Some(ref path) => {
            let c = File::open(path)?;

            let value: Value = serde_yaml::from_reader(c).map_err(|err| {
                anyhow!(
                    "Could not parse config {:?} as YAML: {}",
                    path.as_ref(),
                    err
                )
            })?;

            get_steps_raw(&file_contents, &value)
                .map_err(|err| anyhow!("Could not parse file {:?}: {}", path_ref, err))
        }
        None => get_steps_raw(&file_contents, &Value::Mapping(serde_yaml::Mapping::new()))
            .map_err(|err| anyhow!("Could not parse file {:?}: {}", path_ref, err)),
    }
}
