extern crate lorikeet;

#[macro_use]
extern crate structopt;

extern crate serde_yaml;

extern crate isatty;

#[macro_use] extern crate log;
extern crate env_logger;

extern crate openssl_probe;

use structopt::StructOpt;


use isatty::stdout_isatty;

use lorikeet::yaml::get_steps;
use lorikeet::runner::run_steps;
use lorikeet::submitter::StepResult;


#[derive(StructOpt, Debug)]
#[structopt(name = "lorikeet", about = "a parallel test runner for devops")]
struct Arguments {
    #[structopt(short = "q", long = "quiet", help = "Don't output results to console")]
    quiet: bool,

    #[structopt(short = "c", long = "config", help = "Configuration File")]
    config: Option<String>,

    #[structopt(help = "Test Plan", default_value = "test.yml")]
    test_plan: String,

    #[structopt(short = "w", long = "webhook", help = "Webhook submission URL")]
    webhook: Vec<String>,
}

fn main() {

    openssl_probe::init_ssl_cert_env_vars();

    let opt = Arguments::from_args();

    env_logger::init();

    let mut steps = get_steps(&opt.test_plan, &opt.config).expect("Could not get steps");

    debug!("Steps:{:?}", steps);

    let outcomes = run_steps(&mut steps);

    debug!("Finished.  Outcomes are:{:?}", outcomes);

    let mut has_errors = false;

    let colours = stdout_isatty();

    let mut results = Vec::new();

    for step in steps.into_iter() {

        if let Some(ref outcome) = step.outcome {
            if let Some(_) = outcome.error {
                has_errors = true;
            }
        }

        let result = StepResult::from(step);
        if !opt.quiet {
            result.terminal_print(&colours);
        }

        results.push(result);
    }

    debug!("Steps finished! Submitting webhooks");

    for url in opt.webhook {
        lorikeet::submitter::submit_webhook(&results, &url, None).expect("Could not send webhook")
    }


    if has_errors {
        std::process::exit(1)
    }

}


