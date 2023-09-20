use anyhow::Result;
use std::collections::HashMap;
use oxigraph::io::GraphFormat;
use rdf_class_viz::Visualizer;
use std::env;
use std::fs::File;
use std::io::BufReader;

pub fn main() -> Result<()> {
    // Get command-line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "Usage: {} <ontology_file1> <ontology_file2> ... <graph_filename>",
            args[0]
        );
        std::process::exit(1);
    }

    // Extract the graph filename
    let graph_filename = args.last().unwrap();

    let filter = |from: &str, to: &str, edge: &str| {
        true
    };

    let color_map_defn = vec![
        ("https://brickschema.org/schema/Brick#Location", "LightCoral"),
        ("https://brickschema.org/schema/Brick#Point", "Gold"),
        ("https://brickschema.org/schema/Brick#Equipment", "#32BF84"),
    ];
    let color_map: HashMap<&str, &str> = color_map_defn.into_iter().collect();

    // Create a Visualizer
    let mut v = Visualizer::new(filter, color_map)?;

    // Process ontology files
    for ontology_file in &args[1..args.len() - 1] {
        let f = File::open(ontology_file)?;
        let f = BufReader::new(f);
        v.add_ontology(f, GraphFormat::Turtle)?;
    }

    // Process the graph file
    let f = File::open(graph_filename)?;
    let f = BufReader::new(f);
    println!("{}", v.create_graph(f, GraphFormat::Turtle)?);

    Ok(())
}
