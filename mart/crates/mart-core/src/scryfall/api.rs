//! Scryfall HTTP client (blocking, ureq-based) and set-pull workflow.
//!
//! Rate-limit policy:
//!   * A minimum gap of 150 ms between requests (Scryfall asks for 50–100 ms).
//!   * HTTP 429 → typed `ApiError::RateLimited`. The set-pull workflow stops
//!     immediately on 429, leaving any partial output in place. Re-running the
//!     pull will skip files that already exist.
//!
//! Per Scryfall's docs, image servers (`cards.scryfall.io`, `svgs.scryfall.io`)
//! are not subject to the same rate limits as `api.scryfall.com`, but we
//! throttle uniformly to be a polite consumer.

use std::cell::Cell;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;
use ureq::{Agent, AgentBuilder};

const USER_AGENT: &str =
    concat!("mart/", env!("CARGO_PKG_VERSION"), " (+https://github.com/OscarPapaMike/mechoe)");
const ACCEPT: &str = "application/json;q=0.9,*/*;q=0.8";
const API_BASE: &str = "https://api.scryfall.com";
const DEFAULT_GAP_MS: u64 = 150;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("HTTP 429 from Scryfall — rate limited. Halting. Wait several minutes and re-run; existing files will be skipped.")]
    RateLimited,
    #[error("HTTP {status} from {url}: {body}")]
    Http { status: u16, url: String, body: String },
    #[error("transport error contacting {url}: {detail}")]
    Transport { url: String, detail: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub struct Client {
    agent: Agent,
    min_gap: Duration,
    last_request: Cell<Option<Instant>>,
}

impl Client {
    pub fn new() -> Self {
        let agent = AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(60))
            .user_agent(USER_AGENT)
            .build();
        Self {
            agent,
            min_gap: Duration::from_millis(DEFAULT_GAP_MS),
            last_request: Cell::new(None),
        }
    }

    fn throttle(&self) {
        if let Some(prev) = self.last_request.get() {
            let elapsed = prev.elapsed();
            if elapsed < self.min_gap {
                std::thread::sleep(self.min_gap - elapsed);
            }
        }
        self.last_request.set(Some(Instant::now()));
    }

    fn get(&self, url: &str) -> Result<ureq::Response, ApiError> {
        self.throttle();
        match self
            .agent
            .get(url)
            .set("Accept", ACCEPT)
            .call()
        {
            Ok(r) => Ok(r),
            Err(ureq::Error::Status(429, _)) => Err(ApiError::RateLimited),
            Err(ureq::Error::Status(status, resp)) => Err(ApiError::Http {
                status,
                url: url.to_string(),
                body: resp.into_string().unwrap_or_default(),
            }),
            Err(ureq::Error::Transport(e)) => Err(ApiError::Transport {
                url: url.to_string(),
                detail: e.to_string(),
            }),
        }
    }

    /// Search for all printings in `set_code` (e.g. "DRK"). Handles pagination.
    pub fn search_set(&self, set_code: &str) -> Result<Vec<Value>, ApiError> {
        let initial = format!(
            "{API_BASE}/cards/search?q=e%3A{}&unique=prints&order=set",
            set_code.to_lowercase()
        );
        self.search_paginated(&initial)
    }

    fn search_paginated(&self, first_url: &str) -> Result<Vec<Value>, ApiError> {
        let mut all = Vec::new();
        let mut url = first_url.to_string();
        loop {
            let resp = self.get(&url)?;
            let page: Value = resp.into_json()?;
            if let Some(arr) = page.get("data").and_then(|d| d.as_array()) {
                all.extend(arr.iter().cloned());
            }
            let has_more = page.get("has_more").and_then(|v| v.as_bool()).unwrap_or(false);
            let next = page.get("next_page").and_then(|v| v.as_str()).map(String::from);
            match (has_more, next) {
                (true, Some(n)) => url = n,
                _ => break,
            }
        }
        Ok(all)
    }

    /// Stream a binary URL to disk.
    pub fn download_to(&self, url: &str, path: &Path) -> Result<u64, ApiError> {
        self.throttle();
        let resp = match self.agent.get(url).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(429, _)) => return Err(ApiError::RateLimited),
            Err(ureq::Error::Status(status, resp)) => return Err(ApiError::Http {
                status,
                url: url.to_string(),
                body: resp.into_string().unwrap_or_default(),
            }),
            Err(ureq::Error::Transport(e)) => return Err(ApiError::Transport {
                url: url.to_string(),
                detail: e.to_string(),
            }),
        };
        let mut file = std::fs::File::create(path)?;
        let mut reader = resp.into_reader().take(50 * 1024 * 1024);
        Ok(std::io::copy(&mut reader, &mut file)?)
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
pub struct PullStats {
    pub cards: usize,
    pub json_written: usize,
    pub art_written: usize,
    pub skipped_existing: usize,
}

