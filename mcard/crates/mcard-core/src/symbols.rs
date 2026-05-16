//! Mana cost parsing + Scryfall SVG rasterization.
//!
//! For now, SVGs are loaded from a directory passed via `--symbols-dir`. The
//! filename for a given symbol is the symbol code with `{`, `}`, `/` stripped:
//! `{R}` → `R.svg`, `{W/U}` → `WU.svg`, `{2/W}` → `2W.svg`. This matches the
//! basename used by Scryfall's CDN, so the cache directory and the upstream
//! URLs map 1-to-1.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{self, Tree};
use skia_safe::{images, AlphaType, ColorType, Data, Image, ImageInfo};

#[derive(Debug, thiserror::Error)]
pub enum SymbolError {
    #[error("symbols directory not provided; pass --symbols-dir")]
    NoDir,
    #[error("symbol not found: {0}")]
    NotFound(String),
    #[error("svg parse failed for {0}: {1}")]
    Parse(String, String),
    #[error("rasterization failed for {0}")]
    Raster(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Tokenize a Scryfall mana_cost string. `"{2}{W/U}"` → `["2", "W/U"]`.
pub fn parse_mana_cost(mana_cost: &str) -> Vec<String> {
    parse_mana_cost_inline(mana_cost)
        .into_iter()
        .filter_map(|s| if let InlineSpan::Symbol(t) = s { Some(t) } else { None })
        .collect()
}

#[derive(Debug, Clone)]
pub enum InlineSpan {
    Text(String),
    Symbol(String),
}

/// Tokenize a string that mixes body text with `{...}` symbol tokens, preserving
/// order. Used for rules-text layout where symbols appear inline.
pub fn parse_mana_cost_inline(s: &str) -> Vec<InlineSpan> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut sym  = String::new();
    let mut in_sym = false;
    for c in s.chars() {
        match c {
            '{' => {
                if !text.is_empty() { out.push(InlineSpan::Text(std::mem::take(&mut text))); }
                in_sym = true; sym.clear();
            }
            '}' if in_sym => {
                out.push(InlineSpan::Symbol(std::mem::take(&mut sym)));
                in_sym = false;
            }
            _ if in_sym => sym.push(c),
            _ => text.push(c),
        }
    }
    if !text.is_empty() { out.push(InlineSpan::Text(text)); }
    out
}

fn token_to_filename(token: &str) -> String {
    token.replace('/', "")
}

/// Loaded SVG trees, keyed by token (e.g. "R", "W/U"). Cached after first load.
pub struct SymbolCache {
    dir:   PathBuf,
    trees: HashMap<String, Tree>,
}

impl SymbolCache {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, trees: HashMap::new() }
    }

    fn load_tree(dir: &Path, token: &str) -> Result<Tree, SymbolError> {
        let filename = format!("{}.svg", token_to_filename(token));
        let path = dir.join(&filename);
        let bytes = std::fs::read(&path)
            .map_err(|_| SymbolError::NotFound(token.to_string()))?;
        let opt = usvg::Options::default();
        Tree::from_data(&bytes, &opt)
            .map_err(|e| SymbolError::Parse(token.to_string(), e.to_string()))
    }

    /// Rasterize `token` to an `Image` whose width is `target_w_px`. Aspect
    /// ratio is preserved.
    pub fn rasterize(&mut self, token: &str, target_w_px: f32) -> Result<Image, SymbolError> {
        if !self.trees.contains_key(token) {
            let tree = Self::load_tree(&self.dir, token)?;
            self.trees.insert(token.to_string(), tree);
        }
        let tree = &self.trees[token];

        let svg_size = tree.size();
        let scale = target_w_px / svg_size.width();
        let target_h_px = (svg_size.height() * scale).ceil() as u32;
        let target_w = target_w_px.ceil() as u32;
        let mut pixmap = Pixmap::new(target_w, target_h_px)
            .ok_or_else(|| SymbolError::Raster(token.to_string()))?;
        resvg::render(tree, Transform::from_scale(scale, scale), &mut pixmap.as_mut());

        // tiny_skia pixmaps store premultiplied alpha — declare Premul so Skia
        // composites correctly and avoids dark-fringe artifacts at circle edges.
        let info = ImageInfo::new(
            (target_w as i32, target_h_px as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );
        let row_bytes = (target_w * 4) as usize;
        let data = Data::new_copy(pixmap.data());
        images::raster_from_data(&info, data, row_bytes)
            .ok_or_else(|| SymbolError::Raster(token.to_string()))
    }
}
