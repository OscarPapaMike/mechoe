//! 1993/premodern-style renderer with main+alt color scheme.

use std::path::{Path, PathBuf};

use skia_safe::{
    surfaces, Color, Color4f, Data, EncodedImageFormat, Image, Paint, Path as SkPath, RRect,
    Vector,
};

use crate::fonts::Fonts;
use crate::frame::FrameSpec;
use crate::geometry::{card_size_px, Dpi, MmRect};
use crate::rules::draw_rules;
use crate::scryfall::Card;
use crate::symbols::{parse_mana_cost, SymbolCache};
use crate::text::{draw_in_rect, HAlign, TextStyle};

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("failed to read art file: {0}")]
    ArtRead(#[from] std::io::Error),
    #[error("could not decode artwork as PNG/JPEG")]
    ArtDecode,
    #[error("skia surface allocation failed")]
    SurfaceAlloc,
    #[error("PNG encoding failed")]
    PngEncode,
}

#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub dpi: Dpi,
    pub fonts_dir: Option<PathBuf>,
    pub symbols_dir: Option<PathBuf>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self { dpi: Dpi::default(), fonts_dir: None, symbols_dir: None }
    }
}

pub fn render_png(
    card: &Card,
    art_path: Option<&Path>,
    opts: &RenderOptions,
) -> Result<Vec<u8>, RenderError> {
    let dpi = opts.dpi;
    let (w_px, h_px) = card_size_px(dpi);

    let frame = FrameSpec::premodern(card.frame_color());
    let fonts = Fonts::load(opts.fonts_dir.as_deref());
    let mut symbol_cache = opts.symbols_dir.as_ref().map(|d| SymbolCache::new(d.clone()));

    let mut surface =
        surfaces::raster_n32_premul((w_px, h_px)).ok_or(RenderError::SurfaceAlloc)?;
    let canvas = surface.canvas();

    // 1. Outer black border (rounded card stock).
    canvas.clear(Color::TRANSPARENT);
    let outer_rect_px = frame.outer_rect().to_skia(dpi);
    let radius = frame.corner_radius_mm * dpi.px_per_mm();
    let rrect = RRect::new_rect_radii(outer_rect_px, &[Vector::new(radius, radius); 4]);
    let mut black = Paint::default();
    black.set_anti_alias(true);
    black.set_color(Color::BLACK);
    canvas.draw_rrect(rrect, &black);

    // 2. Inner area filled with ALT color — becomes the bottom-half background.
    let mut alt_paint = Paint::new(frame.alt_color(), None);
    alt_paint.set_anti_alias(true);
    canvas.draw_rect(frame.inner_rect().to_skia(dpi), &alt_paint);

    // 3. Title bar painted in MAIN color.
    let mut main_paint = Paint::new(frame.main_color(), None);
    main_paint.set_anti_alias(true);
    canvas.draw_rect(frame.title_bar.to_skia(dpi), &main_paint);

    // 4. Art (covers the middle band between title and alt area).
    let art_rect_px = frame.art_box.to_skia(dpi);
    if let Some(path) = art_path {
        draw_art(canvas, path, art_rect_px)?;
    } else {
        let mut placeholder = Paint::default();
        placeholder.set_anti_alias(true);
        placeholder.set_color4f(Color4f::new(0.12, 0.12, 0.13, 1.0), None);
        canvas.draw_rect(art_rect_px, &placeholder);
    }

    // 5. Mana cost — drawn in title bar with a white disc behind each symbol
    // so it reads cleanly on any main color. Returns left edge in px so the
    // title text knows how much room to leave.
    let mana_left_px = if let (Some(cost), Some(cache)) =
        (card.mana_cost.as_deref(), symbol_cache.as_mut())
    {
        draw_mana_cost(canvas, cache, &frame, dpi, cost)
    } else {
        None
    };

    // 6. Title text — alt color on the main-color title bar, Windsor Roman.
    // Shifted halfway up (valign_frac=0.30) and halfway in toward the left
    // edge (1.25 mm pad). Mana cost is priority: leave a 3 mm gap and let
    // the title compress horizontally (scale_x) to fit.
    let title_padded = pad_horizontal(frame.title_bar, 1.25).to_skia(dpi);
    let title_fit_w = match mana_left_px {
        Some(left_px) => (left_px - title_padded.left - mm_to_px(3.0, dpi)).max(0.0),
        None => title_padded.width(),
    };
    let alt_color_int = color4f_to_color(frame.alt_color());
    draw_in_rect(
        canvas,
        &fonts.roman,
        &card.name,
        title_padded,
        TextStyle::new(mm_to_px(3.84, dpi))
            .with_color(alt_color_int)
            .with_halign(HAlign::Left)
            .with_fit(title_fit_w)
            .with_valign_frac(0.30),
    );

    // 7. Type line — main color, Windsor Demi, old "Summon X" naming. No
    // extra horizontal padding so it shares the left edge of the body text.
    let type_text = card.old_type_line();
    let type_padded = frame.type_bar.to_skia(dpi);
    let set_symbol_w_mm = 4.5;
    let set_symbol_right_pad_mm = 1.0;
    let type_fit_w = (type_padded.width()
        - mm_to_px(set_symbol_w_mm + set_symbol_right_pad_mm + 1.0, dpi))
        .max(0.0);
    let main_color_int = color4f_to_color(frame.main_color());
    draw_in_rect(
        canvas,
        &fonts.demi,
        &type_text,
        type_padded,
        TextStyle::new(mm_to_px(3.6, dpi))
            .with_color(main_color_int)
            .with_halign(HAlign::Left)
            .with_fit(type_fit_w),
    );

    // 8. Set symbol placeholder on the right of the type-line band.
    draw_set_symbol_placeholder(
        canvas,
        type_padded,
        mm_to_px(set_symbol_w_mm, dpi),
        mm_to_px(set_symbol_right_pad_mm, dpi),
        frame.main_color(),
    );

    // 9. Rules text + flavor text. Both are black on the alt-color
    // background; flavor uses Windsor Light Condensed BT with a fake-italic
    // skew (the face has no italic variant).
    let rules_rect_px = frame.rules_box.to_skia(dpi);
    let oracle = card.oracle_text.as_deref().filter(|s| !s.trim().is_empty());
    let flavor = card.flavor_text.as_deref().filter(|s| !s.trim().is_empty());
    let (oracle_rect, flavor_rect) = match (oracle.is_some(), flavor.is_some()) {
        (true, true) => {
            let split = rules_rect_px.top + rules_rect_px.height() * 0.60;
            let gap = mm_to_px(0.8, dpi);
            (
                Some(skia_safe::Rect::new(
                    rules_rect_px.left,
                    rules_rect_px.top,
                    rules_rect_px.right,
                    split,
                )),
                Some(skia_safe::Rect::new(
                    rules_rect_px.left,
                    split + gap,
                    rules_rect_px.right,
                    rules_rect_px.bottom,
                )),
            )
        }
        (true, false) => (Some(rules_rect_px), None),
        (false, true) => (None, Some(rules_rect_px)),
        (false, false) => (None, None),
    };

    if let (Some(oracle), Some(rect)) = (oracle, oracle_rect) {
        draw_rules(
            canvas,
            &fonts.light,
            None,
            oracle,
            rect,
            /* base */ mm_to_px(3.0, dpi),
            /* min  */ mm_to_px(2.0, dpi),
            /* skew */ 0.0,
        );
    }
    if let (Some(flavor), Some(rect)) = (flavor, flavor_rect) {
        draw_rules(
            canvas,
            &fonts.flavor,
            None,
            flavor,
            rect,
            /* base */ mm_to_px(2.6, dpi),
            /* min  */ mm_to_px(1.7, dpi),
            /* skew */ -0.18,
        );
    }

    // 10. P/T — large black Windsor Roman, no tab background.
    if let (Some(p), Some(t)) = (&card.power, &card.toughness) {
        let pt_text = format!("{p} / {t}");
        let pt_rect_px = frame.pt_box.to_skia(dpi);
        draw_in_rect(
            canvas,
            &fonts.roman,
            &pt_text,
            pt_rect_px,
            TextStyle::new(mm_to_px(6.0, dpi))
                .with_color(Color::BLACK)
                .with_halign(HAlign::Right),
        );
    }

    // Encode PNG.
    let snapshot = surface.image_snapshot();
    let mut ctx = surface.direct_context();
    let png = snapshot
        .encode(ctx.as_mut(), EncodedImageFormat::PNG, None)
        .ok_or(RenderError::PngEncode)?;
    Ok(png.as_bytes().to_vec())
}

