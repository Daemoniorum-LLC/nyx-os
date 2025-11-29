//! Dependency graph for service ordering

use crate::service::ServiceSpec;
use anyhow::{bail, Result};
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;
use tracing::debug;

/// Dependency graph for services
pub struct DependencyGraph {
    /// The graph structure
    graph: DiGraph<String, ()>,
    /// Map from service name to node index
    nodes: HashMap<String, NodeIndex>,
}

impl DependencyGraph {
    /// Create a new empty dependency graph
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            nodes: HashMap::new(),
        }
    }

    /// Add a service to the graph
    pub fn add_service(&mut self, name: &str, dependencies: &[String]) {
        // Get or create node for this service
        let node = *self.nodes.entry(name.to_string()).or_insert_with(|| {
            self.graph.add_node(name.to_string())
        });

        // Add edges for dependencies
        for dep in dependencies {
            let dep_node = *self.nodes.entry(dep.clone()).or_insert_with(|| {
                self.graph.add_node(dep.clone())
            });

            // Edge goes from dependency to dependent (dep -> node)
            // This means "dep must start before node"
            self.graph.add_edge(dep_node, node, ());
        }
    }

    /// Get topological order (services in order they should start)
    pub fn topological_order(&self) -> Result<Vec<String>> {
        match toposort(&self.graph, None) {
            Ok(indices) => {
                Ok(indices
                    .into_iter()
                    .map(|idx| self.graph[idx].clone())
                    .collect())
            }
            Err(cycle) => {
                let node = &self.graph[cycle.node_id()];
                bail!("Dependency cycle detected involving: {}", node)
            }
        }
    }

    /// Validate the graph (check for cycles)
    pub fn validate(&self) -> Result<()> {
        self.topological_order()?;
        Ok(())
    }

    /// Get direct dependencies of a service
    pub fn dependencies(&self, name: &str) -> Vec<String> {
        if let Some(&node) = self.nodes.get(name) {
            self.graph
                .neighbors_directed(node, petgraph::Direction::Incoming)
                .map(|idx| self.graph[idx].clone())
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get services that depend on this one
    pub fn dependents(&self, name: &str) -> Vec<String> {
        if let Some(&node) = self.nodes.get(name) {
            self.graph
                .neighbors_directed(node, petgraph::Direction::Outgoing)
                .map(|idx| self.graph[idx].clone())
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get number of services in the graph
    pub fn service_count(&self) -> usize {
        self.nodes.len()
    }

    /// Check if a service exists in the graph
    pub fn contains(&self, name: &str) -> bool {
        self.nodes.contains_key(name)
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Build dependency graph from service specs
pub fn build_graph(services: &[ServiceSpec]) -> Result<DependencyGraph> {
    let mut graph = DependencyGraph::new();

    // First pass: add all services
    for service in services {
        graph.add_service(&service.name, &service.dependencies);
    }

    // Validate
    graph.validate()?;

    debug!(
        "Built dependency graph with {} services",
        graph.service_count()
    );

    Ok(graph)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_dependency() {
        let mut graph = DependencyGraph::new();
        graph.add_service("b", &["a".into()]);
        graph.add_service("a", &[]);

        let order = graph.topological_order().unwrap();
        assert_eq!(order, vec!["a", "b"]);
    }

    #[test]
    fn test_complex_dependencies() {
        let mut graph = DependencyGraph::new();
        graph.add_service("d", &["b".into(), "c".into()]);
        graph.add_service("b", &["a".into()]);
        graph.add_service("c", &["a".into()]);
        graph.add_service("a", &[]);

        let order = graph.topological_order().unwrap();

        // a must come before b, c, d
        // b and c must come before d
        let a_pos = order.iter().position(|x| x == "a").unwrap();
        let b_pos = order.iter().position(|x| x == "b").unwrap();
        let c_pos = order.iter().position(|x| x == "c").unwrap();
        let d_pos = order.iter().position(|x| x == "d").unwrap();

        assert!(a_pos < b_pos);
        assert!(a_pos < c_pos);
        assert!(b_pos < d_pos);
        assert!(c_pos < d_pos);
    }

    #[test]
    fn test_cycle_detection() {
        let mut graph = DependencyGraph::new();
        graph.add_service("a", &["b".into()]);
        graph.add_service("b", &["c".into()]);
        graph.add_service("c", &["a".into()]);

        assert!(graph.topological_order().is_err());
    }
}
