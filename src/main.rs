extern crate lorikeet;

#[macro_use]
extern crate structopt_derive;
extern crate structopt;

extern crate serde_yaml;

extern crate isatty;

#[macro_use] extern crate log;
extern crate env_logger;

use structopt::StructOpt;


use isatty::stdout_isatty;

use lorikeet::yaml::get_steps;
use lorikeet::runner::run_steps;


#[derive(StructOpt, Debug)]
#[structopt(name = "lorikeet", about = "a parallel test runner for devops")]
struct Arguments {
    #[structopt(short = "c", long = "config", help = "Configuration File")]
    config: Option<String>,

    #[structopt(help = "Test Plan", default_value = "test.yml")]
    test_plan: String,
}

fn main() {

    let opt = Arguments::from_args();

    env_logger::init().expect("Could not initialise logger");

    let steps = get_steps(&opt.test_plan, &opt.config);

    debug!("Steps:{:?}", steps);

    let outcomes = run_steps(&steps);

    debug!("Finished.  Outcomes are:{:?}", outcomes);

    let mut has_errors = false;

    let colours = stdout_isatty();

    for (i, outcome) in outcomes.iter().enumerate() {

        if let Err(_) = outcome.result {
            has_errors = true;
        }

        outcome.terminal_print(&steps[i], &colours);
    }

    if has_errors {
        std::process::exit(1)
    }

}


