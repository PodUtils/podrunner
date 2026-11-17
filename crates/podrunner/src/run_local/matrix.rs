//! The cells of a static matrix, as GitHub makes them (#225): the product
//! of the vectors, less each `exclude`, then each `include`. A value that
//! is an expression is not static: its key is left out.

use std::collections::BTreeMap;

use pod_actions_workflow::yaml::{Kind, Node};

/// One cell: each key of the matrix and its value.
pub(super) type Cell = BTreeMap<String, String>;

/// The text of a static value.
fn text(n: &Node) -> Option<String> {
    match &n.kind {
        Kind::String(s) if !s.contains("${{") => Some(s.clone()),
        Kind::Number(x) => Some(x.to_string()),
        Kind::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// The entries of a list of maps (`include`, `exclude`). An entry with a
/// value that is not static is left out: the local mode cannot decide it,
/// and an empty entry would match each cell (#240).
fn entries(n: &Node) -> Vec<Cell> {
    let Kind::Seq(items) = &n.kind else { return Vec::new() };
    items
        .iter()
        .filter_map(|i| match &i.kind {
            Kind::Map(m) => m.iter().map(|(k, v)| Some((k.name.clone(), text(v)?))).collect::<Option<Cell>>(),
            _ => None,
        })
        .filter(|e| !e.is_empty())
        .collect()
}

/// The cells of the matrix of `job`; one empty cell for a job with none.
pub(super) fn cells(job: &pod_actions_workflow::Job) -> Vec<Cell> {
    let Some(Kind::Map(m)) = job.strategy.as_ref().and_then(|s| s.matrix.as_ref()).map(|m| &m.kind) else {
        return vec![Cell::new()];
    };
    let (mut include, mut exclude) = (Vec::new(), Vec::new());
    let mut out: Vec<Cell> = vec![Cell::new()];
    let mut vector_keys = Vec::new();
    for (k, v) in m {
        match (k.name.as_str(), &v.kind) {
            ("include", _) => include = entries(v),
            ("exclude", _) => exclude = entries(v),
            (key, Kind::Seq(items)) => {
                let values: Vec<String> = items.iter().filter_map(text).collect();
                vector_keys.push(key.to_string());
                out = out
                    .iter()
                    .flat_map(|c| {
                        values.iter().map(move |v| {
                            let mut c = c.clone();
                            c.insert(key.to_string(), v.clone());
                            c
                        })
                    })
                    .collect();
            }
            _ => {}
        }
    }
    // An exclude drops each cell that has each of its values.
    out.retain(|c| !exclude.iter().any(|e| e.iter().all(|(k, v)| c.get(k) == Some(v))));
    if vector_keys.is_empty() {
        out.clear();
    }
    // An include extends each cell that it does not change, else it is a cell
    // of its own.
    for inc in include {
        let mut matched = false;
        for c in out.iter_mut() {
            let fits = inc.iter().all(|(k, v)| !vector_keys.contains(k) || c.get(k) == Some(v));
            if fits {
                // An original value stays; an added value can be overwritten
                // by a later include, as GitHub says (#239).
                for (k, v) in inc.iter().filter(|(k, _)| !vector_keys.contains(k)) {
                    c.insert(k.clone(), v.clone());
                }
                matched = true;
            }
        }
        if !matched {
            out.push(inc);
        }
    }
    if out.is_empty() {
        out.push(Cell::new());
    }
    out
}

#[cfg(test)]
mod tests {
    /// #225: an excluded cell is no cell; an include adds a cell or a key.
    #[test]
    fn the_cells_of_a_matrix() {
        let w = pod_actions_workflow::parse(
            "on: push\njobs:\n  j:\n    runs-on: x\n    strategy:\n      matrix:\n        v: [15, 16]\n        \
             os: [a]\n        exclude: [{v: 16}]\n        include: [{v: 15, extra: e}, {v: 17}]\n    steps: [{run: x}]\n",
        )
        .unwrap()
        .workflow;
        let got: Vec<String> = super::cells(&w.jobs[0])
            .iter()
            .map(|c| c.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(","))
            .collect();
        assert_eq!(got, ["extra=e,os=a,v=15", "v=17"]);
    }

    fn cells_of(matrix: &str) -> Vec<String> {
        let text = format!(
            "on: push\njobs:\n  j:\n    runs-on: x\n    strategy:\n      matrix:\n{matrix}    steps: [{{run: x}}]\n"
        );
        let w = pod_actions_workflow::parse(&text).unwrap().workflow;
        super::cells(&w.jobs[0])
            .iter()
            .map(|c| c.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(","))
            .collect()
    }

    /// #239: a later include overwrites a value that an earlier one added.
    #[test]
    fn a_later_include_overwrites_an_added_value() {
        assert_eq!(
            cells_of("        v: [1]\n        include: [{v: 1, extra: a}, {v: 1, extra: b}]\n"),
            ["extra=b,v=1"]
        );
    }

    /// #240: an exclude with an expression drops no cell.
    #[test]
    fn an_exclude_with_an_expression_drops_no_cell() {
        assert_eq!(cells_of("        os: [a, b]\n        exclude: [{os: '${{ inputs.x }}'}]\n"), ["os=a", "os=b"]);
    }
}
