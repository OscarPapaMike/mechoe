//! Generate SVG files for generic mana cost numerals (0–16, X) using Windsor Heavy.
//!
//! Each SVG is a 100×100 viewBox with a coloured circle and the numeral as a
//! pure vector `<path>` extracted from Skia's glyph outlines.  No PNG is
//! embedded, so there are no premultiplied-alpha edge artefacts when resvg
//! composites the symbol over the white disc in draw_mana_cost.

use std::path::Path;

use skia_safe::{Font, path::{Iter as PathIter, Verb}};

use crate::fonts::Fonts;

/// Generic mana background: light warm grey.
const BG_R: u8 = 220;
const BG_G: u8 = 216;
const BG_B: u8 = 212;

/// Circle radius in the 100×100 viewBox — matches Scryfall colored pip radius.
const CIRCLE_R: f32 = 50.0;

/// Fixed gap (in viewBox units) between the glyph ink edge and the circle
/// boundary on the tightest axis.  Changing this one value rescales all glyphs.
const GLYPH_MARGIN: f32 = 9.0;

/// Tokens generated: 0–16, X, and T (custom tap symbol).
pub const TOKENS: &[&str] = &[
    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
    "X", "T",
];

#[derive(Debug, thiserror::Error)]
pub enum GenError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Generate SVG files for all generic mana tokens into `out_dir`.
pub fn generate_numeral_svgs(fonts: &Fonts, out_dir: &Path) -> Result<Vec<String>, GenError> {
    std::fs::create_dir_all(out_dir)?;
    let mut written = Vec::new();
    for &token in TOKENS {
        let filename = format!("{token}.svg");
        let path = out_dir.join(&filename);
        let svg = build_svg(fonts, token);
        std::fs::write(&path, svg.as_bytes())?;
        written.push(filename);
    }
    Ok(written)
}

fn build_svg(fonts: &Fonts, label: &str) -> String {
    let size_f = 100.0_f32;

    // Internal font size for glyph extraction.  Scale is applied via math
    // below; larger gives better floating-point precision in the SVG path.
    let extract_size = if label.len() > 1 { 60.0_f32 } else { 90.0_f32 };
    let font = Font::from_typeface(fonts.heavy.clone(), extract_size);

    // Ink bounding box for centering.
    let (_, ink) = font.measure_str(label, None);

    // Scale so the tightest axis has exactly GLYPH_MARGIN units between the
    // ink edge and the circle boundary.  usable = 2*(CIRCLE_R - GLYPH_MARGIN).
    let usable = 2.0 * (CIRCLE_R - GLYPH_MARGIN);
    let scale = (usable / ink.width()).min(usable / ink.height());

    // Translate: map ink centre to (50, 50).
    let ink_cx = (ink.left + ink.right) * 0.5;
    let ink_cy = (ink.top + ink.bottom) * 0.5;
    let tx = size_f * 0.5 - ink_cx * scale;
    let ty = size_f * 0.5 - ink_cy * scale;

    // Glyph IDs for the label (use the vec variant to avoid sizing guess-work).
    let glyph_ids = font.str_to_glyphs_vec(label);

    // Per-glyph advances.
    let mut widths = vec![0.0f32; glyph_ids.len()];
    font.get_widths(&glyph_ids, &mut widths);

    // Build the SVG `d` string from glyph outlines.
    let mut d = String::new();
    let mut x_off = 0.0f32;
    for (&glyph_id, &adv) in glyph_ids.iter().zip(widths.iter()) {
        if let Some(glyph_path) = font.get_path(glyph_id) {
            append_path(&mut d, &glyph_path, x_off, scale, tx, ty);
        }
        x_off += adv;
    }

    let bg = format!("#{:02x}{:02x}{:02x}", BG_R, BG_G, BG_B);
    let r = CIRCLE_R as u32;

    if label == "T" {
        // Tap symbol: Windsor Heavy "T" rotated 40° clockwise around the circle centre.
        // A clipPath trims the corners that extend past the circle after rotation.
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\" width=\"100\" height=\"100\">\n  \
             <clipPath id=\"c\"><circle cx=\"50\" cy=\"50\" r=\"{r}\"/></clipPath>\n  \
             <circle cx=\"50\" cy=\"50\" r=\"{r}\" fill=\"{bg}\"/>\n  \
             <path d=\"{d}\" fill=\"#000000\" transform=\"rotate(40 50 50)\" clip-path=\"url(#c)\"/>\n\
             </svg>\n"
        )
    } else {
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\" width=\"100\" height=\"100\">\n  \
             <circle cx=\"50\" cy=\"50\" r=\"{r}\" fill=\"{bg}\"/>\n  \
             <path d=\"{d}\" fill=\"#000000\"/>\n\
             </svg>\n"
        )
    }
}

/// Serialize one glyph path into the running SVG `d` string, applying:
///   final_x = raw_x * scale + x_off * scale + tx
///   final_y = raw_y * scale + ty
fn append_path(d: &mut String, path: &skia_safe::Path, x_off: f32, scale: f32, tx: f32, ty: f32) {
    let px = |raw: f32| -> f32 { raw * scale + x_off * scale + tx };
    let py = |raw: f32| -> f32 { raw * scale + ty };

    for (verb, pts) in PathIter::new(path, false) {
        match verb {
            Verb::Move => {
                d.push_str(&format!("M{:.2} {:.2}", px(pts[0].x), py(pts[0].y)));
            }
            Verb::Line => {
                d.push_str(&format!("L{:.2} {:.2}", px(pts[1].x), py(pts[1].y)));
            }
            Verb::Quad => {
                d.push_str(&format!(
                    "Q{:.2} {:.2} {:.2} {:.2}",
                    px(pts[1].x), py(pts[1].y),
                    px(pts[2].x), py(pts[2].y),
                ));
            }
            Verb::Conic => {
                // Approximate conic as quadratic — visually indistinguishable at this size.
                d.push_str(&format!(
                    "Q{:.2} {:.2} {:.2} {:.2}",
                    px(pts[1].x), py(pts[1].y),
                    px(pts[2].x), py(pts[2].y),
                ));
            }
            Verb::Cubic => {
                d.push_str(&format!(
                    "C{:.2} {:.2} {:.2} {:.2} {:.2} {:.2}",
                    px(pts[1].x), py(pts[1].y),
                    px(pts[2].x), py(pts[2].y),
                    px(pts[3].x), py(pts[3].y),
                ));
            }
            Verb::Close | Verb::Done => {
                d.push('Z');
            }
        }
    }
}