fn draw_mana_cost(
    canvas: &skia_safe::Canvas,
    cache: &mut SymbolCache,
    frame: &FrameSpec,
    dpi: Dpi,
    mana_cost: &str,
) -> Option<f32> {
    let tokens = parse_mana_cost(mana_cost);
    if tokens.is_empty() {
        return None;
    }

    let symbol_size_mm = 4.4;
    let gap_mm = 0.4;
    let right_pad_mm = 1.5;

    let symbol_size_px = symbol_size_mm * dpi.px_per_mm();
    let gap_px = gap_mm * dpi.px_per_mm();
    let right_pad_px = right_pad_mm * dpi.px_per_mm();

    let bar = frame.title_bar.to_skia(dpi);
    let total_w = tokens.len() as f32 * symbol_size_px
        + (tokens.len() as f32 - 1.0).max(0.0) * gap_px;
    let start_x = bar.right - right_pad_px - total_w;
    let y = bar.top + (bar.height() - symbol_size_px) * 0.5;

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    let mut white_bg = Paint::default();
    white_bg.set_anti_alias(true);
    white_bg.set_color(Color::WHITE);

    let mut x = start_x;
    for token in &tokens {
        let cx = x + symbol_size_px * 0.5;
        let cy = y + symbol_size_px * 0.5;
        // White disc behind the symbol, slightly larger than the symbol so
        // the colored glyph reads clearly on the dark title bar.
        canvas.draw_circle((cx, cy), symbol_size_px * 0.5, &white_bg);
        match cache.rasterize(token, symbol_size_px) {
            Ok(image) => {
                let dst = skia_safe::Rect::from_xywh(x, y, symbol_size_px, symbol_size_px);
                canvas.draw_image_rect(&image, None, dst, &paint);
            }
            Err(e) => eprintln!("warning: skipping mana symbol {{{token}}}: {e}"),
        }
        x += symbol_size_px + gap_px;
    }

    Some(start_x)
}

