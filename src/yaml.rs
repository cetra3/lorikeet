
use std::fs::File;

use serde_yaml::{self,Value};
use tera::{Tera,Context};

use step::{RunType, ExpectType, Step, Requirement, BashVariant, HttpVariant};
use linked_hash_map::LinkedHashMap;


#[derive(Debug, PartialEq, Deserialize)]
struct StepYaml {
    value: Option<String>,
    bash: Option<BashVariant>,
    http: Option<HttpVariant>,
    matches: Option<String>,
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

    return RunType::Value(step.value.clone().unwrap_or(String::new()))
}

fn get_expecttype(step: &StepYaml) -> ExpectType {

    match step.matches {
        Some(ref string) => ExpectType::Matches(string.clone()),
        None => ExpectType::Anything
    }
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
            let context = Context::new();
            tera.render("test_plan", &context).expect("Could not render the test plan!")
        }
    };

    debug!("{}", test_plan_yaml);

    let input_steps: LinkedHashMap<String, StepYaml> = serde_yaml::from_str(&test_plan_yaml).unwrap();
    let mut steps: Vec<Step> =  Vec::new();



    for (name, step) in input_steps {
        steps.push(Step {
            name: name,
            run: get_runtype(&step),
            expect: get_expecttype(&step),
            require: step.require.map(|require| require.to_vec()).unwrap_or(Vec::new()),
            required_by: step.required_by.map(|require| require.to_vec()).unwrap_or(Vec::new()),
        });
    }

    steps
}
