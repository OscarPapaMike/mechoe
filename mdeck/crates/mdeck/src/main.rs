use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rusqlite::params;

use mdata_core::{
    http::Client,
    index::{find_by_name, find_by_set_num, open_index},
    paths::{data_dir as resolve_data_dir, index_path},
    store::ensure_card,
};
use mcard_core::{render_png, Card, CardStyle, Dpi, RenderOptions};

// ── CLI ──────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "mdeck", version, about = "Deck file renderer for mechoe")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render all cards in a .deck file to PNG.
    Render {
        /// Path to a .deck file.
        deck: PathBuf,

        /// Output directory.
        #[arg(short, long, default_value = "/tmp/")]
        out: PathBuf,

        /// Render DPI (overrides any `# dpi N` in the deck file).
        #[arg(long)]
        dpi: Option<f32>,

        /// Card frame style: basic or classic (overrides `# style …` in deck).
        #[arg(long)]
        style: Option<String>,

        /// Render all copies of each card (e.g. "4 Leviathan" → 4 files).
        #[arg(long)]
        full_quantity: bool,

        /// Data directory override.
        #[arg(long, env = "MECHOE_DATA")]
        data_dir: Option<PathBuf>,
    },

    /// Generate a .deck file listing the first premodern printing of every
    /// unique card name (6 225 entries in "- SET NUM" format).
    GenPremodern {
        /// Output .deck file path.
        #[arg(short, long, default_value = "premodern.deck")]
        out: PathBuf,

        /// Data directory override.
        #[arg(long, env = "MECHOE_DATA")]
        data_dir: Option<PathBuf>,
    },
}

// ── Deck parsing ─────────────────────────────────────────────────────────────

#[derive(Debug)]
struct DeckEntry {
    kind:     EntryKind,
    quantity: u32,
}

#[derive(Debug)]
enum EntryKind {
    ByName(String),
    BySetNum(String, String), // set_code, collector_number
}

#[derive(Debug)]
struct DeckConfig {
    dpi:   f32,
    style: String,
}

impl Default for DeckConfig {
    fn default() -> Self {
        Self { dpi: 300.0, style: "basic".into() }
    }
}

fn parse_deck(path: &Path) -> Result<(DeckConfig, Vec<DeckEntry>)> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;

    let mut config = DeckConfig::default();
    let mut entries = Vec::new();

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() { continue; }

        let first = line.chars().next().unwrap();

        if first == '#' {
            // `# key value` — deck-level config
            let rest = line[1..].trim();
            let mut parts = rest.splitn(2, char::is_whitespace);
            let key = parts.next().unwrap_or("").trim();
            let val = parts.next().unwrap_or("").trim();
            match key.to_ascii_lowercase().as_str() {
                "dpi"   => { if let Ok(v) = val.parse::<f32>() { config.dpi   = v; } }
                "style" => { config.style = val.to_ascii_lowercase(); }
                _       => {}
            }
        } else if first == '-' {
            // `- SET NUM [qty]`
            let rest = line[1..].trim();
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() < 2 {
                eprintln!("  [warn] malformed set-line, skipping: {line:?}");
                continue;
            }
            let set_code = parts[0].to_uppercase();
            let num      = parts[1].to_string();
            let qty      = parts.get(2).and_then(|s| s.parse::<u32>().ok()).unwrap_or(1);
            entries.push(DeckEntry { kind: EntryKind::BySetNum(set_code, num), quantity: qty });
        } else if first.is_ascii_digit() {
            // `{qty} {card name}`
            let mut parts = line.splitn(2, char::is_whitespace);
            let qty  = parts.next().unwrap_or("1").parse::<u32>().unwrap_or(1);
            let name = parts.next().unwrap_or("").trim();
            if name.is_empty() { continue; }
            entries.push(DeckEntry { kind: EntryKind::ByName(name.to_string()), quantity: qty });
        } else {
            // Header / section label — print as visual separator, don't render
            eprintln!("── {line} ──");
        }
    }

    Ok((config, entries))
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn resolve_meta_subdir(data: &Path, name: &str) -> Option<PathBuf> {
    let p = data.join("_meta").join(name);
    if p.is_dir() { Some(p) } else { None }
}

// ── render subcommand ─────────────────────────────────────────────────────────

