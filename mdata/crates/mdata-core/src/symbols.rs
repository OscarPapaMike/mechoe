use std::path::Path;

use crate::{http::Client, MdataError};

const SYMBOLOGY_URL: &str = "https://api.scryfall.com/symbology";

/// Convert a Scryfall symbol code like `{W/U}` to a filename like `WU`.
/// Strips `{`, `}`, and `/`.
pub fn symbol_filename(code: &str) -> String {
    code.chars()
        .filter(|c| *c != '{' && *c != '}' && *c != '/')
        .collect()
}

#[derive(Debug, Default)]
pub struct SymbolSyncStats {
    pub total: usize,
    pub downloaded: usize,
    pub skipped: usize,
}

/// Fetch all mana/card symbols from Scryfall and store them as SVG files
/// under `symbols_dir`. Existing files are skipped.
pub fn sync_symbols(
    client: &Client,
    symbols_dir: &Path,
) -> Result<SymbolSyncStats, MdataError> {
    std::fs::create_dir_all(symbols_dir)?;

    let resp = client.get_json(SYMBOLOGY_URL)?;
    let entries = resp["data"]
        .as_array()
        .ok_or_else(|| MdataError::Transport("symbology response missing 'data' array".into()))?;

    let mut stats = SymbolSyncStats { total: entries.len(), ..Default::default() };

    for entry in entries {
        let code = match entry["symbol"].as_str() {
            Some(s) => s,
            None => continue,
        };
        let svg_uri = match entry["svg_uri"].as_str() {
            Some(s) => s,
            None => continue,
        };

        let filename = symbol_filename(code);
        let dest = symbols_dir.join(format!("{filename}.svg"));

        if dest.exists() {
            stats.skipped += 1;
            continue;
        }

        client.download_to(svg_uri, &dest)?;
        stats.downloaded += 1;
    }

    Ok(stats)
}
