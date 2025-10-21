use crate::step::RunType;
use crate::step::Step;
use anyhow::{Error, anyhow};
use petgraph::prelude::GraphMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Require;

pub fn create_graph(steps: &[Step]) -> Result<GraphMap<usize, Require, petgraph::Directed>, Error> {
    let mut graph = GraphMap::<usize, Require, petgraph::Directed>::new();

    for i in 0..steps.len() {
        //Add a dependency for the step to run first if the run type is `step`
        if let RunType::Step(ref dep) = steps[i].run {
            let dep_index = steps.iter().position(|step| &step.name == dep).ok_or_else(|| anyhow!("Could not build step graph: `{}` can not be found. defined from step run type on `{}`", dep, steps[i].name))?;
            graph.add_edge(dep_index, i, Require);
        }

        for dep in steps[i].require.iter() {
            let dep_index = steps.iter().position(|step| &step.name == dep).ok_or_else(|| anyhow!("Could not build step graph: `{}` can not be found. defined from `require` on `{}`", dep, steps[i].name))?;
            graph.add_edge(dep_index, i, Require);
        }

        for dep in steps[i].required_by.iter() {
            let dep_index = steps.iter().position(|step| &step.name == dep).ok_or_else(|| anyhow!("Could not build step graph: `{}` can not be found. defined from `required_by` on `{}`", dep, steps[i].name))?;

            graph.add_edge(i, dep_index, Require);
        }
    }

    match petgraph::algo::toposort(&graph, None) {
        Ok(_) => Ok(graph),
        Err(err) => Err(anyhow!(
            "Could not build step graph: `{}` has a circular dependency",
            steps[err.node_id()].name
        )),
    }
}
