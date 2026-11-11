//! The journal: one JSON line for each transition, in
//! `STATE_DIR/journal.jsonl`, and the reading of it at start.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use podrunner_schema::JobId;

use crate::state::Line;

/// The writer of the journal.
pub struct Journal {
    path: PathBuf,
    file: File,
}

impl Journal {
    pub fn open(state_dir: &Path) -> std::io::Result<Journal> {
        std::fs::create_dir_all(state_dir)?;
        let path = state_dir.join("journal.jsonl");
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Journal { path, file })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append one line, and flush it: a kill of podrunner loses no
    /// transition that it wrote.
    pub fn write(&mut self, line: &Line) -> std::io::Result<()> {
        let mut text = serde_json::to_string(line).map_err(std::io::Error::other)?;
        text.push('\n');
        self.file.write_all(text.as_bytes())?;
        self.file.flush()
    }
}

/// What the journal says at start.
#[derive(Debug, Default)]
pub struct Replay {
    /// The last line of each job that is not final, by source and job id.
    pub open: BTreeMap<(String, JobId), Line>,
    /// The highest unit number in the journal, plus 1.
    pub next_unit: u64,
    /// The lines that could not be read (a torn last line after a kill).
    pub bad_lines: u64,
}

/// Read a journal. A missing file is an empty journal.
pub fn replay(state_dir: &Path) -> std::io::Result<Replay> {
    let mut r = Replay::default();
    let f = match File::open(state_dir.join("journal.jsonl")) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(r),
        Err(e) => return Err(e),
    };
    for line in BufReader::new(f).lines() {
        let Ok(line) = serde_json::from_str::<Line>(&line?) else {
            r.bad_lines += 1;
            continue;
        };
        if let Some(n) = line.unit_id.as_ref().and_then(|u| u.as_str()[2..].parse::<u64>().ok()) {
            r.next_unit = r.next_unit.max(n + 1);
        }
        let key = (line.source.clone(), line.job_id.clone());
        if line.state.is_final() {
            r.open.remove(&key);
        } else {
            // A later line without a unit keeps the unit of an earlier one.
            let unit = line.unit_id.clone().or_else(|| r.open.get(&key).and_then(|l| l.unit_id.clone()));
            let kind = line.unit_kind.clone().or_else(|| r.open.get(&key).and_then(|l| l.unit_kind.clone()));
            r.open.insert(key, Line { unit_id: unit, unit_kind: kind, ..line });
        }
    }
    Ok(r)
}
