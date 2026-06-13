// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::Hash,
};

use daggy::{
    NodeIndex,
    petgraph::{
        Direction,
        visit::{Dfs, Visitable},
    },
};

#[derive(thiserror::Error, Debug, Clone)]
pub enum DagError {
    #[error("Node already exists in DAG")]
    NodeAlreadyExists,

    #[error("Item is not defined in dependency structure")]
    MissingTInDependencies,

    #[error("Dependent T is not defined")]
    MissingDependentT,

    #[error("Missing T in T->idx lookup")]
    MissingAddedT,

    #[error("Missing NodeIndex in internal Dag")]
    MissingNInGraph,

    #[error("Cycle deteced when building DAG")]
    CyclicGraph(#[from] daggy::WouldCycle<()>),

    #[error("T is missing in the graph")]
    MissingTInGraph,
}

/// General Dag operations for identifier T. T is stored, and cloned so it should
/// be a cheap identifier, not a heavy object (Copy-weight is ideal).
/// Exposes ActaTools-relevant functions for the underlying dag
#[derive(Debug)]
pub struct ActaDag<T>
where
    T: Hash + PartialEq + Eq + Clone,
{
    dag: daggy::Dag<T, ()>,
    by_node: HashMap<T, NodeIndex>,
}

impl<T: Hash + PartialEq + Eq + Clone> ActaDag<T> {
    /// From a set of nodes and dependencies, builds the DAG. Automatically checks for cycles as edges are added, returning an DagError:CyclicGraph
    /// if so.
    /// Only T nodes with dependencies need to be defined in dependencies
    pub fn build_from_nodes(
        nodes: impl IntoIterator<Item = T>,
        dependencies: HashMap<T, HashSet<T>>,
    ) -> Result<Self, DagError> {
        let mut dag: daggy::Dag<T, ()> = daggy::Dag::new();
        let mut by_node: HashMap<T, NodeIndex> = HashMap::new();

        // collect the nodes
        let nodes: Vec<T> = nodes.into_iter().collect();

        for n in nodes.clone() {
            let idx = dag.add_node(n.clone());
            by_node.insert(n.clone(), idx);
        }

        let empty_set: HashSet<T> = HashSet::new();

        // now add the edges, should go from the parent to the child
        for n in nodes {
            // assume there are no dependencies if not provided
            let n_parents = dependencies.get(&n).unwrap_or(&empty_set);

            for n_parent in n_parents {
                let n_parent_idx = by_node
                    .get(n_parent)
                    .ok_or_else(|| DagError::MissingDependentT)?;
                let n_child_idx = by_node.get(&n).ok_or_else(|| DagError::MissingAddedT)?;

                dag.add_edge(*n_parent_idx, *n_child_idx, ())?;
            }
        }

        Ok(Self { dag, by_node })
    }

    fn get_by_node(&self, node: &T) -> Result<NodeIndex, DagError> {
        let node_idx = self
            .by_node
            .get(node)
            .ok_or_else(|| DagError::MissingTInGraph)?;

        Ok(*node_idx)
    }

    /// Returns an iterator walking the direct parents of the node T
    pub fn parents(&self, node: &T) -> Result<impl Iterator<Item = &T> + '_, DagError> {
        let node_idx = self.get_by_node(&node)?;
        Ok(self
            .dag
            .graph()
            .neighbors_directed(node_idx, Direction::Incoming)
            .filter_map(|idx| self.dag.graph().node_weight(idx)))
    }

    /// collects all the ancestors of a node
    pub fn collect_ancestors(&self, name: &T) -> Result<HashSet<T>, DagError> {
        let node_index = self.get_by_node(name)?;
        let mut ancestors = HashSet::new();
        Self::collect_ancestors_inner(&self.dag, node_index, &mut ancestors);

        let ancestor_names = ancestors
            .iter()
            .map(|n| self.dag.graph().node_weight(*n))
            .collect::<Option<HashSet<&T>>>();
        let ancestor_names: HashSet<T> = ancestor_names
            .ok_or_else(|| DagError::MissingTInGraph)?
            .into_iter()
            .cloned()
            .collect();

        Ok(ancestor_names)
    }

    fn collect_ancestors_inner<N, E>(
        dag: &daggy::Dag<N, E>,
        node: NodeIndex,
        ancestors: &mut HashSet<NodeIndex>,
    ) {
        for parent in dag.graph().neighbors_directed(node, Direction::Incoming) {
            if ancestors.insert(parent) {
                Self::collect_ancestors_inner(dag, parent, ancestors);
            }
        }
    }

