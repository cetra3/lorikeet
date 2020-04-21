use crate::step::FilterType;

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::sync::Mutex;

use crate::step::{ExpectType, Outcome, RetryPolicy, RunType, Step, STEP_OUTPUT};

use crate::graph::{create_graph, Require};
use petgraph::prelude::GraphMap;
use petgraph::{Directed, Direction};

use serde::Deserialize;

use log::debug;

use anyhow::{anyhow, Error};

use chashmap::CHashMap;

pub struct StepRunner<'a> {
    pub run: RunType,
    pub expect: ExpectType,
    pub retry: RetryPolicy,
    pub filters: Vec<FilterType>,
    pub graph: Arc<GraphMap<usize, Require, Directed>>,
    pub steps: Arc<Mutex<Vec<Status>>>,
    pub name: &'a str,
    pub name_lookup: &'a CHashMap<&'a str, usize>,
    pub index: usize,
    pub notify: UnboundedSender<usize>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub enum Status {
    InProgress,
    Outstanding,
    Completed(Outcome),
}

impl<'a> StepRunner<'a> {
    pub async fn poll(&self) {
        debug!("Poll received for `{}`", self.index);

        let mut cur_steps = self.steps.lock().await;

        match cur_steps[self.index] {
            //If it's already completed, return
            Status::Completed(_) => {
                return;
            }
            _ => (),
        }

        let mut has_error = false;

        for neighbor in self
            .graph
            .neighbors_directed(self.index, Direction::Incoming)
        {
            match cur_steps[neighbor] {
                Status::Completed(ref status_outcome) => {
                    if let Some(_) = status_outcome.error {
                        self.notify
                            .send(self.index)
                            .expect("Could not notify executor");
                        has_error = true;
                        break;
                    }
                }
                _ => {
                    debug!(
                        "Neighbor {} isn't completed for {}, skipping",
                        neighbor, self.index
                    );
                    return;
                }
            };
        }

        if has_error {
            cur_steps[self.index] = Status::Completed(Outcome {
                output: Some("".into()),
                error: Some("Dependency Not Met".into()),
                duration: Duration::from_secs(0),
            });
            return;
        }

        if cur_steps[self.index] == Status::Outstanding {
            cur_steps[self.index] = Status::InProgress;

            let run = self.run.clone();

            let expect = self.expect.clone();
            let retry = self.retry;
            let tx = self.notify.clone();
            let index = self.index;
            let steps = self.steps.clone();
            let filters = self.filters.clone();
            let name = self.name.to_string();

            //let task = task::current();
            tokio::spawn(async move {
                let outcome = run.execute(expect, filters, retry).await;
                debug!("Step `{}` done: {:?}", index, outcome);
                if let Some(ref output) = outcome.output {
                    STEP_OUTPUT.insert(name, output.clone());
                }

                steps.lock().await[index] = Status::Completed(outcome);
                tx.send(index).expect("Could not notify executor");
            });
        }
    }
}

pub async fn run_steps(steps: &mut Vec<Step>) -> Result<(), Error> {
    let graph = create_graph(&steps)?;

    let steps_status: Arc<Mutex<Vec<Status>>> =
        Arc::new(Mutex::new(vec![Status::Outstanding; steps.len()]));

    //We want the runners to drop after this so we can return the steps status
    {
        let lookup: CHashMap<&str, usize> = CHashMap::new();

        for i in 0..steps.len() {
            lookup.insert(&steps[i].name, i);
        }

        let shared_graph = Arc::new(graph);

        let mut runners = Vec::new();

        let (tx, mut rx) = unbounded_channel();

        for i in 0..steps.len() {
            let future = StepRunner {
                run: steps[i].run.clone(),
                expect: steps[i].expect.clone(),
                retry: steps[i].retry,
                filters: steps[i].filters.clone(),
                name: &steps[i].name,
                graph: shared_graph.clone(),
                steps: steps_status.clone(),
                index: i,
                name_lookup: &lookup,
                notify: tx.clone(),
            };

            runners.push(future);
        }

        //Kick off the process
        for runner in runners.iter_mut() {
            runner.poll().await;
        }

        for _ in 0..steps.len() {
            if let Some(finished) = rx.recv().await {
                for neighbor in shared_graph.neighbors_directed(finished, Direction::Outgoing) {
                    runners[neighbor].poll().await;
                }
            }
        }
    }

    let steps_ptr =
        Arc::try_unwrap(steps_status).map_err(|_| anyhow!("Could not unwrap arc pointer"))?;

    for (i, status) in steps_ptr.into_inner().into_iter().enumerate() {
        if let Status::Completed(outcome) = status {
            steps[i].outcome = Some(outcome);
        }
    }

    Ok(())
}
