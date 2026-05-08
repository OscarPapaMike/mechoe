use std::cell::Cell;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};

use ureq::{Agent, AgentBuilder};

use crate::MdataError;

const USER_AGENT: &str =
    concat!("mdata/", env!("CARGO_PKG_VERSION"), " (+https://github.com/OscarPapaMike/mechoe)");
const MIN_GAP_MS: u64 = 150;

pub struct Client {
    agent: Agent,
    min_gap: Duration,
    last_req: Cell<Option<Instant>>,
}

impl Client {
    pub fn new() -> Self {
        Self {
            agent: AgentBuilder::new()
                .timeout_connect(Duration::from_secs(10))
                .timeout_read(Duration::from_secs(120))
                .user_agent(USER_AGENT)
                .build(),
            min_gap: Duration::from_millis(MIN_GAP_MS),
            last_req: Cell::new(None),
        }
    }

    fn throttle(&self) {
        if let Some(prev) = self.last_req.get() {
            let elapsed = prev.elapsed();
            if elapsed < self.min_gap {
                std::thread::sleep(self.min_gap - elapsed);
            }
        }
        self.last_req.set(Some(Instant::now()));
    }

    pub fn get_json(&self, url: &str) -> Result<serde_json::Value, MdataError> {
        self.throttle();
        match self.agent.get(url).set("Accept", "application/json").call() {
            Ok(r) => Ok(r.into_json()?),
            Err(ureq::Error::Status(429, _)) => Err(MdataError::RateLimited),
            Err(ureq::Error::Status(status, resp)) => Err(MdataError::Http {
                status,
                url: url.to_string(),
                body: resp.into_string().unwrap_or_default(),
            }),
            Err(ureq::Error::Transport(e)) => Err(MdataError::Transport(e.to_string())),
        }
    }

    /// Stream a URL to disk. Returns bytes written.
    pub fn download_to(&self, url: &str, path: &Path) -> Result<u64, MdataError> {
        self.throttle();
        let resp = match self.agent.get(url).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(429, _)) => return Err(MdataError::RateLimited),
            Err(ureq::Error::Status(status, resp)) => {
                return Err(MdataError::Http {
                    status,
                    url: url.to_string(),
                    body: resp.into_string().unwrap_or_default(),
                })
            }
            Err(ureq::Error::Transport(e)) => {
                return Err(MdataError::Transport(e.to_string()))
            }
        };
        let mut file = std::fs::File::create(path)?;
        // 4 GB cap — bulk data is ~540 MB compressed
        let mut reader = resp.into_reader().take(4 * 1024 * 1024 * 1024);
        Ok(std::io::copy(&mut reader, &mut file)?)
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}