fn draw_set_symbol_placeholder(
    canvas: &skia_safe::Canvas,
    type_bar_px: skia_safe::Rect,
    size_px: f32,
    right_pad_px: f32,
    outline: Color4f,
) {
    let cx = type_bar_px.right - right_pad_px - size_px * 0.5;
    let cy = type_bar_px.top + type_bar_px.height() * 0.5;
    let r = size_px * 0.5;

    let mut path = SkPath::new();
    path.move_to((cx, cy - r));
    path.line_to((cx + r, cy));
    path.line_to((cx, cy + r));
    path.line_to((cx - r, cy));
    path.close();

    let mut fill = Paint::default();
    fill.set_anti_alias(true);
    fill.set_color(Color::WHITE);
    canvas.draw_path(&path, &fill);

    let mut stroke = Paint::new(outline, None);
    stroke.set_anti_alias(true);
    stroke.set_style(skia_safe::paint::Style::Stroke);
    stroke.set_stroke_width(size_px * 0.06);
    canvas.draw_path(&path, &stroke);
}

fn draw_art(
    canvas: &skia_safe::Canvas,
    path: &Path,
    dst: skia_safe::Rect,
) -> Result<(), RenderError> {
    let bytes = std::fs::read(path)?;
    let data = Data::new_copy(&bytes);
    let image = Image::from_encoded(data).ok_or(RenderError::ArtDecode)?;
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    canvas.draw_image_rect(&image, None, dst, &paint);
    Ok(())
}

fn pad_horizontal(r: MmRect, pad_mm: f32) -> MmRect {
    MmRect::new(r.x + pad_mm, r.y, (r.w - 2.0 * pad_mm).max(0.0), r.h)
}

fn mm_to_px(mm: f32, dpi: Dpi) -> f32 {
    mm * dpi.px_per_mm()
}

fn color4f_to_color(c: Color4f) -> Color {
    Color::from_argb(
        (c.a * 255.0).round().clamp(0.0, 255.0) as u8,
        (c.r * 255.0).round().clamp(0.0, 255.0) as u8,
        (c.g * 255.0).round().clamp(0.0, 255.0) as u8,
        (c.b * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}
