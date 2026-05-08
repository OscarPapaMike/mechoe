//! Font loading.
//!
//! Three Windsor weights:
//!   * Roman — title (large) and P/T (large)
//!   * Demi  — type line
//!   * Light — rules-text body
//!
//! Matched by family/style via the system font manager (macOS Core Text picks
//! up `~/Library/Fonts/`), or loaded from a `--fonts-dir` if the filenames are
//! present there. Falls back to generic system faces if Windsor isn't
//! installed.

use std::path::{Path, PathBuf};

use skia_safe::{
    font_style::{Slant, Weight, Width},
    FontMgr, FontStyle, Typeface,
};

#[allow(dead_code)]
const CONDENSED: Width = Width::CONDENSED;

#[derive(Clone)]
pub struct Fonts {
    /// Windsor Roman — title and P/T.
    pub roman: Typeface,
    /// Windsor Demi — type line.
    pub demi: Typeface,
    /// Windsor Light — rules text.
    pub light: Typeface,
    /// Windsor Light Condensed BT — flavor text (rendered with a fake italic
    /// skew, since this face has no italic variant).
    pub flavor: Typeface,
    /// Windsor Heavy / Bold — generic mana numeral SVG generation.
    pub heavy: Typeface,
    /// Google Code Sans (or Courier fallback) — bottom-left info stamp.
    pub code: Typeface,
    /// Noto Sans Symbols 2 — set symbol glyph on the type line.
    pub symbols2: Option<Typeface>,
}

impl Fonts {
    pub fn load(assets_dir: Option<&Path>) -> Self {
        let mgr = FontMgr::new();
        let regular = FontStyle::new(Weight::NORMAL,    Width::NORMAL, Slant::Upright);
        let demi    = FontStyle::new(Weight::SEMI_BOLD, Width::NORMAL, Slant::Upright);
        let light   = FontStyle::new(Weight::LIGHT,     Width::NORMAL, Slant::Upright);
        let bold    = FontStyle::new(Weight::BOLD,      Width::NORMAL, Slant::Upright);

        let roman = mgr.match_family_style("Windsor BT", regular)
            .or_else(|| mgr.match_family_style("Windsor", regular))
            .or_else(|| try_load_named(assets_dir, "Windsor BT Roman.ttf", &mgr))
            .or_else(|| try_load_named(assets_dir, "WindsorBT.ttf", &mgr))
            .or_else(|| mgr.match_family_style("Helvetica Neue", bold))
            .or_else(|| mgr.match_family_style("", bold))
            .expect("no Roman/regular typeface available");

        let demi_face = mgr.match_family_style("Windsor", demi)
            .or_else(|| mgr.match_family_style("Windsor BT", demi))
            .or_else(|| try_load_named(assets_dir, "Windsor Demi.ttf", &mgr))
            .or_else(|| mgr.match_family_style("Windsor", bold))
            .or_else(|| mgr.match_family_style("", bold))
            .unwrap_or_else(|| roman.clone());

        let light_face = mgr.match_family_style("Windsor", light)
            .or_else(|| mgr.match_family_style("Windsor BT", light))
            .or_else(|| try_load_named(assets_dir, "Windsor Light BT.ttf", &mgr))
            .or_else(|| mgr.match_family_style("Times New Roman", regular))
            .or_else(|| mgr.match_family_style("", regular))
            .unwrap_or_else(|| roman.clone());

        let light_condensed = FontStyle::new(Weight::LIGHT, CONDENSED, Slant::Upright);
        let flavor_face = mgr.match_family_style("Windsor LtCn BT", regular)
            .or_else(|| mgr.match_family_style("Windsor", light_condensed))
            .or_else(|| try_load_named(assets_dir, "Windsor Light Condensed BT.ttf", &mgr))
            .unwrap_or_else(|| light_face.clone());

        let extra_bold = FontStyle::new(Weight::EXTRA_BOLD, Width::NORMAL, Slant::Upright);
        let heavy_face = mgr.match_family_style("Windsor BT", extra_bold)
            .or_else(|| mgr.match_family_style("Windsor", extra_bold))
            .or_else(|| mgr.match_family_style("Windsor BT", bold))
            .or_else(|| mgr.match_family_style("Windsor", bold))
            .or_else(|| try_load_named(assets_dir, "Windsor Heavy BT.ttf", &mgr))
            .or_else(|| try_load_named(assets_dir, "Windsor Bold.ttf", &mgr))
            .unwrap_or_else(|| demi_face.clone());

        let code_face = mgr.match_family_style("Google Code Sans", regular)
            .or_else(|| try_load_named(assets_dir, "GoogleCodeSans-Regular.ttf", &mgr))
            .or_else(|| try_load_named(assets_dir, "GoogleCodeSans.ttf", &mgr))
            .or_else(|| mgr.match_family_style("Courier New", regular))
            .or_else(|| mgr.match_family_style("Courier", regular))
            .or_else(|| mgr.match_family_style("Monaco", regular))
            .unwrap_or_else(|| roman.clone());

        let symbols2 = mgr.match_family_style("Noto Sans Symbols 2", regular)
            .or_else(|| try_load_named(assets_dir, "NotoSansSymbols2-Regular.ttf", &mgr));

        Self {
            roman,
            demi: demi_face,
            light: light_face,
            flavor: flavor_face,
            heavy: heavy_face,
            code: code_face,
            symbols2,
        }
    }
}

fn try_load_named(dir: Option<&Path>, name: &str, mgr: &FontMgr) -> Option<Typeface> {
    let path: PathBuf = dir?.join(name);
    let bytes = std::fs::read(&path).ok()?;
    mgr.new_from_data(&bytes, None)
}
