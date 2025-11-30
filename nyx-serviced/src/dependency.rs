//! Dependency resolution for service ordering

use crate::unit::Unit;
use anyhow::{Result, anyhow};
use std::collections::{HashMap, HashSet, VecDeque};

/// Resolve service startup order based on dependencies
pub fn resolve_order<'a>(units: &[&'a Unit]) -> Result<Vec<&'a Unit>> {
    let mut graph = DependencyGraph::new();

    for unit in units {
        graph.add_unit(unit);
    }

    graph.topological_sort()
}

/// Check if starting a service would satisfy its dependencies
pub fn check_dependencies(
    unit: &Unit,
    running: &HashSet<String>,
    available: &HashSet<String>,
) -> DependencyCheck {
    let mut missing = Vec::new();
    let mut not_running = Vec::new();

    // Check requires
    for req in &unit.install.requires {
        if !available.contains(req) {
            missing.push(req.clone());
        } else if !running.contains(req) {
            not_running.push(req.clone());
        }
    }

    // Check after (must be running or not required)
    for after in &unit.install.after {
        if available.contains(after) && !running.contains(after) {
            not_running.push(after.clone());
        }
    }

    if !missing.is_empty() {
        DependencyCheck::Missing(missing)
    } else if !not_running.is_empty() {
        DependencyCheck::NotRunning(not_running)
    } else {
        DependencyCheck::Satisfied
    }
}

/// Result of dependency check
#[derive(Debug, Clone)]
pub enum DependencyCheck {
    /// All dependencies satisfied
    Satisfied,
    /// Some required units don't exist
    Missing(Vec<String>),
    /// Some required units exist but aren't running
    NotRunning(Vec<String>),
}

impl DependencyCheck {
    pub fn is_satisfied(&self) -> bool {
        matches!(self, DependencyCheck::Satisfied)
    }

    pub fn can_wait(&self) -> bool {
        matches!(self, DependencyCheck::NotRunning(_))
    }

    pub fn blocking_units(&self) -> Vec<&str> {
        match self {
            DependencyCheck::Missing(units) => units.iter().map(|s| s.as_str()).collect(),
            DependencyCheck::NotRunning(units) => units.iter().map(|s| s.as_str()).collect(),
            DependencyCheck::Satisfied => vec![],
        }
    }
}

/// Graph for topological sorting
struct DependencyGraph<'a> {
    units: HashMap<&'a str, &'a Unit>,
    edges: HashMap<&'a str, Vec<&'a str>>,  // unit -> units that depend on it
    in_degree: HashMap<&'a str, usize>,
}

impl<'a> DependencyGraph<'a> {
    fn new() -> Self {
        Self {
            units: HashMap::new(),
            edges: HashMap::new(),
            in_degree: HashMap::new(),
        }
    }

    fn add_unit(&mut self, unit: &'a Unit) {
        self.units.insert(&unit.name, unit);
        self.edges.entry(&unit.name).or_default();
        self.in_degree.entry(&unit.name).or_insert(0);

        // Process "after" dependencies (this unit starts after those)
        for after in &unit.install.after {
            if let Some(after_name) = self.units.keys().find(|&&n| n == after.as_str()) {
                self.edges.entry(*after_name).or_default().push(&unit.name);
                *self.in_degree.entry(&unit.name).or_insert(0) += 1;
            }
        }

        // Process "before" dependencies (this unit starts before those)
        for before in &unit.install.before {
            if let Some(_) = self.units.get(before.as_str()) {
                self.edges.entry(&unit.name).or_default().push(before.as_str());
                *self.in_degree.entry(before.as_str()).or_insert(0) += 1;
            }
        }

        // Process "requires" (implies after)
        for req in &unit.install.requires {
            if let Some(req_name) = self.units.keys().find(|&&n| n == req.as_str()) {
                self.edges.entry(*req_name).or_default().push(&unit.name);
                *self.in_degree.entry(&unit.name).or_insert(0) += 1;
            }
        }
    }

    fn topological_sort(&self) -> Result<Vec<&'a Unit>> {
        let mut result = Vec::new();
        let mut queue = VecDeque::new();
        let mut in_degree = self.in_degree.clone();

        // Find all nodes with no incoming edges
        for (name, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(*name);
            }
        }

        while let Some(name) = queue.pop_front() {
            if let Some(unit) = self.units.get(name) {
                result.push(*unit);
            }

            // Reduce in-degree of dependents
            if let Some(dependents) = self.edges.get(name) {
                for &dep in dependents {
                    if let Some(degree) = in_degree.get_mut(dep) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(dep);
                        }
                    }
                }
            }
        }

        // Check for cycles
        if result.len() != self.units.len() {
            let remaining: Vec<_> = self.units.keys()
                .filter(|n| in_degree.get(*n).copied().unwrap_or(0) > 0)
                .collect();
            return Err(anyhow!(
                "Circular dependency detected involving: {:?}",
                remaining
            ));
        }

        Ok(result)
    }
}

