use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use oxigraph::io::GraphFormat;
use oxigraph::model::*;
use oxigraph::sparql::QueryResults;
use oxigraph::store::Store;
use petgraph::dot::Dot;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::Graph;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufRead;
use std::io::Write;

static PREFIXES: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert("brick", "https://brickschema.org/schema/Brick#");
    map.insert("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#");
    map.insert("owl", "http://www.w3.org/2002/07/owl#");
    map
});

fn rewrite_term(node: &Term) -> String {
    let mut s = node.to_string();
    for (prefix, namespace) in PREFIXES.iter() {
        s = s.replace(namespace, format!("{}_", prefix).as_str());
    }
    let matches: &[_] = &['<', '>', '"'];
    s.trim_matches(matches).to_owned()
}

fn graph_to_dot(graph: &petgraph::Graph<&str, &str>, filename: &str) -> Result<()> {
    let mut file = File::create(filename)?;
    write!(file, "{:?}", Dot::with_config(graph, &[]))?;
    Ok(())
}

type ColorFn = fn(node: &str) -> String;
type FilterFn = fn(from: &str, to: &str, edge: &str) -> bool;

pub struct Visualizer<'a> {
    store: Store,
    labels: Vec<String>,
    g: Graph<&'a str, &'a str>,
    nodes: HashMap<&'a str, NodeIndex>,
    filter: FilterFn,
    class_color_map: HashMap<&'a str, &'a str>,
    colors: HashMap<String, String>
}

impl<'a> Visualizer<'a> {
    pub fn new(filter: FilterFn, class_color_map: HashMap<&'a str, &'a str>) -> Result<Self> {
        Ok(Visualizer {
            store: Store::new()?,
            labels: Vec::new(),
            g: Graph::new(),
            nodes: HashMap::new(),
            colors: HashMap::new(),
            class_color_map,
            filter,
        })
    }

    pub fn add_ontology(&mut self, content: impl BufRead, format: GraphFormat) -> Result<()> {
        Ok(self.store.bulk_loader().load_graph(
            content,
            format,
            GraphNameRef::DefaultGraph,
            None,
        )?)
    }

    pub fn graph_to_d2lang(&self) -> Result<String> {
        let mut w = Vec::new();

        // Write edge labels
        for edge in self.g.edge_references() {
            let source = edge.source();
            let target = edge.target();
            let label = edge.weight();
            writeln!(
                w,
                "{} -> {}: {}",
                self.g.node_weight(source).unwrap(),
                self.g.node_weight(target).unwrap(),
                label
            )?;
        }

        // write colors
        for (node, color) in self.colors.iter() {
            writeln!(w, "{}.style.fill: \"{}\"", node, color)?;
        }

        Ok(String::from_utf8(w)?)
    }

    fn to_color(&'a self, node: &Term) -> Result<&'a str> {
        for (class_name, color) in self.class_color_map.iter() {
            let q = format!("PREFIX owl: <http://www.w3.org/2002/07/owl#>
                     PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
                     ASK {{
                        {0} (rdfs:subClassOf|owl:equivalentClass)* <{1}>
                     }}", node, class_name);
            if let QueryResults::Boolean(is_subclass) = self.store.query(&q)? {
                if is_subclass {
                    return Ok(color);
                }
            }

        }
        Ok("#ffffff")
    }

    pub fn create_graph(&'a mut self, data_graph: impl BufRead, format: GraphFormat) -> Result<String> {
        // load into a graph
        self.store.bulk_loader().load_graph(
            data_graph,
            format,
            GraphNameRef::DefaultGraph,
            None,
        )?;

        let q = "PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
                 PREFIX owl: <http://www.w3.org/2002/07/owl#>
                 SELECT ?from ?p ?to WHERE {
                     ?x rdf:type ?from .
                     ?x ?p ?y .
                     ?y rdf:type ?to .
                     ?from a owl:Class .
                     ?to a owl:Class .
                 }";

        if let QueryResults::Solutions(solutions) = self.store.query(q)? {
            let mut edges: Vec<(usize, usize, usize)> = Vec::new();
            for row in solutions {
                let row = row?;

                {
                    let from = row.get("from").unwrap().to_string();
                    let to = row.get("to").unwrap().to_string();
                    let p = row.get("p").unwrap().to_string();

                    if !(self.filter)(from.as_str(), to.as_str(), p.as_str()) {
                        continue;
                    }
                }
                let from_term = row.get("from").unwrap();
                let f = rewrite_term(&from_term);
                if !self.colors.contains_key(&f) {
                    self.colors.insert(f.clone(), self.to_color(&from_term).unwrap().to_owned());
                }
                self.labels.push(f);
                let f_idx = self.labels.len() - 1;

                let to_term = row.get("to").unwrap();
                let t = rewrite_term(&to_term);
                if !self.colors.contains_key(&t) {
                    self.colors.insert(t.clone(), self.to_color(&to_term).unwrap().to_owned());
                }
                self.labels.push(t);
                let t_idx = self.labels.len() - 1;

                let e = rewrite_term(row.get("p").unwrap());
                self.labels.push(e);
                let e_idx = self.labels.len() - 1;
                edges.push((f_idx, t_idx, e_idx));
            }

            // Now that we have collected all the data, update the graph outside the loop
            for (from, to, edge) in edges {
                let from: &'a str = self.labels[from].as_str();
                let from_idx = *self
                    .nodes
                    .entry(from)
                    .or_insert_with(|| self.g.add_node(from));

                let to: &'a str = self.labels[to].as_str();
                let to_idx = *self.nodes.entry(to).or_insert_with(|| self.g.add_node(to));

                self.g
                    .update_edge(from_idx, to_idx, self.labels[edge].as_str());
            }
        }

        graph_to_dot(&self.g, "output.dot")?;
        self.graph_to_d2lang()
    }
}
