use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use mdata_core::{
    bulk::{download_bulk, fetch_bulk_info, stream_cards},
    http::Client,
    index::{create_index, find_by_name, find_by_set, find_by_set_num, insert_card, open_index},
    paths::{bulk_path, card_art_path, card_json_path, data_dir, index_path, symbols_dir},
    store::ensure_card,
    symbols::sync_symbols,
};

#[derive(Parser)]
#[command(name = "mdata", version, about = "Scryfall data manager for mechoe")]
struct Cli {
    /// Data directory (overrides MECHOE_DATA env var; default: ./data)
    #[arg(long, env = "MECHOE_DATA", global = true)]
    data_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Download Scryfall bulk data and build the local search index.
    /// Must be run at least once before other commands work offline.
    Sync,

    /// Fetch a card by name (oldest printing by default).
    Fetch {
        /// Exact card name, case-insensitive (e.g. "Pouncing Jaguar").
        name: String,
        /// Print all printings found without fetching any files.
        #[arg(long)]
        info_only: bool,
    },

    /// Fetch a specific printing by set code and collector number.
    Get {
        /// Three-letter set code, e.g. USG.
        set_code: String,
        /// Collector number, e.g. 269.
        collector_number: String,
    },

    /// Fetch all cards in a set (JSON + art crop).
    Pull {
        /// Three-letter set code, e.g. DRK.
        set_code: String,
    },