pub struct PullOptions<'a> {
    pub set_code: &'a str,
    pub out_dir: PathBuf,
    pub with_art: bool,
    pub on_progress: Option<Box<dyn FnMut(&str) + 'a>>,
}

/// Pull every card in `set_code`. Per-card JSON is written to
/// `<out_dir>/<collector_number>.json`; art_crop to
/// `<out_dir>/<collector_number>.jpg`. Existing files are skipped.
///
/// On HTTP 429 from any request, returns `ApiError::RateLimited` immediately
/// without continuing — partial output is left in place and a re-run will
/// resume by skipping existing files.
pub fn pull_set(client: &Client, mut opts: PullOptions) -> Result<PullStats, ApiError> {
    std::fs::create_dir_all(&opts.out_dir)?;

    let mut emit = |s: &str| {
        if let Some(cb) = opts.on_progress.as_mut() {
            cb(s);
        } else {
            eprintln!("{s}");
        }
    };

    emit(&format!("[scryfall] searching set {} ...", opts.set_code.to_uppercase()));
    let cards = client.search_set(opts.set_code)?;
    emit(&format!("[scryfall] {} cards in set {}", cards.len(), opts.set_code.to_uppercase()));

    let mut stats = PullStats { cards: cards.len(), ..Default::default() };

    for (idx, card) in cards.iter().enumerate() {
        let num = card
            .get("collector_number")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let name = card.get("name").and_then(|v| v.as_str()).unwrap_or("?");

        let json_path = opts.out_dir.join(format!("{num}.json"));
        let art_path = opts.out_dir.join(format!("{num}.jpg"));

        let mut wrote_anything = false;

        if json_path.exists() {
            stats.skipped_existing += 1;
        } else {
            let body = serde_json::to_string_pretty(card)?;
            std::fs::write(&json_path, body)?;
            stats.json_written += 1;
            wrote_anything = true;
        }

        if opts.with_art {
            // Single-faced card.
            let art_url = card
                .get("image_uris")
                .and_then(|u| u.get("art_crop"))
                .and_then(|v| v.as_str());
            if let Some(url) = art_url {
                if !art_path.exists() {
                    client.download_to(url, &art_path)?;
                    stats.art_written += 1;
                    wrote_anything = true;
                }
            } else if let Some(faces) = card.get("card_faces").and_then(|f| f.as_array()) {
                for (i, face) in faces.iter().enumerate() {
                    let face_url = face
                        .get("image_uris")
                        .and_then(|u| u.get("art_crop"))
                        .and_then(|v| v.as_str());
                    if let Some(url) = face_url {
                        let face_path = opts.out_dir.join(format!("{num}_face{i}.jpg"));
                        if !face_path.exists() {
                            client.download_to(url, &face_path)?;
                            stats.art_written += 1;
                            wrote_anything = true;
                        }
                    }
                }
            }
        }

        if wrote_anything {
            emit(&format!(
                "  [{}/{}] {} \u{2014} {}",
                idx + 1,
                cards.len(),
                num,
                name
            ));
        }
    }

    let manifest = serde_json::json!({
        "set_code": opts.set_code.to_uppercase(),
        "card_count": stats.cards,
        "json_written": stats.json_written,
        "art_written": stats.art_written,
        "skipped_existing": stats.skipped_existing,
    });
    std::fs::write(
        opts.out_dir.join("_manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    Ok(stats)
}
