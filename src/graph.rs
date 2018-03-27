
use petgraph::prelude::GraphMap;
use petgraph;
use step::Step;
use step::RunType;


#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Require;

pub fn create_graph(steps: &Vec<Step>) -> GraphMap<usize, Require, petgraph::Directed> {
    let mut graph = GraphMap::<usize, Require, petgraph::Directed>::new();

    for i in 0..steps.len() {

        //Add a dependency for the step to run first if the run type is `step`
        if let RunType::Step(ref dep) = steps[i].run {
            let dep_index = steps.iter().position(|ref step| &step.name == dep).expect(&format!("Could not find step: {}! Dependency for: {}", dep, steps[i].name));
            graph.add_edge(dep_index, i,  Require);
        }

        for dep in steps[i].require.iter() {
            let dep_index = steps.iter().position(|ref step| &step.name == dep).expect(&format!("Could not find step: {}! Dependency for: {}", dep, steps[i].name));
            graph.add_edge(dep_index, i,  Require);
        }

        for dep in steps[i].required_by.iter() {
            let dep_index = steps.iter().position(|ref step| &step.name == dep).expect(&format!("Could not find step: {}!", dep));
            graph.add_edge(i, dep_index,  Require);
        }
    }

    petgraph::algo::toposort(&graph, None).expect("Encountered a Circular Dependency!");

    graph
}