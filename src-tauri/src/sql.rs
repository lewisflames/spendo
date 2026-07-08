use rusqlite::types::ToSql;

/// Generate `?1, ?2, ...` placeholders for an `IN (...)` clause.
pub fn in_clause(count: usize) -> String {
    (1..=count)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Convert owned boxed params to references for query execution.
pub fn param_refs(params: &[Box<dyn ToSql>]) -> Vec<&dyn ToSql> {
    params.iter().map(|p| p.as_ref()).collect()
}

/// Convert a slice of owned strings to trait-object refs (e.g. for GUID lists).
pub fn string_param_refs(guids: &[String]) -> Vec<&dyn ToSql> {
    guids.iter().map(|g| g as &dyn ToSql).collect()
}
