use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use mart_core::{render_png, symbol_gen, Card, CardStyle, Dpi, RenderOptions};
use mart_core::fonts::Fonts;

#[derive(ValueEnum, Clone, Debug, Default)]
enum StyleArg {
    #[default]
    Basic,
    Classic,
}

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
        /// Draw a bottom-left info stamp with this version string (e.g. "1").
        #[arg(long)]
        stamp_version: Option<String>,
        /// Card frame style.
        #[arg(long, value_enum, default_value_t = StyleArg::Basic)]
        style: StyleArg,
    },
    /// Generate Windsor Heavy SVG files for generic mana numerals (0–16, X).
    GenSymbols {
        /// Directory to write SVGs into. Defaults to data/_meta/symbols/.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Directory containing Windsor font files (optional).
        #[arg(long)]
        fonts_dir: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Render { card_json, art, out, dpi, fonts_dir, symbols_dir, stamp_version, style } => {
            cmd_render(&card_json, art.as_deref(), &out, dpi, fonts_dir, symbols_dir, stamp_version, style)
        }
        Command::GenSymbols { out_dir, fonts_dir } => {
            cmd_gen_symbols(out_dir, fonts_dir)
        }
    }
}

fn resolve_meta_subdir(name: &str) -> Option<PathBuf> {
    if let Ok(base) = std::env::var("MECHOE_DATA") {
        let p = PathBuf::from(base).join("_meta").join(name);
        if p.is_dir() { return Some(p); }
    }
    let p = PathBuf::from("data").join("_meta").join(name);
    if p.is_dir() { Some(p) } else { None }
}

fn resolve_symbols_dir(explicit: Option<PathBuf>) -> Option<PathBuf> {
    explicit.or_else(|| resolve_meta_subdir("symbols"))
}

fn resolve_rails_dir() -> Option<PathBuf> {
    resolve_meta_subdir("rails")
}

fn cmd_gen_symbols(out_dir: Option<PathBuf>, fonts_dir: Option<PathBuf>) -> Result<()> {
    let out_dir = out_dir.unwrap_or_else(|| PathBuf::from("data/_meta/symbols"));
    let fonts = Fonts::load(fonts_dir.as_deref());
    let written = symbol_gen::generate_numeral_svgs(&fonts, &out_dir)
        .with_context(|| format!("generating SVGs in {}", out_dir.display()))?;
    println!("wrote {} SVG files to {}", written.len(), out_dir.display());
    for f in &written {
        println!("  {f}");
    }
    Ok(())
}

fn cmd_render(
    card_json: &Path,
    art: Option<&Path>,
    out: &Path,
    dpi: f32,
    fonts_dir: Option<PathBuf>,
    symbols_dir: Option<PathBuf>,
    stamp_version: Option<String>,
    style: StyleArg,
) -> Result<()> {
    let json_bytes = std::fs::read(card_json)
        .with_context(|| format!("reading card JSON {}", card_json.display()))?;
    let card: Card = serde_json::from_slice(&json_bytes)
        .with_context(|| format!("parsing card JSON {}", card_json.display()))?;

    let opts = RenderOptions {
        dpi: Dpi(dpi),
        fonts_dir,
        symbols_dir: resolve_symbols_dir(symbols_dir),
        rails_dir: resolve_rails_dir(),
        stamp_version,
        card_style: match style {
            StyleArg::Basic   => CardStyle::Basic,
            StyleArg::Classic => CardStyle::Classic,
        },
    };
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

