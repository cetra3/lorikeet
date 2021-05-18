use crate::step::FilterType;

use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use crate::step::{ExpectType, Outcome, RetryPolicy, RunType, Step, STEP_OUTPUT};

use crate::graph::{create_graph, Require};
use petgraph::prelude::GraphMap;
use petgraph::{Directed, Direction};

use log::*;

use anyhow::Error;

pub struct StepRunner {
    pub name: String,
    pub index: usize,
    pub run: RunType,
    pub expect: ExpectType,
    pub retry: RetryPolicy,
    pub filters: Vec<FilterType>,
    pub notify: UnboundedSender<(usize, Outcome)>,
}

//Spawns into a background task so we can poll the rest
impl StepRunner {
    pub async fn poll(self) {
        debug!("Running: {}", self.name);

        tokio::spawn(async move {
            let outcome = self
                .run
                .execute(self.expect, self.filters, self.retry)
                .await;

            if let Some(ref output) = outcome.output {
                STEP_OUTPUT.insert(self.name.clone(), output.clone());
            }

            if let Err(err) = self.notify.send((self.index, outcome)) {
                error!("Could not notify executor:{}", err);
            }

            debug!("Completed: {}", self.name);
        });
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Status {
    Awaiting,
    Completed(Outcome),
}

pub async fn run_steps(steps: &mut Vec<Step>) -> Result<(), Error> {
    let graph = create_graph(&steps)?;

    let mut statuses = Vec::new();
    statuses.resize(steps.len(), Status::Awaiting);

    //We want the runners to drop after this so we can return the steps status
    {
        let mut runners = Vec::new();

        let (tx, mut rx) = unbounded_channel();

        for (i, step) in steps.iter().enumerate() {
            let future = StepRunner {
                run: step.run.clone(),
                expect: step.expect.clone(),
                retry: step.retry,
                filters: step.filters.clone(),
                name: step.name.clone(),
                index: i,
                notify: tx.clone(),
            };

            runners.push(future);
        }

        //We want to start all the ones that don't have any outgoing neighbors
        let (to_start, waiting) = runners
            .into_iter()
            .partition::<Vec<StepRunner>, _>(|job| can_start(job.index, &statuses, &graph));

        runners = waiting;

        let mut active = 0;

        for runner in to_start.into_iter() {
            runner.poll().await;
            active += 1;
        }

        while active > 0 {
            debug!(
                "Active amount: {}, runners waiting: {}",
                active,
                runners.len()
            );
            if let Some((idx, outcome)) = rx.recv().await {
                active -= 1;
                let has_error = outcome.error.is_some();

                statuses[idx] = Status::Completed(outcome);

                for neighbor in graph.neighbors_directed(idx, Direction::Outgoing) {
                    if let Some(job_idx) = runners.iter().position(|job| job.index == neighbor) {
                        if !has_error && can_start(runners[job_idx].index, &statuses, &graph) {
                            let runner = runners.swap_remove(job_idx);
                            runner.poll().await;
                            active += 1;
                        }
                    }
                }
            }
        }
    }

    for (i, status) in statuses.into_iter().enumerate() {
        if let Status::Completed(outcome) = status {
            steps[i].outcome = Some(outcome);
        } else {
            steps[i].outcome = Some(Outcome {
                output: Some("".into()),
                error: Some("Dependency Not Met".into()),
                duration: Duration::from_secs(0),
            });
        }
    }

    Ok(())
}

fn can_start(idx: usize, statuses: &[Status], graph: &GraphMap<usize, Require, Directed>) -> bool {
    debug!("Checking if we can start for {}", idx);

    for neighbor in graph.neighbors_directed(idx, Direction::Incoming) {
        match statuses[neighbor] {
            Status::Awaiting => {
                debug!("Neighbour {} Not Completed", neighbor);
                return false;
            }
            Status::Completed(ref outcome) => {
                if outcome.error.is_some() {
                    debug!("Neighbour {} Has Error", neighbor);
                    return false;
                } else {
                    debug!("Neighbour {} Completed", neighbor);
                }
            }
        }
    }

    true
}
