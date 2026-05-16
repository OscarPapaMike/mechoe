//! Rules-text layout: paragraphs of mixed body text + inline mana symbols,
//! greedy line break, font-size auto-fit to the rules box.

use skia_safe::{Canvas, ClipOp, Color, Font, Paint, Rect, Typeface};

use crate::symbols::{parse_mana_cost_inline, InlineSpan, SymbolCache};

#[derive(Debug, Clone)]
enum Atom {
    Word(String),
    /// Word rendered upright (no skew) — used for *title* spans in flavor text.
    Upright(String),
    Symbol(String),
    Space,
}

fn tokenize(oracle: &str) -> Vec<Vec<Atom>> {
    let mut paragraphs = Vec::new();
    for para in oracle.split('\n') {
        let mut atoms = Vec::new();
        for span in parse_mana_cost_inline(para) {
            match span {
                InlineSpan::Symbol(s) => atoms.push(Atom::Symbol(s)),
                InlineSpan::Text(t) => {
                    let mut upright = false;
                    for segment in t.split('*') {
                        let mut iter = segment.split(' ').peekable();
                        while let Some(word) = iter.next() {
                            if !word.is_empty() {
                                if upright {
                                    atoms.push(Atom::Upright(word.to_string()));
                                } else {
                                    atoms.push(Atom::Word(word.to_string()));
                                }
                            }
                            if iter.peek().is_some() {
                                atoms.push(Atom::Space);
                            }
                        }
                        upright = !upright;
                    }
                }
            }
        }
        paragraphs.push(atoms);
    }
    paragraphs
}

struct Line {
    para_idx: usize,
    start: usize,
    end: usize,        // exclusive
    baseline_y: f32,
}

struct Layout {
    lines: Vec<Line>,
    total_height_px: f32,
    line_left_x: f32,
    line_right_x: f32,
    notch: Option<(f32, f32)>,  // (notch_top_y, notch_right_x)
}

fn atom_width(
    atom: &Atom,
    font: &Font,
    symbol_w_px: f32,
    space_w_px: f32,
    plain_symbols: bool,
) -> f32 {
    match atom {
        Atom::Word(w) | Atom::Upright(w) => font.measure_str(w, None).0,
        Atom::Symbol(t) => {
            if plain_symbols {
                font.measure_str(t, None).0
            } else {
                symbol_w_px
            }
        }
        Atom::Space => space_w_px,
    }
}

fn layout(
    paragraphs: &[Vec<Atom>],
    font: &Font,
    symbol_w_px: f32,
    line_height_px: f32,
    paragraph_gap_px: f32,
    inner: Rect,
    plain_symbols: bool,
    // (notch_top_y, notch_right_x): lines whose top is at or below this y
    // are constrained to notch_right_x on the right instead of inner.right.
    notch: Option<(f32, f32)>,
) -> Layout {
    let space_w_px = font.measure_str(" ", None).0.max(symbol_w_px * 0.25);
    let (_, metrics) = font.metrics();
    let baseline_offset = -metrics.ascent;

    let mut lines = Vec::new();
    let mut y = inner.top;

    for (pi, atoms) in paragraphs.iter().enumerate() {
        if atoms.is_empty() {
            y += paragraph_gap_px;
            continue;
        }
        let mut i = 0;
        while i < atoms.len() {
            while i < atoms.len() && matches!(atoms[i], Atom::Space) { i += 1; }
            if i >= atoms.len() { break; }

            let right_x = match notch {
                Some((notch_top, notch_right)) if y + baseline_offset >= notch_top => notch_right,
                _ => inner.right,
            };
            let avail_w = right_x - inner.left;

            let line_start = i;
            let mut line_w = 0.0_f32;
            let mut last_space_end = i;

            while i < atoms.len() {
                let w = atom_width(&atoms[i], font, symbol_w_px, space_w_px, plain_symbols);
                if line_w + w > avail_w && i > line_start {
                    break;
                }
                line_w += w;
                i += 1;
                if matches!(atoms.get(i.saturating_sub(1)), Some(Atom::Space)) {
                    last_space_end = i;
                }
            }
            let line_end = if i < atoms.len() && last_space_end > line_start {
                last_space_end
            } else {
                i
            };
            lines.push(Line {
                para_idx: pi,
                start: line_start,
                end: line_end,
                baseline_y: y + baseline_offset,
            });
            y += line_height_px;
            i = line_end;
        }
        if pi + 1 < paragraphs.len() {
            y += paragraph_gap_px;
        }
    }

    Layout {
        lines,
        total_height_px: y - inner.top,
        line_left_x: inner.left,
        line_right_x: inner.right,
        notch,
    }
}

