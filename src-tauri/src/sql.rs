use crate::utils;
use rusqlite::types::ToSql;

/// SQLite caps bound parameters per statement; chunk `IN (...)` lists larger
/// than this so queries can never hit "too many SQL variables".
pub const MAX_IN_PARAMS: usize = 500;

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

/// Push the shared transaction date window and cross-field search predicate
/// onto `w`. Callers must alias `transactions` as `t`. Dates are half-open
/// (`>= from AND < date(to,'+1 day')`); search matches description, number,
/// notes, split memos, tag and payee names, with LIKE metacharacters escaped.
/// Single source of truth for the ledger and account-detail filters.
pub fn push_tx_date_search(
    w: &mut WhereBuilder,
    date_from: Option<&str>,
    date_to: Option<&str>,
    search: Option<&str>,
) {
    if let Some(df) = date_from {
        w.add("t.post_date >= {}", vec![Box::new(df.to_string())]);
    }
    if let Some(dt) = date_to {
        w.add(
            "t.post_date < date({}, '+1 day')",
            vec![Box::new(dt.to_string())],
        );
    }
    if let Some(s) = search {
        let s = utils::escape_like(s);
        w.add(
            "(t.description LIKE '%' || {} || '%' ESCAPE '\\' \
             OR t.num LIKE '%' || {} || '%' ESCAPE '\\' \
             OR t.notes LIKE '%' || {} || '%' ESCAPE '\\' \
             OR EXISTS(SELECT 1 FROM splits m WHERE m.tx_guid = t.guid AND m.memo LIKE '%' || {} || '%' ESCAPE '\\') \
             OR EXISTS(SELECT 1 FROM transaction_tags tt JOIN tags g ON g.guid = tt.tag_guid \
                       WHERE tt.tx_guid = t.guid AND g.name LIKE '%' || {} || '%' ESCAPE '\\') \
             OR EXISTS(SELECT 1 FROM transaction_payees tp JOIN payees p ON p.guid = tp.payee_guid \
                       WHERE tp.tx_guid = t.guid AND p.name LIKE '%' || {} || '%' ESCAPE '\\'))",
            vec![
                Box::new(s.clone()) as Box<dyn ToSql>,
                Box::new(s.clone()),
                Box::new(s.clone()),
                Box::new(s.clone()),
                Box::new(s.clone()),
                Box::new(s),
            ],
        );
    }
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

    pub fn is_empty(&self) -> bool {
        self.clauses.is_empty()
    }

    /// `AND`-prefixed clause list, or empty string when no clauses were added.
    /// Replaces the repeated `if w.clauses.is_empty() { String::new() } else …` pattern.
    pub fn and_clause(&self, prefix: &str) -> String {
        if self.is_empty() {
            String::new()
        } else {
            format!("{prefix} {}", self.joined())
        }
    }

    /// `WHERE`-prefixed clause list, or empty string when no clauses were added.
    /// Includes the leading space so call sites can write `t{where_clause}`.
    pub fn where_clause(&self) -> String {
        if self.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", self.joined())
        }
    }
}