/// Get units that should stop when a given unit stops
pub fn get_reverse_dependencies<'a>(
    unit_name: &str,
    units: &'a HashMap<String, Unit>,
) -> Vec<&'a str> {
    let mut reverse_deps = Vec::new();

    for (name, unit) in units {
        // If unit A requires B, and B stops, A should stop
        if unit.install.requires.iter().any(|r| r == unit_name) {
            reverse_deps.push(name.as_str());
        }
    }

    reverse_deps
}

/// Get units that should start before a given unit
pub fn get_start_before<'a>(
    unit_name: &str,
    units: &'a HashMap<String, Unit>,
) -> Vec<&'a str> {
    let mut start_before = Vec::new();

    // Get the target unit
    if let Some(unit) = units.get(unit_name) {
        // All units in "after" should start before this one
        for after in &unit.install.after {
            if units.contains_key(after) {
                start_before.push(after.as_str());
            }
        }

        // All units in "requires" should start before this one
        for req in &unit.install.requires {
            if units.contains_key(req) && !start_before.contains(&req.as_str()) {
                start_before.push(req.as_str());
            }
        }
    }

    start_before
}

/// Transaction for atomic multi-service operations
#[derive(Debug, Clone)]
pub struct ServiceTransaction {
    /// Services to start
    pub to_start: Vec<String>,
    /// Services to stop
    pub to_stop: Vec<String>,
    /// Services to restart
    pub to_restart: Vec<String>,
    /// Order of operations
    pub order: Vec<TransactionOp>,
}

#[derive(Debug, Clone)]
pub enum TransactionOp {
    Start(String),
    Stop(String),
    Restart(String),
}

impl ServiceTransaction {
    pub fn new() -> Self {
        Self {
            to_start: Vec::new(),
            to_stop: Vec::new(),
            to_restart: Vec::new(),
            order: Vec::new(),
        }
    }

    /// Plan starting a service and its dependencies
    pub fn plan_start(
        &mut self,
        unit_name: &str,
        units: &HashMap<String, Unit>,
        running: &HashSet<String>,
    ) {
        // Get dependencies that need to start first
        let deps = get_start_before(unit_name, units);

        for dep in deps {
            if !running.contains(dep) && !self.to_start.contains(&dep.to_string()) {
                self.to_start.push(dep.to_string());
                self.order.push(TransactionOp::Start(dep.to_string()));
            }
        }

        // Add the target service
        if !running.contains(unit_name) && !self.to_start.contains(&unit_name.to_string()) {
            self.to_start.push(unit_name.to_string());
            self.order.push(TransactionOp::Start(unit_name.to_string()));
        }
    }

    /// Plan stopping a service and its dependents
    pub fn plan_stop(
        &mut self,
        unit_name: &str,
        units: &HashMap<String, Unit>,
        running: &HashSet<String>,
    ) {
        // Get services that depend on this one
        let reverse = get_reverse_dependencies(unit_name, units);

        // Stop dependents first
        for dep in reverse {
            if running.contains(dep) && !self.to_stop.contains(&dep.to_string()) {
                self.to_stop.push(dep.to_string());
                self.order.push(TransactionOp::Stop(dep.to_string()));
            }
        }

        // Stop the target service
        if running.contains(unit_name) && !self.to_stop.contains(&unit_name.to_string()) {
            self.to_stop.push(unit_name.to_string());
            self.order.push(TransactionOp::Stop(unit_name.to_string()));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

impl Default for ServiceTransaction {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_unit(name: &str, after: Vec<&str>, requires: Vec<&str>) -> Unit {
        Unit {
            name: name.to_string(),
            description: String::new(),
            documentation: vec![],
            service: Default::default(),
            install: crate::unit::InstallConfig {
                after: after.into_iter().map(String::from).collect(),
                requires: requires.into_iter().map(String::from).collect(),
                ..Default::default()
            },
            resources: Default::default(),
            socket: None,
        }
    }

    #[test]
    fn test_simple_dependency_order() {
        let network = make_unit("network", vec![], vec![]);
        let database = make_unit("database", vec!["network"], vec![]);
        let app = make_unit("app", vec!["database"], vec!["database"]);

        let units: Vec<&Unit> = vec![&app, &database, &network];
        let order = resolve_order(&units).unwrap();

        let names: Vec<&str> = order.iter().map(|u| u.name.as_str()).collect();
        assert_eq!(names, vec!["network", "database", "app"]);
    }

    #[test]
    fn test_circular_dependency_detection() {
        let a = make_unit("a", vec!["b"], vec![]);
        let b = make_unit("b", vec!["a"], vec![]);

        let units: Vec<&Unit> = vec![&a, &b];
        let result = resolve_order(&units);

        assert!(result.is_err());
    }
}
