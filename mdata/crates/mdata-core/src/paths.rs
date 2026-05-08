use std::path::{Path, PathBuf};

/// Resolve the data directory. Precedence:
/// 1. explicit override (from --data-dir flag)
/// 2. MECHOE_DATA env var
/// 3. default: ./data relative to cwd
pub fn data_dir(override_path: Option<&Path>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_owned();
    }
    if let Ok(v) = std::env::var("MECHOE_DATA") {
        return PathBuf::from(v);
    }
    PathBuf::from("data")
}

pub fn meta_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("_meta")
}

pub fn bulk_path(data_dir: &Path) -> PathBuf {
    meta_dir(data_dir).join("default_cards.json")
}

pub fn index_path(data_dir: &Path) -> PathBuf {
    meta_dir(data_dir).join("index.db")
}

pub fn symbols_dir(data_dir: &Path) -> PathBuf {
    meta_dir(data_dir).join("symbols")
}

pub fn card_json_path(data_dir: &Path, set_code: &str, collector_number: &str) -> PathBuf {
    data_dir
        .join(set_code.to_uppercase())
        .join(format!("{collector_number}.json"))
}

pub fn card_art_path(data_dir: &Path, set_code: &str, collector_number: &str) -> PathBuf {
    data_dir
        .join(set_code.to_uppercase())
        .join(format!("{collector_number}.jpg"))
}