    /// Show all printings of a card without downloading anything.
    Info {
        /// Exact card name, case-insensitive.
        name: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let data = data_dir(cli.data_dir.as_deref());

    match cli.command {
        Command::Sync => cmd_sync(&data),
        Command::Fetch { name, info_only } => cmd_fetch(&data, &name, info_only),
        Command::Get { set_code, collector_number } => cmd_get(&data, &set_code, &collector_number),
        Command::Pull { set_code } => cmd_pull(&data, &set_code),
        Command::Info { name } => cmd_info(&data, &name),
    }
}

fn cmd_sync(data: &std::path::Path) -> Result<()> {
    let client = Client::new();

    eprintln!("[sync] fetching bulk-data manifest ...");
    let (url, updated_at) =
        fetch_bulk_info(&client).context("fetching bulk-data manifest")?;
    eprintln!("[sync] default_cards updated {updated_at}");

    let bulk = bulk_path(data);
    if bulk.exists() {
        eprintln!("[sync] bulk file already present, skipping download");
    } else {
        download_bulk(&client, &url, &bulk).context("downloading bulk data")?;
    }

    let idx = index_path(data);
    eprintln!("[sync] building index → {} ...", idx.display());

    // Remove stale index so we start fresh.
    if idx.exists() {
        std::fs::remove_file(&idx)?;
    }
    let conn = create_index(&idx).context("creating index")?;

    conn.execute_batch("BEGIN").context("starting transaction")?;
    let count =
        stream_cards(&bulk, |card| insert_card(&conn, card)).context("indexing cards")?;
    conn.execute_batch("COMMIT").context("committing transaction")?;

    eprintln!("[sync] indexed {count} cards");

    // The bulk file is large (~540 MB). Remove it now that the index is built;
    // all card JSON is stored in the SQLite index and extracted on demand.
    std::fs::remove_file(&bulk)?;
    eprintln!("[sync] removed bulk file (card JSON preserved in index)");

    // Fetch mana/card symbol SVGs from Scryfall.
    let sym_dir = symbols_dir(&data);
    eprintln!("[sync] fetching mana symbols → {} ...", sym_dir.display());
    let sym_stats = sync_symbols(&client, &sym_dir).context("syncing symbols")?;
    eprintln!(
        "[sync] symbols: {} total, {} downloaded, {} already cached",
        sym_stats.total, sym_stats.downloaded, sym_stats.skipped
    );

    Ok(())
}

fn cmd_fetch(data: &std::path::Path, name: &str, info_only: bool) -> Result<()> {
    let conn = open_index(&index_path(data))
        .context("opening index — run `mdata sync` first")?;
    let records = find_by_name(&conn, name).context("querying index")?;

    if records.is_empty() {
        anyhow::bail!(
            "card not found: {name:?}\nCheck spelling or run `mdata sync` to refresh the index."
        );
    }

    if info_only {
        println!("{} — {} printing(s)", records[0].name, records.len());
        for r in &records {
            println!("  {} #{:<5}  {}", r.set_code, r.collector_number, r.released_at);
        }
        return Ok(());
    }

    // Default: oldest printing (records are sorted released_at ASC).
    let record = &records[0];
    let client = Client::new();
    let paths = ensure_card(&client, data, record)
        .with_context(|| format!("fetching {} {} #{}", record.name, record.set_code, record.collector_number))?;

    println!("{} ({}) #{}", record.name, record.set_code, record.collector_number);
    println!("json: {}", paths.json.display());
    if let Some(art) = &paths.art {
        println!("art:  {}", art.display());
    }
    Ok(())
}

fn cmd_get(data: &std::path::Path, set_code: &str, collector_number: &str) -> Result<()> {
    let conn = open_index(&index_path(data))
        .context("opening index — run `mdata sync` first")?;
    let record = find_by_set_num(&conn, set_code, collector_number)
        .context("querying index")?
        .ok_or_else(|| {
            anyhow::anyhow!("card not found: {set_code} #{collector_number}")
        })?;

    let client = Client::new();
    let paths = ensure_card(&client, data, &record)
        .with_context(|| format!("fetching {} #{}", record.set_code, record.collector_number))?;

    println!("{} ({}) #{} — {}", record.name, record.set_code, record.collector_number, record.released_at);
    println!("json: {}", paths.json.display());
    if let Some(art) = &paths.art {
        println!("art:  {}", art.display());
    }
    Ok(())
}

fn cmd_pull(data: &std::path::Path, set_code: &str) -> Result<()> {
    let conn = open_index(&index_path(data))
        .context("opening index — run `mdata sync` first")?;
    let records = find_by_set(&conn, set_code).context("querying index")?;

    if records.is_empty() {
        anyhow::bail!(
            "set {set_code} not found in index — check the set code or run `mdata sync`"
        );
    }

    let set_upper = set_code.to_uppercase();
    eprintln!("[pull] {} cards in set {set_upper}", records.len());
    let client = Client::new();

    let mut json_written = 0usize;
    let mut art_written = 0usize;
    let mut skipped = 0usize;

    for (i, record) in records.iter().enumerate() {
        let json_existed =
            card_json_path(data, &record.set_code, &record.collector_number).exists();
        let art_existed =
            card_art_path(data, &record.set_code, &record.collector_number).exists();

        let paths = ensure_card(&client, data, record).with_context(|| {
            format!("fetching {} #{}", record.set_code, record.collector_number)
        })?;

        let new_json = !json_existed;
        let new_art = paths.art.is_some() && !art_existed;

        if new_json || new_art {
            eprintln!(
                "  [{}/{}] {} #{} — {}",
                i + 1,
                records.len(),
                record.set_code,
                record.collector_number,
                record.name
            );
            if new_json { json_written += 1; }
            if new_art  { art_written  += 1; }
        } else {
            skipped += 1;
        }
    }

    println!(
        "set {set_upper} — {} cards ({json_written} new JSON, {art_written} new art, {skipped} skipped)",
        records.len()
    );
    Ok(())
}

fn cmd_info(data: &std::path::Path, name: &str) -> Result<()> {
    let conn = open_index(&index_path(data))
        .context("opening index — run `mdata sync` first")?;
    let records = find_by_name(&conn, name).context("querying index")?;

    if records.is_empty() {
        anyhow::bail!("card not found: {name:?}");
    }

    println!("{} — {} printing(s)", records[0].name, records.len());
    for r in &records {
        let cached = card_json_path(data, &r.set_code, &r.collector_number).exists();
        let marker = if cached { "✓" } else { " " };
        println!(
            "  [{marker}] {} #{:<5}  {}",
            r.set_code, r.collector_number, r.released_at
        );
    }
    Ok(())
}
