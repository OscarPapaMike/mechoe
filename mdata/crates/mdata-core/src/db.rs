//! High-level database handle that hides rusqlite from downstream crates.

use std::path::Path;

use rusqlite::Connection;

use crate::{index, paths, MdataError};
pub use crate::index::CardRecord;

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open the index at `data_dir/_meta/index.db`. Returns `MdataError::NoIndex`
    /// if the file does not exist (run `mdata sync` first).
    pub fn open(data_dir: &Path) -> Result<Self, MdataError> {
        let path = paths::index_path(data_dir);
        let conn = index::open_index(&path)?;
        Ok(Self { conn })
    }

    /// Substring search on card name (case-insensitive LIKE). Returns up to
    /// `limit` candidate records for the caller to fuzzy-re-rank.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<CardRecord>, MdataError> {
        index::search_by_name(&self.conn, query, limit)
    }

    /// All printings of an exact card name, oldest first.
    pub fn find_by_name(&self, name: &str) -> Result<Vec<CardRecord>, MdataError> {
        index::find_by_name(&self.conn, name)
    }
}
