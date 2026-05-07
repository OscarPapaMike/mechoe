use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};

use mart_core::{
    render_png,
    scryfall::api::{pull_set, Client, PullOptions},
    Card, Dpi, RenderOptions,
};

#[derive(Parser)]
#[command(name = "mart", version, about = "MTG proxy card generator")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render a single card to PNG.
    Render {
        /// Path to a Scryfall-format JSON file.
        card_json: PathBuf,
        /// Optional artwork PNG/JPEG. If omitted, a placeholder fills the art box.
        art: Option<PathBuf>,
        /// Output PNG path.
        #[arg(short, long, default_value = "out.png")]
        out: PathBuf,
        /// Render DPI (default 300 → 750×1050 px).
        #[arg(long, default_value_t = 300.0)]
        dpi: f32,
        /// Directory containing Beleren-Bold.ttf and other typefaces.
        #[arg(long)]
        fonts_dir: Option<PathBuf>,
        /// Directory containing Scryfall mana SVGs (R.svg, G.svg, ...).
        #[arg(long)]
        symbols_dir: Option<PathBuf>,
    },
    /// Archive every card in a Scryfall set (JSON + art_crop image) to a local
    /// directory. Existing files are skipped, so re-running resumes safely.
    Pull {
        /// Three-letter set code (e.g. DRK, TMP, LEA). Case-insensitive.
        set_code: String,
        /// Output directory. Defaults to ~/cards/<SET>.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Skip downloading art_crop images (JSON only).
        #[arg(long)]
        no_art: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Render { card_json, art, out, dpi, fonts_dir, symbols_dir } => {
            cmd_render(&card_json, art.as_deref(), &out, dpi, fonts_dir, symbols_dir)
        }
        Command::Pull { set_code, out, no_art } => cmd_pull(&set_code, out, !no_art),
    }
}

fn cmd_render(
    card_json: &Path,
    art: Option<&Path>,
    out: &Path,
    dpi: f32,
    fonts_dir: Option<PathBuf>,
    symbols_dir: Option<PathBuf>,
) -> Result<()> {
    let json_bytes = std::fs::read(card_json)
        .with_context(|| format!("reading card JSON {}", card_json.display()))?;
    let card: Card = serde_json::from_slice(&json_bytes)
        .with_context(|| format!("parsing card JSON {}", card_json.display()))?;

    let opts = RenderOptions { dpi: Dpi(dpi), fonts_dir, symbols_dir };
    let png = render_png(&card, art, &opts).context("rendering card")?;

    std::fs::write(out, &png)
        .with_context(|| format!("writing PNG {}", out.display()))?;

    println!(
        "rendered {:?} → {} ({} bytes, {} dpi)",
        card.name,
        out.display(),
        png.len(),
        dpi
    );
    Ok(())
}

fn cmd_pull(set_code: &str, out: Option<PathBuf>, with_art: bool) -> Result<()> {
    let out_dir = match out {
        Some(p) => p,
        None => {
            let home = std::env::var("HOME")
                .map_err(|_| anyhow!("HOME not set; pass --out explicitly"))?;
            PathBuf::from(home).join("cards").join(set_code.to_uppercase())
        }
    };

    let client = Client::new();
    let opts = PullOptions {
        set_code,
        out_dir: out_dir.clone(),
        with_art,
        on_progress: None,
    };
    let stats = pull_set(&client, opts).context("pulling Scryfall set")?;

    println!(
        "set {} archived to {} \u{2014} {} cards ({} new JSON, {} new art, {} skipped)",
        set_code.to_uppercase(),
        out_dir.display(),
        stats.cards,
        stats.json_written,
        stats.art_written,
        stats.skipped_existing,
    );
    Ok(())
}
