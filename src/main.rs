#[macro_use]
extern crate serde_derive;

extern crate serde_yaml;

extern crate petgraph;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::prelude::*;
use std::thread;

use petgraph::visit::depth_first_search;
use petgraph::prelude::GraphMap;


#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct StepYaml {
    run: Option<String>,
    require: Option<Vec<String>>,
    required_by: Option<Vec<String>>
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Step {
    name: String,
    run: String,
    require: Vec<String>,
    required_by: Vec<String>
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Status {
    Outstanding,
    Completed,
    Error
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Require;

fn main() {

    let mut file = File::open("test.yaml").unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).expect("Could not read file to string");

    let steps: BTreeMap<String, StepYaml> = serde_yaml::from_str(&contents).unwrap();
    let mut graph = petgraph::graphmap::GraphMap::<&str, Require, petgraph::Directed>::new();

    for (name, stepyaml) in steps.iter() {

       graph.add_node(name);

       if let Some(ref deps) = stepyaml.require {
           for dep in deps {
                if steps.contains_key(dep) {
                    graph.add_edge(dep, name, Require);
                } else {
                    println!("Could not find dependency named '{}' in yaml file!", dep);
                }
           }
       }

       if let Some(ref deps) = stepyaml.required_by {
           for dep in deps {
                if steps.contains_key(dep) {
                    graph.add_edge(name, dep, Require);
                } else {
                    println!("Could not find dependency named '{}' in yaml file!", dep);
                }
           }
       }

       graph.add_edge("ROOT_NODE", name, Require);

    }

    let mut result: BTreeMap<String, Status> = BTreeMap::new();

    result.insert(String::from("ROOT_NODE"), Status::Completed);

    println!("{:?}\n", graph);

    //Checked for circular dependencies!
    petgraph::algo::toposort(&graph, None).expect("Encountered a Circular Dependency!");


    for neighbor in graph.neighbors_directed("ROOT_NODE", petgraph::Direction::Outgoing) {
        submit_node(&graph, neighbor, &mut result);
    }

}

fn submit_node(graph: &GraphMap<&str, Require, petgraph::Directed>, node: &str, mut result: &mut BTreeMap<String, Status>) {
        let mut can_submit = true;

        //Check for completed node
        if let Some(status) = result.get(node) {
            can_submit = match *status {
                Status::Outstanding => true,
                Status::Error => false,
                Status::Completed => false       
            };
        }

        //Check completed neighbors
        for neighbor in graph.neighbors_directed(node, petgraph::Direction::Incoming) {
            if let Some(status) = result.get(neighbor) {

                can_submit = match *status {
                    Status::Outstanding => false,
                    Status::Error => false,
                    Status::Completed => can_submit       
                };

            } else {
                can_submit = false;
            }
        }

        if can_submit {

            result.insert(String::from(node), Status::Completed);

            println!("Completed: {}", node);

            for neighbor in graph.neighbors_directed(node, petgraph::Direction::Outgoing) {
                submit_node(&graph, neighbor, &mut result);
            }

        }
}