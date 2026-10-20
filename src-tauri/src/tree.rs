use crate::db::AppError;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

/// Minimal account metadata for ancestor-path walks.
pub struct AccountMeta {
    pub name: String,
    pub parent_guid: Option<String>,
    pub account_type: String,
}

/// Load every account's path metadata in one query so ancestor walks are
/// in-memory instead of one query per level.
pub fn load_account_metas(conn: &Connection) -> Result<HashMap<String, AccountMeta>, AppError> {
    let mut stmt = conn.prepare("SELECT guid, name, parent_guid, account_type FROM accounts")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                AccountMeta {
                    name: row.get(1)?,
                    parent_guid: row.get(2)?,
                    account_type: row.get(3)?,
                },
            ))
        })?
        .collect::<Result<HashMap<_, _>, _>>()?;
    Ok(rows)
}

/// `separator`-joined account path, skipping the internal ROOT ancestor. The
/// visited set guards against `parent_guid` cycles in corrupt books.
pub fn full_account_path(
    guid: &str,
    metas: &HashMap<String, AccountMeta>,
    separator: &str,
) -> String {
    let mut parts: Vec<&str> = Vec::new();
    let mut visited: HashSet<&str> = HashSet::new();
    let mut current = Some(guid);
    while let Some(g) = current {
        if !visited.insert(g) {
            break;
        }
        let Some(meta) = metas.get(g) else {
            break;
        };
        if meta.account_type != "ROOT" {
            parts.push(meta.name.as_str());
        }
        current = meta.parent_guid.as_deref();
        // Cap the walk so a long corrupt chain can't blow up the caller.
        if visited.len() > 100 {
            break;
        }
    }
    parts.reverse();
    parts.join(separator)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn index(rows: Vec<(&str, Option<&str>, &str)>) -> TreeIndex {
        build_index(
            rows.into_iter()
                .map(|(g, p, n)| (g.to_string(), p.map(str::to_string), n.to_string())),
        )
    }

    #[test]
    fn builds_name_sorted_adjacency() {
        let idx = index(vec![
            ("root", None, "Root"),
            ("b", Some("root"), "Beta"),
            ("a", Some("root"), "Alpha"),
            ("c", Some("b"), "Gamma"),
        ]);
        assert_eq!(idx.roots, vec!["root".to_string()]);
        assert_eq!(idx.children["root"], vec!["a".to_string(), "b".to_string()]);
        assert_eq!(idx.children["b"], vec!["c".to_string()]);
    }

    #[test]
    fn orphaned_parents_become_roots() {
        let idx = index(vec![("x", Some("missing"), "X"), ("y", None, "Y")]);
        // `missing` is not in the included set, so `x` is promoted to a root.
        assert_eq!(idx.roots, vec!["x".to_string(), "y".to_string()]);
    }

    #[test]
    fn parent_cycles_do_not_hang_or_lose_nodes() {
        // Corrupt book: a <-> b cycle. Both stay reachable via `children`
        // (they reference each other); neither silently disappears.
        let idx = index(vec![("a", Some("b"), "A"), ("b", Some("a"), "B")]);
        assert!(idx.children.contains_key("a"));
        assert!(idx.children.contains_key("b"));
        assert_eq!(idx.roots.len(), 0);
    }
}
