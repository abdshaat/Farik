//! The machine-level index of the owner's Claude Code history, apart from every project's log.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::Connection;

use crate::error::StoreError;
use crate::migrations;

/// The index's file name in Catervas's state folder.
pub const MACHINE_DB: &str = "machine.db";

const BUSY_TIMEOUT: Duration = Duration::from_secs(30);

/// The open index. It has one writer at a time, so it uses SQLite's default rollback journal.
pub struct MachineIndex {
    pub(crate) connection: Connection,
}

/// Opens the index at `path`, making its folder and the file when they are not there, and brings it
/// to the shape this version expects.
///
/// # Errors
///
/// `Io` when the folder cannot be made; `Sqlite` when the file cannot be opened or migrated.
pub fn open_machine_index(path: &Path, now: DateTime<Utc>) -> Result<MachineIndex, StoreError> {
    if let Some(directory) = path
        .parent()
        .filter(|directory| !directory.as_os_str().is_empty())
    {
        std::fs::create_dir_all(directory)?;
    }
    let mut connection = Connection::open(path)?;
    connection.busy_timeout(BUSY_TIMEOUT)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    migrations::apply_machine(&mut connection, now)?;
    Ok(MachineIndex { connection })
}

/// Deletes the index and its rollback journal, opening and reading nothing. `true` when there was an
/// index to delete.
///
/// # Errors
///
/// `Io` when a file is there and cannot be removed.
pub fn forget_machine_index(path: &Path) -> Result<bool, StoreError> {
    let mut journal = path.as_os_str().to_owned();
    journal.push("-journal");
    let mut existed = false;
    for file in [PathBuf::from(path), PathBuf::from(journal)] {
        match std::fs::remove_file(&file) {
            Ok(()) => existed = true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(existed)
}

impl MachineIndex {
    /// The migration versions applied to this index, in order.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the ledger cannot be read.
    pub fn applied_migrations(&self) -> Result<Vec<i64>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT version FROM schema_migrations ORDER BY version")?;
        let versions = statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        Ok(versions)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use rusqlite::Connection;

    use super::*;
    use crate::files::fixtures::TempProject;

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 12, 12, 0, 0)
            .single()
            .expect("a valid time")
    }

    #[test]
    fn opens_the_machine_index_and_records_its_migration() {
        let temp = TempProject::new("opens-machine-index");
        let path = temp.root.join("state").join(MACHINE_DB);
        let index = open_machine_index(&path, now()).expect("the index opens");
        assert_eq!(index.applied_migrations().expect("versions"), vec![1]);
        drop(index);
        let again = open_machine_index(&path, now()).expect("it opens again");
        assert_eq!(again.applied_migrations().expect("versions"), vec![1]);
        drop(again);
        let connection = Connection::open(&path).expect("the file opens");
        let mut tables: Vec<String> = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name != 'schema_migrations'")
            .expect("a statement")
            .query_map([], |row| row.get(0))
            .expect("a query")
            .collect::<Result<_, _>>()
            .expect("rows");
        tables.sort();
        assert_eq!(
            tables,
            vec!["projects", "replies", "sessions", "transcript_files"]
        );
        assert_eq!(
            crate::migrations::known_versions(),
            (1..=15).collect::<Vec<i64>>()
        );
    }

    #[test]
    fn forgets_the_machine_index() {
        let temp = TempProject::new("forgets-machine-index");
        let path = temp.root.join(MACHINE_DB);
        drop(open_machine_index(&path, now()).expect("the index opens"));
        let journal = temp.root.join("machine.db-journal");
        std::fs::write(&journal, b"x").expect("a journal");
        let state = temp.root.join("state.json");
        std::fs::write(&state, b"{}").expect("a state file");
        assert!(forget_machine_index(&path).expect("forgets"));
        assert!(!path.exists());
        assert!(!journal.exists());
        assert!(state.exists());
        assert!(!forget_machine_index(&path).expect("forgets again"));
    }
}
