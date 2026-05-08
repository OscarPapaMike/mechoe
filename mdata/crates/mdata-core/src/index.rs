use std::path::Path;

use rusqlite::{params, Connection};
use serde_json::Value;

use crate::MdataError;

pub fn open_index(path: &Path) -> Result<Connection, MdataError> {
    if !path.exists() {
        return Err(MdataError::NoIndex);
    }
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    Ok(conn)
}

pub fn create_index(path: &Path) -> Result<Connection, MdataError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    create_schema(&conn)?;
    Ok(conn)
}

fn create_schema(conn: &Connection) -> Result<(), MdataError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS cards (
            id               TEXT PRIMARY KEY,
            oracle_id        TEXT,
            name             TEXT NOT NULL,
            set_code         TEXT NOT NULL,
            collector_number TEXT NOT NULL,
            released_at      TEXT NOT NULL,
            art_crop_url     TEXT,
            card_json        TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_name    ON cards(name COLLATE NOCASE);
        CREATE INDEX IF NOT EXISTS idx_set     ON cards(set_code, collector_number);
        CREATE INDEX IF NOT EXISTS idx_oracle  ON cards(oracle_id, released_at);",
    )?;
    Ok(())
}

pub fn insert_card(conn: &Connection, card: &Value) -> Result<(), MdataError> {
    let id = card["id"].as_str().unwrap_or("");
    let name = card["name"].as_str().unwrap_or("");

    // skip malformed entries
    if id.is_empty() || name.is_empty() {
        return Ok(());
    }

    let oracle_id = card["oracle_id"].as_str();
    let set_code = card["set"].as_str().unwrap_or("").to_uppercase();
    let collector_number = card["collector_number"].as_str().unwrap_or("");
    let released_at = card["released_at"].as_str().unwrap_or("");

    // single-faced cards have image_uris directly; double-faced have card_faces[0].image_uris
    let art_crop_url = card["image_uris"]["art_crop"]
        .as_str()
        .or_else(|| card["card_faces"][0]["image_uris"]["art_crop"].as_str());

    let card_json = serde_json::to_string(card)?;

    conn.execute(
        "INSERT OR REPLACE INTO cards
             (id, oracle_id, name, set_code, collector_number, released_at, art_crop_url, card_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            oracle_id,
            name,
            set_code,
            collector_number,
            released_at,
            art_crop_url,
            card_json
        ],
    )?;

    Ok(())
}

#[derive(Debug, Clone)]
pub struct CardRecord {
    pub id: String,
    pub oracle_id: Option<String>,
    pub name: String,
    pub set_code: String,
    pub collector_number: String,
    pub released_at: String,
    pub art_crop_url: Option<String>,
    pub card_json: String,
}

/// All printings of a card by exact name (case-insensitive), oldest first.
pub fn find_by_name(conn: &Connection, name: &str) -> Result<Vec<CardRecord>, MdataError> {
    let mut stmt = conn.prepare(
        "SELECT id, oracle_id, name, set_code, collector_number, released_at, art_crop_url, card_json
         FROM cards
         WHERE name = ?1 COLLATE NOCASE
         ORDER BY released_at ASC, collector_number ASC",
    )?;
    let rows = stmt.query_map(params![name], row_to_record)?;
    rows.map(|r| r.map_err(MdataError::Db)).collect()
}

/// Single card by set code + collector number.
pub fn find_by_set_num(
    conn: &Connection,
    set_code: &str,
    num: &str,
) -> Result<Option<CardRecord>, MdataError> {
    let mut stmt = conn.prepare(
        "SELECT id, oracle_id, name, set_code, collector_number, released_at, art_crop_url, card_json
         FROM cards
         WHERE set_code = ?1 AND collector_number = ?2",
    )?;
    let mut rows = stmt.query_map(params![set_code.to_uppercase(), num], row_to_record)?;
    rows.next().transpose().map_err(MdataError::Db)
}

/// All cards in a set, sorted by collector number.
pub fn find_by_set(conn: &Connection, set_code: &str) -> Result<Vec<CardRecord>, MdataError> {
    let mut stmt = conn.prepare(
        "SELECT id, oracle_id, name, set_code, collector_number, released_at, art_crop_url, card_json
         FROM cards
         WHERE set_code = ?1
         ORDER BY CAST(collector_number AS INTEGER), collector_number",
    )?;
    let rows = stmt.query_map(params![set_code.to_uppercase()], row_to_record)?;
    rows.map(|r| r.map_err(MdataError::Db)).collect()
}

/// Full-text substring search across card names, ordered by match quality.
/// Returns up to `limit` results (pre-fuzzy-ranking — caller should re-rank).
pub fn search_by_name(conn: &Connection, query: &str, limit: usize) -> Result<Vec<CardRecord>, MdataError> {
    let pattern = format!("%{query}%");
    let mut stmt = conn.prepare(
        "SELECT id, oracle_id, name, set_code, collector_number, released_at, art_crop_url, card_json
         FROM cards
         WHERE name LIKE ?1
         ORDER BY released_at ASC, collector_number ASC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![pattern, limit as i64], row_to_record)?;
    rows.map(|r| r.map_err(MdataError::Db)).collect()
}

fn row_to_record(row: &rusqlite::Row<'_>) -> Result<CardRecord, rusqlite::Error> {
    Ok(CardRecord {
        id: row.get(0)?,
        oracle_id: row.get(1)?,
        name: row.get(2)?,
        set_code: row.get(3)?,
        collector_number: row.get(4)?,
        released_at: row.get(5)?,
        art_crop_url: row.get(6)?,
        card_json: row.get(7)?,
    })
}