pub fn draw_rules(
    canvas: &Canvas,
    body_face: &Typeface,
    mut cache: Option<&mut SymbolCache>,
    oracle_text: &str,
    rect_px: Rect,
    base_size_px: f32,
    min_size_px: f32,
    font_skew_x: f32,
    text_color: Color,
    center_max_lines: Option<usize>,
    // (notch_top_y_px, notch_right_x_px): lines whose top falls at or below
    // notch_top_y are wrapped to notch_right_x instead of rect_px.right.
    notch_px: Option<(f32, f32)>,
    // Optional drop shadow: (color, dx, dy) in pixels.
    shadow: Option<(Color, f32, f32)>,
    // Override the height used for vertical centering. When None, rect_px.height() is used.
    // Pass the type-line-to-PT-box span for creature cards so short text centers in
    // the visible creature text zone rather than just the rules rect.
    center_height_px: Option<f32>,
) -> f32 {
    let paragraphs = tokenize(oracle_text);
    if paragraphs.iter().all(|p| p.is_empty()) {
        return base_size_px;
    }

    let inner = rect_px;

    let plain_symbols = cache.is_none();
    let try_size = |size_px: f32| -> (Layout, Font, f32) {
        let mut font = Font::from_typeface(body_face.clone(), size_px);
        font.set_subpixel(true);
        if font_skew_x != 0.0 {
            font.set_skew_x(font_skew_x);
        }
        let symbol_w_px = size_px * 0.95;
        let line_height_px = size_px * 1.18;
        let paragraph_gap_px = size_px * 0.55;
        let lay = layout(
            &paragraphs,
            &font,
            symbol_w_px,
            line_height_px,
            paragraph_gap_px,
            inner,
            plain_symbols,
            notch_px,
        );
        (lay, font, symbol_w_px)
    };

    let center_check = |lay: &Layout| -> (f32, bool) {
        let do_center = center_max_lines.map_or(false, |max| lay.lines.len() <= max);
        let y_off = if do_center {
            let h = center_height_px.unwrap_or_else(|| inner.height());
            (h - lay.total_height_px) * 0.5
        } else {
            0.0
        };
        (y_off, do_center)
    };

    let make_upright_font = |_font: &Font, size_px: f32| -> Option<Font> {
        if font_skew_x == 0.0 {
            return None;
        }
        let mut uf = Font::from_typeface(body_face.clone(), size_px);
        uf.set_subpixel(true);
        Some(uf)
    };

    // Try the base size first; if it fits, use it.
    let (lay, font, sw) = try_size(base_size_px);
    if lay.total_height_px <= inner.height() {
        let upright = make_upright_font(&font, base_size_px);
        let (y_off, center_h) = center_check(&lay);
        canvas.save();
        canvas.clip_rect(rect_px, ClipOp::Intersect, true);
        draw_layout(canvas, &paragraphs, &lay, &font, upright.as_ref(), cache.as_deref_mut(), sw, text_color, y_off, center_h, shadow);
        canvas.restore();
        return base_size_px;
    }

    // Binary-search for the largest size that fits.
    let mut lo = min_size_px;
    let mut hi = base_size_px;
    let mut best: Option<(Layout, Font, f32, f32)> = None;
    for _ in 0..7 {
        let mid = (lo + hi) * 0.5;
        let (lay, font, sw) = try_size(mid);
        if lay.total_height_px <= inner.height() {
            lo = mid;
            best = Some((lay, font, sw, mid));
        } else {
            hi = mid;
        }
    }

    let (lay, font, sw, size) = best.unwrap_or_else(|| {
        let (lay, font, sw) = try_size(min_size_px);
        (lay, font, sw, min_size_px)
    });
    let upright = make_upright_font(&font, size);
    let (y_off, center_h) = center_check(&lay);
    canvas.save();
    canvas.clip_rect(rect_px, ClipOp::Intersect, true);
    draw_layout(canvas, &paragraphs, &lay, &font, upright.as_ref(), cache.as_deref_mut(), sw, text_color, y_off, center_h, shadow);
    canvas.restore();
    size
}

