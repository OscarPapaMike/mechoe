use std::path::{Path, PathBuf};

use crate::{http::Client, index::CardRecord, paths::{card_art_path, card_json_path}, MdataError};

pub struct CardPaths {
    pub json: PathBuf,
    /// None if the card has no art_crop URL (rare).
    pub art: Option<PathBuf>,
}

/// Ensure `data/<SET>/<NUM>.json` and `data/<SET>/<NUM>.jpg` exist locally.
/// JSON is written from the index; art is fetched from Scryfall if not already cached.
pub fn ensure_card(
    client: &Client,
    data_dir: &Path,
    record: &CardRecord,
) -> Result<CardPaths, MdataError> {
    let json_path = card_json_path(data_dir, &record.set_code, &record.collector_number);
    let art_path = card_art_path(data_dir, &record.set_code, &record.collector_number);

    if let Some(parent) = json_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if !json_path.exists() {
        std::fs::write(&json_path, &record.card_json)?;
    }

    let art = if let Some(url) = &record.art_crop_url {
        if !art_path.exists() {
            client.download_to(url, &art_path)?;
        }
        Some(art_path)
    } else {
        None
    };

    Ok(CardPaths { json: json_path, art })
}
