
use step::RegexVariant;
use step::FilterType;
use std::fs::File;

use serde_yaml::{self,Value};
use tera::{Tera,Context};

use step::{RunType, ExpectType, Step, Requirement, BashVariant, HttpVariant, SystemVariant};
use linked_hash_map::LinkedHashMap;


#[derive(Debug, PartialEq, Deserialize)]
struct StepYaml {
    description: Option<String>,
    value: Option<String>,
    bash: Option<BashVariant>,
    http: Option<HttpVariant>,
    system: Option<SystemVariant>,
    matches: Option<String>,
    #[serde(default)]
    filters: Vec<FilterType>,
    jmespath: Option<String>,
    regex: Option<RegexVariant>,
    do_output: Option<bool>,
    less_than: Option<String>,
    greater_than: Option<String>,
    require: Option<Requirement>,
    required_by: Option<Requirement>
}

fn get_runtype(step: &StepYaml) -> RunType {

    if let Some(ref variant) = step.bash {
        return RunType::Bash(variant.clone())
    }

    if let Some(ref variant) = step.http {
        return RunType::Http(variant.clone())
    }

    if let Some(ref variant) = step.system {
        return RunType::System(variant.clone())
    }

    return RunType::Value(step.value.clone().unwrap_or(String::new()))
}

fn get_expecttype(step: &StepYaml) -> ExpectType {

    if let Some(ref string) = step.matches {
        return ExpectType::Matches(string.clone())
    }

    if let Some(ref string) = step.greater_than {
        return ExpectType::GreaterThan(string.parse().expect("Could not parse number"))
    }

    if let Some(ref string) = step.less_than {
        return ExpectType::LessThan(string.parse().expect("Could not parse number"))
    }

    return ExpectType::Anything
}

fn get_filters(step: &StepYaml) -> Vec<FilterType> {

    let mut filters: Vec<FilterType> = step.filters.clone();

    if let Some(ref jmespath) = step.jmespath {
        filters.push(FilterType::JmesPath(jmespath.clone()))
    };

    if let Some(ref variant) = step.regex {
        filters.push(FilterType::Regex(variant.clone()))
    };

    if let Some(output) = step.do_output {
        if output == false {
                filters.push(FilterType::NoOutput)
        }
    };

    return filters
}


pub fn get_steps(test_plan: &str, config: &Option<String>) -> Vec<Step> {

    let mut tera = Tera::default();


    tera.add_template_file(test_plan, Some("test_plan")).expect("Could not load test plan file!");

    let test_plan_yaml = match *config {
        Some(ref config_file) => {

            let config_file = File::open(&config_file).expect("Could not open config file");

            let config: Value = serde_yaml::from_reader(config_file).expect("Could not read config file as yaml");

            tera.render("test_plan", &config).expect("Could not render the test plan with config!")
        },
        None => {
            let mut context = Context::new();
            tera.render("test_plan", &context).expect("Could not render the test plan!")
        }
    };

    debug!("{}", test_plan_yaml);

    let input_steps: LinkedHashMap<String, StepYaml> = serde_yaml::from_str(&test_plan_yaml).unwrap();
    let mut steps: Vec<Step> =  Vec::new();
    


    for (name, step) in input_steps {

        let run = get_runtype(&step);

        let expect = get_expecttype(&step);

        let filters = get_filters(&step);

        steps.push(Step {
            name: name,
            run: run,
            expect: expect,
            description: step.description,
            filters: filters,
            outcome: None,
            require: step.require.map(|require| require.to_vec()).unwrap_or(Vec::new()),
            required_by: step.required_by.map(|require| require.to_vec()).unwrap_or(Vec::new()),
        });
    }

    steps
}