fn draw_layout(
    canvas: &Canvas,
    paragraphs: &[Vec<Atom>],
    layout: &Layout,
    font: &Font,
    upright_font: Option<&Font>,
    mut cache: Option<&mut SymbolCache>,
    symbol_w_px: f32,
    text_color: Color,
    y_offset: f32,
    center_h: bool,
    shadow: Option<(Color, f32, f32)>,
) {
    let default_space_w = font.measure_str(" ", None).0.max(symbol_w_px * 0.25);
    let plain_symbols = cache.is_none();

    let mut text_paint = Paint::default();
    text_paint.set_anti_alias(true);
    text_paint.set_color(text_color);

    let mut shadow_paint = shadow.map(|(color, _, _)| {
        let mut p = Paint::default();
        p.set_anti_alias(true);
        p.set_color(color);
        p
    });

    let mut img_paint = Paint::default();
    img_paint.set_anti_alias(true);

    for line in &layout.lines {
        let atoms = &paragraphs[line.para_idx][line.start..line.end];
        let baseline_y = line.baseline_y + y_offset;

        let right_x = match layout.notch {
            Some((notch_top, notch_right)) if line.baseline_y >= notch_top => notch_right,
            _ => layout.line_right_x,
        };
        let line_w = right_x - layout.line_left_x;

        // When centering, each line starts from its natural center — no justification.
        let natural_w = |atoms: &[Atom]| -> f32 {
            atoms.iter().map(|a| match a {
                Atom::Word(w)    => font.measure_str(w.as_str(), None).0,
                Atom::Upright(w) => upright_font.unwrap_or(font).measure_str(w.as_str(), None).0,
                Atom::Symbol(t)  => if plain_symbols { font.measure_str(t.as_str(), None).0 } else { symbol_w_px },
                Atom::Space      => default_space_w,
            }).sum()
        };

        let space_w = if center_h {
            default_space_w
        } else {
            // Justify all lines except the last; skip if natural fill < 60%.
            let is_last = line.end >= paragraphs[line.para_idx].len();
            if !is_last {
                let n_spaces = atoms.iter().filter(|a| matches!(a, Atom::Space)).count();
                if n_spaces > 0 {
                    let word_w: f32 = atoms.iter().map(|a| match a {
                        Atom::Word(w)    => font.measure_str(w.as_str(), None).0,
                        Atom::Upright(w) => upright_font.unwrap_or(font).measure_str(w.as_str(), None).0,
                        Atom::Symbol(t)  => if plain_symbols { font.measure_str(t.as_str(), None).0 } else { symbol_w_px },
                        Atom::Space      => 0.0,
                    }).sum();
                    if word_w / line_w >= 0.60 {
                        (line_w - word_w) / n_spaces as f32
                    } else {
                        default_space_w
                    }
                } else {
                    default_space_w
                }
            } else {
                default_space_w
            }
        };

        let mut x = if center_h {
            layout.line_left_x + (line_w - natural_w(atoms)) * 0.5
        } else {
            layout.line_left_x
        };
        for atom in atoms {
            match atom {
                Atom::Word(w) => {
                    if let (Some((_, dx, dy)), Some(sp)) = (shadow, shadow_paint.as_ref()) {
                        canvas.draw_str(w, (x + dx, baseline_y + dy), font, sp);
                    }
                    canvas.draw_str(w, (x, baseline_y), font, &text_paint);
                    x += font.measure_str(w, None).0;
                }
                Atom::Upright(w) => {
                    let uf = upright_font.unwrap_or(font);
                    if let (Some((_, dx, dy)), Some(sp)) = (shadow, shadow_paint.as_ref()) {
                        canvas.draw_str(w, (x + dx, baseline_y + dy), uf, sp);
                    }
                    canvas.draw_str(w, (x, baseline_y), uf, &text_paint);
                    x += uf.measure_str(w, None).0;
                }
                Atom::Space => {
                    x += space_w;
                }
                Atom::Symbol(token) => {
                    let drew = if let Some(cache) = cache.as_deref_mut() {
                        match cache.rasterize(token, symbol_w_px) {
                            Ok(image) => {
                                let top = baseline_y - symbol_w_px * 0.85;
                                let dst = Rect::from_xywh(x, top, symbol_w_px, symbol_w_px);
                                canvas.draw_image_rect(&image, None, dst, &img_paint);
                                true
                            }
                            Err(e) => {
                                eprintln!("warning: skipping inline {{{token}}}: {e}");
                                false
                            }
                        }
                    } else {
                        false
                    };
                    if !drew {
                        canvas.draw_str(token, (x, baseline_y), font, &text_paint);
                        x += font.measure_str(token, None).0;
                    } else {
                        x += symbol_w_px;
                    }
                }
            }
        }
    }
}
