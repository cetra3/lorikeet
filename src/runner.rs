use step::FilterType;
use std::sync::mpsc::Sender;

use std::sync::{Arc, Mutex};
use std::sync::mpsc::channel;
use std::time::Duration;

use step::{Step, Outcome, RunType, ExpectType, RetryPolicy};

use graph::{create_graph, Require};
use petgraph::prelude::GraphMap;
use petgraph::{Directed, Direction};


use std::collections::HashMap;

use threadpool::ThreadPool;


pub struct StepRunner<'a> {
    pub run: RunType,
    pub expect: ExpectType,
    pub retry: RetryPolicy,
    pub filters: Vec<FilterType>,
    pub graph: Arc<GraphMap<usize, Require, Directed>>,
    pub steps: Arc<Mutex<Vec<Status>>>,
    pub pool: ThreadPool,
    pub name_lookup: Arc<HashMap<&'a str, usize>>,
    pub index: usize,
    pub notify: Sender<usize>
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub enum Status {
    InProgress,
    Outstanding,
    Completed(Outcome)
}

impl<'a> StepRunner<'a> {

    pub fn poll(&self) {

        debug!("Poll received for `{}`", self.index);

        match self.steps.lock().unwrap()[self.index] {
                //If it's already completed, return
                Status::Completed(_) => {
                    return;
                }
                _ => ()
        }

        let mut has_error = false;

        for neighbor in self.graph.neighbors_directed(self.index, Direction::Incoming) {
            match self.steps.lock().unwrap()[neighbor] {
                Status::Completed(ref status_outcome) => {

                    if let Some(_) = status_outcome.error {
                        self.notify.send(self.index).expect("Could not notify executor");
                        has_error = true;
                        break;
                    }

                },
                _ => {
                    debug!("Neighbor {} isn't completed for {}, skipping", neighbor, self.index);
                    return;
                }
            };
        }

        if has_error {
            self.steps.lock().unwrap()[self.index] = Status::Completed(Outcome { output: Some("".into()), error: Some("Dependency Not Met".into()), duration: Duration::from_secs(0) });
            return;
        }

        if self.steps.lock().unwrap()[self.index] == Status::Outstanding {

            self.steps.lock().unwrap()[self.index] = Status::InProgress;

            let mut run = self.run.clone();

            //If the run type is `step`, we need to get the output of this step
            if let RunType::Step(ref step) = self.run {
                let step_ref: &str = step;
                let run_index = self.name_lookup.get(&*step_ref).unwrap();

                if let Status::Completed(ref outcome) = self.steps.lock().unwrap()[*run_index] {
                    if let Some(ref output) = outcome.output {
                        run = RunType::Step(output.clone());
                    }
                }
            }

            let expect = self.expect.clone();
            let retry = self.retry;
            let tx = self.notify.clone();
            let index = self.index;
            let steps = self.steps.clone();
            let filters = self.filters.clone();

            //let task = task::current();
            self.pool.execute(move || {

                let outcome = run.execute(expect, filters, retry);
                debug!("Step `{}` done: {:?}", index, outcome);
                steps.lock().unwrap()[index] = Status::Completed(outcome);
                tx.send(index).expect("Could not notify executor");

            });
        }
    }

}

pub fn run_steps(steps: &mut Vec<Step>) {

    let steps_status:  Arc<Mutex<Vec<Status>>> = Arc::new(Mutex::new(vec![Status::Outstanding; steps.len()]));


    //We want the runners to drop after this so we can return the steps status
    {
        let mut lookup: HashMap<&str, usize> = HashMap::new();

        for i in 0..steps.len() {
            lookup.insert(&steps[i].name, i);
        }

        let name_lookup = Arc::new(lookup);

        let shared_graph = Arc::new(create_graph(&steps));

        let mut runners = Vec::new();

        let (tx, rx) = channel();
        let threadpool = ThreadPool::default();

        for i in 0..steps.len() {

            let future = StepRunner {
                run: steps[i].run.clone(),
                expect: steps[i].expect.clone(),
                retry: steps[i].retry,
                filters: steps[i].filters.clone(),
                graph: shared_graph.clone(),
                steps: steps_status.clone(),
                index: i,
                name_lookup: name_lookup.clone(),
                notify: tx.clone(),
                pool: threadpool.clone()
            };

            runners.push(future);
        }

        //Kick off the process
        for runner in runners.iter_mut() {
            runner.poll();
        }

        for _ in 0..steps.len() {

            let finished = rx.recv().expect("Could not receive notification");

            for neighbor in shared_graph.neighbors_directed(finished, Direction::Outgoing) {
                runners[neighbor].poll();
            };
        };

        threadpool.join();

    }

    let steps_ptr = Arc::try_unwrap(steps_status).expect("Could not retrieve the status list");

    for (i, status) in steps_ptr.into_inner().expect("Could not free mutex").into_iter().enumerate() {

        if let Status::Completed(outcome) = status {
            steps[i].outcome = Some(outcome);
        }

    }

}
