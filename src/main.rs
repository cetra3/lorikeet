extern crate lorikeet;

#[macro_use]
extern crate structopt_derive;
extern crate structopt;

#[macro_use] extern crate log;
extern crate env_logger;


use structopt::StructOpt;

use lorikeet::yaml::get_steps;
use lorikeet::runner::run_steps;

#[derive(StructOpt, Debug)]
#[structopt(name = "lorikeet", about = "a parallel test runner for devops")]
struct Arguments {
    #[structopt(short = "c", long = "config", help = "Configuration File", default_value = "config.yml")]
    config: String,

    #[structopt(short = "t", long = "testplan", help = "Test Plan", default_value = "test.yml")]
    test_plan: String,
}

fn main() {

    let opt = Arguments::from_args();

    env_logger::init();


    let steps = get_steps(&opt.test_plan, &opt.config);

    info!("Steps:{:?}", steps);

    let steps_status = run_steps(&steps);

    info!("Finished.  Status is:{:?}", steps_status);

}


