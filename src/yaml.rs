
use std::fs::File;

use serde_yaml::{self,Value};
use tera::{Tera,Context};

use step::{RunType, Step, Requirement};
use std::collections::BTreeMap;


#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct StepYaml {
    run: RunType,
    require: Option<Requirement>,
    required_by: Option<Requirement>
}


pub fn get_steps(test_plan: &str, config: &str) -> Vec<Step> {

    let mut tera = Tera::default();

    tera.add_template_file(test_plan, Some("test_plan")).expect("Could not load test plan file!");


    let test_plan_yaml = match File::open(config) {
        Ok(config_file) => {
            let config: Value = serde_yaml::from_reader(config_file).expect("Could not read config file");
            tera.render("test_plan", &config).expect("Could not render the test plan with config!")
        },
        _ => {
            let context = Context::new();
            tera.render("test_plan", &context).expect("Could not render the test plan!")
        }
    };


    let input_steps: BTreeMap<String, StepYaml> = serde_yaml::from_str(&test_plan_yaml).unwrap();
    let mut steps: Vec<Step> =  Vec::new();

    for (name, step) in input_steps {
        steps.push(Step {
            name: name,
            run: step.run,
            require: step.require.map(|require| require.to_vec()).unwrap_or(Vec::new()),
            required_by: step.required_by.map(|require| require.to_vec()).unwrap_or(Vec::new()),
        });
    }

    steps
}