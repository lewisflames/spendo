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

/// Incrementally builds a WHERE clause with explicit `?N` placeholders so
/// fragments can be combined with positional params that already exist in the
/// query (`param_offset` reserves those leading indices).
///
/// Each [`WhereBuilder::add`] call supplies a condition containing one `{}` per
/// bound value; every occurrence is replaced with the next sequential
/// placeholder index.
pub struct WhereBuilder {
    next_idx: usize,
    pub clauses: Vec<String>,
    pub params: Vec<Box<dyn ToSql>>,
}

impl WhereBuilder {
    pub fn new(param_offset: usize) -> Self {
        Self {
            next_idx: param_offset + 1,
            clauses: vec![],
            params: vec![],
        }
    }

    /// Add a condition without parameters (e.g. a constant filter fragment).
    pub fn push_raw(&mut self, clause: impl Into<String>) {
        self.clauses.push(clause.into());
    }

    /// Add a condition; each `{}` in `cond` binds one value from `params` and
    /// is replaced by its placeholder index.
    ///
    /// # Panics
    /// If `cond` contains fewer/more `{}` markers than `params` — a
    /// programming error caught at construction time rather than as broken SQL.
    pub fn add(&mut self, cond: &str, params: Vec<Box<dyn ToSql>>) {
        let marker_count = cond.matches("{}").count();
        assert_eq!(
            marker_count,
            params.len(),
            "condition has {} placeholders but {} params were given",
            marker_count,
            params.len()
        );
        let mut sql = String::with_capacity(cond.len());
        let mut rest = cond;
        for _ in &params {
            let (head, tail) = rest.split_once("{}").expect("marker count verified above");
            sql.push_str(head);
            sql.push_str(&format!("?{}", self.next_idx));
            self.next_idx += 1;
            rest = tail;
        }
        sql.push_str(rest);
        self.clauses.push(sql);
        self.params.extend(params);
    }

    /// `" AND "`-joined clause list (empty when nothing was added).
    pub fn joined(&self) -> String {
        self.clauses.join(" AND ")
    }
}
