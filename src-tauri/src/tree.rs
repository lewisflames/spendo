use std::collections::{HashMap, HashSet};

/// Parent→children adjacency plus root GUIDs for a set of tree nodes.
/// All lists are sorted by node name so traversal order is stable.
pub struct TreeIndex {
    pub children: HashMap<String, Vec<String>>,
    pub roots: Vec<String>,
}

/// Build adjacency and roots from `(guid, parent_guid, name)` triples.
///
/// A node becomes a child only when its parent is itself among the included
/// guids; otherwise (no parent, corrupt or filtered-out parent) it is a root.
pub fn build_index<I>(rows: I) -> TreeIndex
where
    I: IntoIterator<Item = (String, Option<String>, String)>,
{
    let mut names: HashMap<String, String> = HashMap::new();
    let mut included: HashSet<String> = HashSet::new();
    let mut edges: Vec<(String, String)> = Vec::new();
    let mut roots: Vec<String> = Vec::new();

    for (guid, parent, name) in rows {
        names.insert(guid.clone(), name);
        included.insert(guid.clone());
        match parent {
            Some(p) => edges.push((p, guid)),
            None => roots.push(guid),
        }
    }

    let mut children: HashMap<String, Vec<String>> =
        included.iter().map(|g| (g.clone(), Vec::new())).collect();
    for (parent, child) in edges {
        match children.get_mut(&parent) {
            Some(kids) => kids.push(child),
            None => roots.push(child),
        }
    }

    let by_name = |a: &String, b: &String| names[a].cmp(&names[b]);
    roots.sort_by(by_name);
    for kids in children.values_mut() {
        kids.sort_by(by_name);
    }

    TreeIndex { children, roots }
}
