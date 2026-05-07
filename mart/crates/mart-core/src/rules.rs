//! Rules-text layout: paragraphs of mixed body text + inline mana symbols,
//! greedy line break, font-size auto-fit to the rules box.

use skia_safe::{Canvas, Color, Font, Paint, Rect, Typeface};

use crate::symbols::{parse_mana_cost_inline, InlineSpan, SymbolCache};

#[derive(Debug, Clone)]
enum Atom {
    Word(String),
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
                    let mut iter = t.split(' ').peekable();
                    while let Some(word) = iter.next() {
                        if !word.is_empty() {
                            atoms.push(Atom::Word(word.to_string()));
                        }
                        if iter.peek().is_some() {
                            atoms.push(Atom::Space);
                        }
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
}

fn atom_width(
    atom: &Atom,
    font: &Font,
    symbol_w_px: f32,
    space_w_px: f32,
    plain_symbols: bool,
) -> f32 {
    match atom {
        Atom::Word(w)   => font.measure_str(w, None).0,
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

            let line_start = i;
            let mut line_w = 0.0_f32;
            let mut last_space_end = i;

            while i < atoms.len() {
                let w = atom_width(&atoms[i], font, symbol_w_px, space_w_px, plain_symbols);
                if line_w + w > inner.width() && i > line_start {
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
) -> f32 {
    let paragraphs = tokenize(oracle_text);
    if paragraphs.iter().all(|p| p.is_empty()) {
        return base_size_px;
    }

    // No internal padding: body text shares the type line's left edge and
    // sits flush with the top of the rules box.
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
        );
        (lay, font, symbol_w_px)
    };

    // Try the base size first; if it fits, use it.
    let (lay, font, sw) = try_size(base_size_px);
    if lay.total_height_px <= inner.height() {
        draw_layout(canvas, &paragraphs, &lay, &font, cache.as_deref_mut(), sw);
        return base_size_px;
    }

    // Binary-search for the largest size that fits.
    let mut lo = min_size_px;
    let mut hi = base_size_px;
    let mut best: Option<(Layout, Font, f32, f32)> = None; // (lay, font, sw, size)
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
    draw_layout(canvas, &paragraphs, &lay, &font, cache.as_deref_mut(), sw);
    size
}

fn draw_layout(
    canvas: &Canvas,
    paragraphs: &[Vec<Atom>],
    layout: &Layout,
    font: &Font,
    mut cache: Option<&mut SymbolCache>,
    symbol_w_px: f32,
) {
    let space_w_px = font.measure_str(" ", None).0.max(symbol_w_px * 0.25);

    let mut text_paint = Paint::default();
    text_paint.set_anti_alias(true);
    text_paint.set_color(Color::BLACK);

    let mut img_paint = Paint::default();
    img_paint.set_anti_alias(true);

    for line in &layout.lines {
        let atoms = &paragraphs[line.para_idx][line.start..line.end];
        let mut x = layout.line_left_x;
        for atom in atoms {
            match atom {
                Atom::Word(w) => {
                    canvas.draw_str(w, (x, line.baseline_y), font, &text_paint);
                    x += font.measure_str(w, None).0;
                }
                Atom::Space => {
                    x += space_w_px;
                }
                Atom::Symbol(token) => {
                    let drew = if let Some(cache) = cache.as_deref_mut() {
                        match cache.rasterize(token, symbol_w_px) {
                            Ok(image) => {
                                // Vertically place so most of the symbol sits
                                // above the baseline like a cap-height letter.
                                let top = line.baseline_y - symbol_w_px * 0.85;
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
                        // Premodern style: render the symbol token as a plain
                        // letter/number (no braces). `{T}` → `T`, `{W/U}` → `W/U`.
                        canvas.draw_str(token, (x, line.baseline_y), font, &text_paint);
                        x += font.measure_str(token, None).0;
                    } else {
                        x += symbol_w_px;
                    }
                }
            }
        }
    }
}