    /// Returns an iterator walking the chilren of node T
    pub fn children(&self, node: &T) -> Result<impl Iterator<Item = &T> + '_, DagError> {
        let node_idx = self.get_by_node(&node)?;
        Ok(self
            .dag
            .graph()
            .neighbors_directed(node_idx, Direction::Outgoing)
            .filter_map(|idx| self.dag.graph().node_weight(idx)))
    }

    /// Returns all the nodes in Depth First order, in the order of the starting nodes.
    /// For the purposes of ActaStudy, this naturally will put the steps in the order if the starting nodes are generally
    /// in the order of the "runs"
    pub fn get_all_nodes_dfs<'a>(&self, starting_nodes: &Vec<T>) -> Result<Vec<T>, DagError> {
        let visit_map = self.dag.visit_map();

        let node_stack: Vec<_> = starting_nodes
            .iter()
            .rev()
            .map(|t| self.get_by_node(t))
            .collect::<Result<Vec<_>, DagError>>()?;

        let mut dfs = Dfs::from_parts(node_stack, visit_map);

        let mut dfs_nodes: Vec<T> = Vec::with_capacity(self.dag.node_count());
        while let Some(nx) = dfs.next(&self.dag) {
            dfs_nodes.push(
                self.dag
                    .graph()
                    .node_weight(nx)
                    .ok_or_else(|| DagError::MissingNInGraph)?
                    .clone(),
            );
        }

        Ok(dfs_nodes)
    }
}

pub struct DagBuilder<T>
where
    T: Hash + Eq + Clone,
{
    nodes: HashSet<T>,
    dependencies: HashMap<T, HashSet<T>>,
}

impl<T: Hash + Eq + Clone + Debug> DagBuilder<T> {
    pub fn new() -> Self {
        Self {
            nodes: HashSet::new(),
            dependencies: HashMap::new(),
        }
    }

    /// Adds a node T with its dependencies
    pub fn add_node<I>(&mut self, node: T, dependencies: I) -> Result<(), DagError>
    where
        I: IntoIterator<Item = T>,
    {
        // add the node
        match self.nodes.insert(node.clone()) {
            true => {}
            false => return Err(DagError::NodeAlreadyExists),
        }

        // add the dependencies
        for t in dependencies {
            self.dependencies.entry(node.clone()).or_default().insert(t);
        }

        Ok(())
    }

    /// Consumes the builder to create a realized Dag
    pub fn into_dag(self) -> Result<ActaDag<T>, DagError> {
        ActaDag::build_from_nodes(self.nodes, self.dependencies)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> HashSet<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    fn sorted_strings<'a>(items: impl Iterator<Item = &'a String>) -> Vec<String> {
        let mut values: Vec<String> = items.cloned().collect();
        values.sort();
        values
    }

    #[test]
    fn builder_creates_dag_with_parent_and_child_edges() {
        let mut builder = DagBuilder::new();

        builder.add_node("preprocess".to_string(), []).unwrap();
        builder
            .add_node("solve".to_string(), ["preprocess".to_string()])
            .unwrap();
        builder
            .add_node("postprocess".to_string(), ["solve".to_string()])
            .unwrap();

        let dag = builder.into_dag().unwrap();

        let solve_parents = sorted_strings(dag.parents(&"solve".to_string()).unwrap());
        assert_eq!(solve_parents, vec!["preprocess"]);

        let solve_children = sorted_strings(dag.children(&"solve".to_string()).unwrap());
        assert_eq!(solve_children, vec!["postprocess"]);
    }

    #[test]
    fn builder_supports_multiple_parents() {
        let mut builder = DagBuilder::new();

        builder.add_node("mesh".to_string(), []).unwrap();
        builder.add_node("preprocess".to_string(), []).unwrap();
        builder
            .add_node(
                "solve".to_string(),
                ["mesh".to_string(), "preprocess".to_string()],
            )
            .unwrap();

        let dag = builder.into_dag().unwrap();

        let parents = sorted_strings(dag.parents(&"solve".to_string()).unwrap());
        assert_eq!(parents, vec!["mesh", "preprocess"]);
    }