fn cmd_render(
    deck_path:      &Path,
    out_dir:        &Path,
    cli_dpi:        Option<f32>,
    cli_style:      Option<String>,
    full_quantity:  bool,
    data_override:  Option<PathBuf>,
) -> Result<()> {
    let (mut config, entries) = parse_deck(deck_path)?;

    // CLI flags take priority over deck-file `#` params.
    if let Some(d) = cli_dpi   { config.dpi   = d; }
    if let Some(s) = cli_style { config.style  = s; }

    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("creating output dir {}", out_dir.display()))?;

    let data   = resolve_data_dir(data_override.as_deref());
    let conn   = open_index(&index_path(&data))
        .context("opening index — run `mdata sync` first")?;
    let client = Client::new();

    let card_style = match config.style.as_str() {
        "classic" => CardStyle::Classic,
        _         => CardStyle::Basic,
    };

    let opts = RenderOptions {
        dpi:          Dpi(config.dpi),
        fonts_dir:    None,
        symbols_dir:  resolve_meta_subdir(&data, "symbols"),
        rails_dir:    resolve_meta_subdir(&data, "rails"),
        card_style,
        debug_layout: false,
    };

    let total_files: usize = if full_quantity {
        entries.iter().map(|e| e.quantity as usize).sum()
    } else {
        entries.len()
    };
    eprintln!(
        "[mdeck] {} entries → {} files at {} dpi → {}",
        entries.len(), total_files, config.dpi, out_dir.display()
    );

    let mut global_seq: u32 = 0;

    for entry in &entries {
        let record = match &entry.kind {
            EntryKind::ByName(name) => {
                let records = find_by_name(&conn, name)
                    .with_context(|| format!("querying {name:?}"))?;
                match records.into_iter().next() {
                    Some(r) => r,
                    None => {
                        eprintln!("  [warn] card not found: {name:?} — skipping");
                        continue;
                    }
                }
            }
            EntryKind::BySetNum(set, num) => {
                match find_by_set_num(&conn, set, num)
                    .with_context(|| format!("querying {set} #{num}"))?
                {
                    Some(r) => r,
                    None => {
                        eprintln!("  [warn] {set} #{num} not in index — skipping");
                        continue;
                    }
                }
            }
        };

        // Ensure JSON + art are cached locally.
        let paths = ensure_card(&client, &data, &record)
            .with_context(|| format!("fetching {} {}", record.name, record.set_code))?;

        let json_bytes = std::fs::read(&paths.json)
            .with_context(|| format!("reading {}", paths.json.display()))?;
        let card: Card = serde_json::from_slice(&json_bytes)
            .with_context(|| format!("parsing {}", paths.json.display()))?;

        let copies    = if full_quantity { entry.quantity } else { 1 };
        let safe_name = sanitize_name(&record.name);

        for copy in 1..=copies {
            global_seq += 1;
            let filename = format!("{:05}-{}-{:02}.png", global_seq, safe_name, copy);
            let out_path = out_dir.join(&filename);

            let png = render_png(&card, paths.art.as_deref(), &opts)
                .with_context(|| format!("rendering {}", record.name))?;

            std::fs::write(&out_path, &png)
                .with_context(|| format!("writing {}", out_path.display()))?;

            eprintln!("  [{global_seq}/{total_files}] {} ({}#{}) copy {copy} → {filename}",
                record.name, record.set_code, record.collector_number);
        }
    }

    eprintln!("[mdeck] done — {global_seq} files written");
    Ok(())
}

// ── gen-premodern subcommand ──────────────────────────────────────────────────

const PREMODERN_SETS: &[&str] = &[
    // Old School
    "LEA","LEB","ARN","ATQ","3ED","LEG","DRK","FEM",
    // Ice Age era
    "4ED","ICE","CHR","HML","ALL",
    // Mirage block
    "MIR","VIS","5ED","WTH",
    // Portal
    "POR","P02","PTK",
    // Tempest block
    "TMP","STH","EXO",
    // Urza block
    "USG","ULG","6ED","UDS",
    // Masques block
    "MMQ","NEM","PCY",
    // Invasion block
    "INV","PLS","7ED","APC",
    // Odyssey block
    "ODY","TOR","JUD",
    // Onslaught block
    "ONS","LGN","SCG",
];

fn cmd_gen_premodern(out_path: &Path, data_override: Option<PathBuf>) -> Result<()> {
    let data = resolve_data_dir(data_override.as_deref());
    let conn = open_index(&index_path(&data))
        .context("opening index — run `mdata sync` first")?;

    let set_list = PREMODERN_SETS.iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(", ");

    // First printing of each unique name across premodern sets.
    let query = format!(
        "WITH ranked AS (
             SELECT name, set_code, collector_number, released_at,
                    ROW_NUMBER() OVER (
                        PARTITION BY name
                        ORDER BY released_at ASC, set_code ASC,
                                 CAST(collector_number AS INTEGER) ASC
                    ) AS rn
             FROM cards
             WHERE set_code IN ({set_list})
         )
         SELECT name, set_code, collector_number, released_at
         FROM ranked WHERE rn = 1
         ORDER BY released_at ASC, set_code ASC,
                  CAST(collector_number AS INTEGER) ASC"
    );

    let mut stmt = conn.prepare(&query)
        .context("preparing premodern query")?;

    #[derive(Debug)]
    struct Row { #[allow(dead_code)] name: String, set_code: String, collector_number: String }

    let rows: Vec<Row> = stmt.query_map(params![], |r| Ok(Row {
        name:             r.get(0)?,
        set_code:         r.get(1)?,
        collector_number: r.get(2)?,
    }))?.filter_map(|r| r.ok()).collect();

    let count = rows.len();

    // Group by set so we can write section headers.
    let mut lines: Vec<String> = Vec::with_capacity(count + 16);
    lines.push("# Unique first printings of every premodern card (LEA–SCG)".into());
    lines.push(format!("# {} cards total", count));
    lines.push(String::new());

    let mut current_set = String::new();
    for row in &rows {
        if row.set_code != current_set {
            if !current_set.is_empty() { lines.push(String::new()); }
            lines.push(format!("{}", row.set_code));
            current_set = row.set_code.clone();
        }
        lines.push(format!("- {} {}", row.set_code, row.collector_number));
    }
    lines.push(String::new());

    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(out_path, lines.join("\n"))
        .with_context(|| format!("writing {}", out_path.display()))?;

    eprintln!("[mdeck] wrote {count} entries → {}", out_path.display());
    Ok(())
}

// ── entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Render { deck, out, dpi, style, full_quantity, data_dir } =>
            cmd_render(&deck, &out, dpi, style, full_quantity, data_dir),
        Command::GenPremodern { out, data_dir } =>
            cmd_gen_premodern(&out, data_dir),
    }
}
