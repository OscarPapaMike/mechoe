use thiserror::Error;

#[derive(Debug, Error)]
pub enum MdataError {
    #[error("HTTP 429 — rate limited by Scryfall; wait a few minutes and retry")]
    RateLimited,
    #[error("HTTP {status} from {url}: {body}")]
    Http { status: u16, url: String, body: String },
    #[error("transport error: {0}")]
    Transport(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("card not found: {0:?}")]
    NotFound(String),
    #[error("bulk data not downloaded — run `mdata sync` first")]
    NoBulkData,
    #[error("index not built — run `mdata sync` first")]
    NoIndex,
}