    #[test]
    fn builder_supports_multiple_children() {
        let mut builder = DagBuilder::new();

        builder.add_node("prepare".to_string(), []).unwrap();
        builder
            .add_node("solve_a".to_string(), ["prepare".to_string()])
            .unwrap();
        builder
            .add_node("solve_b".to_string(), ["prepare".to_string()])
            .unwrap();

        let dag = builder.into_dag().unwrap();

        let children = sorted_strings(dag.children(&"prepare".to_string()).unwrap());
        assert_eq!(children, vec!["solve_a", "solve_b"]);
    }

    #[test]
    fn builder_rejects_duplicate_nodes() {
        let mut builder = DagBuilder::new();

        builder.add_node("prepare".to_string(), []).unwrap();

        let err = builder.add_node("prepare".to_string(), []).unwrap_err();

        assert!(matches!(err, DagError::NodeAlreadyExists));
    }

    #[test]
    fn into_dag_errors_when_dependency_node_is_missing() {
        let mut builder = DagBuilder::new();

        builder
            .add_node("solve".to_string(), ["prepare".to_string()])
            .unwrap();

        let err = builder.into_dag().unwrap_err();

        assert!(matches!(err, DagError::MissingDependentT));
    }

    #[test]
    fn build_from_nodes_errors_on_cycle() {
        let nodes = vec!["a".to_string(), "b".to_string()];

        let mut dependencies = HashMap::new();
        dependencies.insert("a".to_string(), set(&["b"]));
        dependencies.insert("b".to_string(), set(&["a"]));

        let err = ActaDag::build_from_nodes(nodes, dependencies).unwrap_err();

        assert!(matches!(err, DagError::CyclicGraph(_)));
    }

    #[test]
    fn parents_errors_when_node_missing_from_graph() {
        let mut builder = DagBuilder::new();

        builder.add_node("a".to_string(), []).unwrap();

        let dag = builder.into_dag().unwrap();

        let res = dag.parents(&"missing".to_string());

        match res {
            Ok(_) => assert!(false),
            Err(s) => match s {
                DagError::MissingTInGraph => assert!(true),
                _ => assert!(false),
            },
        }
    }

    #[test]
    fn children_errors_when_node_missing_from_graph() {
        let mut builder = DagBuilder::new();

        builder.add_node("a".to_string(), []).unwrap();

        let dag = builder.into_dag().unwrap();

        let res = dag.children(&"missing".to_string());

        match res {
            Ok(_) => assert!(false),
            Err(s) => match s {
                DagError::MissingTInGraph => assert!(true),
                _ => assert!(false),
            },
        }
    }

    #[test]
    fn root_node_has_no_parents() {
        let mut builder = DagBuilder::new();

        builder.add_node("root".to_string(), []).unwrap();
        builder
            .add_node("child".to_string(), ["root".to_string()])
            .unwrap();

        let dag = builder.into_dag().unwrap();

        let parents = sorted_strings(dag.parents(&"root".to_string()).unwrap());
        assert!(parents.is_empty());
    }

    #[test]
    fn leaf_node_has_no_children() {
        let mut builder = DagBuilder::new();

        builder.add_node("root".to_string(), []).unwrap();
        builder
            .add_node("child".to_string(), ["root".to_string()])
            .unwrap();

        let dag = builder.into_dag().unwrap();

        let children = sorted_strings(dag.children(&"child".to_string()).unwrap());
        assert!(children.is_empty());
    }

    #[test]
    fn build_from_nodes_allows_missing_dependency_entry_for_node() {
        let nodes = vec!["a".to_string(), "b".to_string()];

        let mut dependencies = HashMap::new();
        dependencies.insert("b".to_string(), set(&["a"]));

        let dag = ActaDag::build_from_nodes(nodes, dependencies).unwrap();

        let parents = sorted_strings(dag.parents(&"b".to_string()).unwrap());
        assert_eq!(parents, vec!["a"]);

        let a_parents = sorted_strings(dag.parents(&"a".to_string()).unwrap());
        assert!(a_parents.is_empty());
    }

    #[test]
    fn children_returns_only_direct_children_not_all_descendants() {
        let mut builder = DagBuilder::new();

        builder.add_node("root".to_string(), []).unwrap();
        builder
            .add_node("middle".to_string(), ["root".to_string()])
            .unwrap();
        builder
            .add_node("leaf".to_string(), ["middle".to_string()])
            .unwrap();

        let dag = builder.into_dag().unwrap();

        let children = sorted_strings(dag.children(&"root".to_string()).unwrap());

        assert_eq!(children, vec!["middle"]);
    }
}
