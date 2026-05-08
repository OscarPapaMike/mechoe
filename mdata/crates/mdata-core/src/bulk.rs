use std::fmt;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use flate2::read::GzDecoder;
use serde::de::{Deserializer as _, SeqAccess, Visitor};

use crate::{http::Client, MdataError};

const BULK_DATA_URL: &str = "https://api.scryfall.com/bulk-data";
const DEFAULT_CARDS_TYPE: &str = "default_cards";

/// Fetch the bulk-data manifest and return (download_uri, updated_at) for default_cards.
pub fn fetch_bulk_info(client: &Client) -> Result<(String, String), MdataError> {
    let manifest = client.get_json(BULK_DATA_URL)?;
    let entries = manifest["data"]
        .as_array()
        .ok_or_else(|| MdataError::Transport("bulk-data response missing 'data' array".into()))?;

    for entry in entries {
        if entry["type"].as_str() == Some(DEFAULT_CARDS_TYPE) {
            let uri = entry["download_uri"]
                .as_str()
                .ok_or_else(|| MdataError::Transport("missing download_uri in bulk manifest".into()))?
                .to_string();
            let updated = entry["updated_at"].as_str().unwrap_or("").to_string();
            return Ok((uri, updated));
        }
    }

    Err(MdataError::Transport(format!(
        "no '{DEFAULT_CARDS_TYPE}' entry in bulk-data manifest"
    )))
}

/// Download the gzip-compressed bulk file to `dest`. Returns bytes written.
pub fn download_bulk(client: &Client, url: &str, dest: &Path) -> Result<u64, MdataError> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    eprintln!("[mdata] downloading bulk data → {}", dest.display());
    let bytes = client.download_to(url, dest)?;
    eprintln!("[mdata] downloaded {:.0} MB", bytes as f64 / 1_000_000.0);
    Ok(bytes)
}

// ---- streaming JSON array parser ----

struct CardStreamer<F> {
    on_card: F,
}

impl<'de, F> Visitor<'de> for CardStreamer<F>
where
    F: FnMut(&serde_json::Value) -> Result<(), MdataError>,
{
    type Value = usize;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a JSON array of Scryfall card objects")
    }

    fn visit_seq<A: SeqAccess<'de>>(mut self, mut seq: A) -> Result<usize, A::Error> {
        let mut count = 0usize;
        while let Some(card) = seq.next_element::<serde_json::Value>()? {
            (self.on_card)(&card).map_err(|e| serde::de::Error::custom(e.to_string()))?;
            count += 1;
            if count % 10_000 == 0 {
                eprintln!("[mdata] indexed {count} cards ...");
            }
        }
        Ok(count)
    }
}

/// Stream-parse the bulk file, calling `on_card` for each card object.
/// Automatically handles both gzip-compressed and plain JSON files.
/// Memory usage is O(one card) — the whole file is never held in memory.
pub fn stream_cards<F>(bulk_path: &Path, on_card: F) -> Result<usize, MdataError>
where
    F: FnMut(&serde_json::Value) -> Result<(), MdataError>,
{
    let file = std::fs::File::open(bulk_path).map_err(|_| MdataError::NoBulkData)?;
    let mut buf = BufReader::with_capacity(256 * 1024, file);

    // Peek at the first two bytes to detect gzip magic (0x1f 0x8b).
    let magic = buf.fill_buf()?;
    let is_gzip = magic.len() >= 2 && magic[0] == 0x1f && magic[1] == 0x8b;

    let count = if is_gzip {
        let gz: Box<dyn Read> = Box::new(GzDecoder::new(buf));
        let mut de = serde_json::Deserializer::from_reader(gz);
        de.deserialize_seq(CardStreamer { on_card }).map_err(MdataError::Json)?
    } else {
        let mut de = serde_json::Deserializer::from_reader(buf);
        de.deserialize_seq(CardStreamer { on_card }).map_err(MdataError::Json)?
    };

    Ok(count)
}
