//! 1993/premodern-style renderer with main+alt color scheme.

use std::path::{Path, PathBuf};

use skia_safe::{
    paint, shaders as skia_shaders, color_filters, gradient_shader, image_filters,
    surfaces, BlendMode, Color, Color4f, Data, EncodedImageFormat, Font, Image, Paint,
    Path as SkPath, RRect, TileMode, Vector,
};

use crate::fonts::Fonts;
use crate::frame::{CardStyle, FrameSpec};
use crate::geometry::{card_size_px, Dpi, MmRect, CARD_HEIGHT_MM, CARD_WIDTH_MM};
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
    /// Directory of rail texture images (e.g. `_meta/rails/red-c.png`).
    /// When present, classic frames sample real-material images instead of noise.
    pub rails_dir: Option<PathBuf>,
    pub card_style: CardStyle,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            dpi: Dpi::default(),
            fonts_dir: None,
            symbols_dir: None,
            rails_dir: None,
            card_style: CardStyle::Basic,
        }
    }
}

pub fn render_png(
    card: &Card,
    art_path: Option<&Path>,
    opts: &RenderOptions,
) -> Result<Vec<u8>, RenderError> {
    let dpi = opts.dpi;
    let (w_px, h_px) = card_size_px(dpi);

    let mut frame = match opts.card_style {
        CardStyle::Basic   => FrameSpec::basic(card.frame_color()),
        CardStyle::Classic => FrameSpec::classic(card.frame_color()),
    };
    // Basic lands: title bar uses the generic Land color, type area keeps the land's identity color.
    if card.type_line.contains("Basic Land") {
        frame.title_frame_color = crate::scryfall::FrameColor::Land;
    }
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

    let mut alt_paint   = Paint::new(frame.alt_color(),   None);
    alt_paint.set_anti_alias(true);
    let mut main_paint  = Paint::new(frame.main_color(),  None);
    main_paint.set_anti_alias(true);
    let mut title_paint = Paint::new(frame.title_color(), None);
    title_paint.set_anti_alias(true);

    // 2-3. Background fills — differ by style.
    match frame.card_style {
        CardStyle::Basic => {
            // Alt color fills the inner area; main color paints the title bar on top.
            canvas.draw_rect(frame.inner_rect().to_skia(dpi), &alt_paint);
            // Dual land: 8 concentric rectangular rings + cloudy noise overlay.
            let ci = card.color_identity.as_deref().unwrap_or(&[]);
            if card.type_line.contains("Land")
                && !card.type_line.contains("Basic")
                && ci.len() >= 2
            {
                let color_a = dual_letter_alt3(&ci[0]);
                let color_b = dual_letter_alt3(&ci[1]);
                let inner_px = frame.inner_rect().to_skia(dpi);
                let rules_top_px = frame.rules_box.to_skia(dpi).top;
                let box_px = skia_safe::Rect::new(
                    inner_px.left,
                    rules_top_px - mm_to_px(0.7, dpi),
                    inner_px.right,
                    inner_px.bottom,
                );
                const N_RINGS: u32 = 8;
                let step_x = box_px.width()  / (2 * N_RINGS) as f32;
                let step_y = box_px.height() / (2 * N_RINGS) as f32;

                canvas.save();
                canvas.clip_rect(box_px, skia_safe::ClipOp::Intersect, false);

                for i in 0..N_RINGS {
                    let color = if i % 2 == 0 { color_a } else { color_b };
                    let mut ring_paint = Paint::new(color, None);
                    ring_paint.set_anti_alias(true);
                    let s = i as f32;
                    let rect = skia_safe::Rect::new(
                        box_px.left   + s * step_x,
                        box_px.top    + s * step_y,
                        box_px.right  - s * step_x,
                        box_px.bottom - s * step_y,
                    );
                    canvas.draw_rect(rect, &ring_paint);
                }

                // Low-frequency fractal noise overlay for a cloudy texture.
                let ppm = dpi.px_per_mm();
                let freq = 1.0 / (ppm * 12.0); // ~12 mm cloud blobs
                if let Some(noise) = skia_shaders::fractal_noise((freq, freq), 3, 0.0, None) {
                    let mut noise_paint = Paint::default();
                    noise_paint.set_shader(noise);
                    noise_paint.set_blend_mode(BlendMode::Multiply);
                    noise_paint.set_alpha_f(0.0);
                    canvas.draw_rect(box_px, &noise_paint);
                }

                canvas.restore();
            }
            // Basic lands: alt3 fill + noise overlay on the rules box.
            if card.type_line.contains("Basic Land") {
                let box_px = frame.rules_box.to_skia(dpi);
                let mut alt3_paint = Paint::new(frame.alt_color(), None);
                alt3_paint.set_anti_alias(true);
                canvas.draw_rect(box_px, &alt3_paint);
                let ppm = dpi.px_per_mm();
                let freq = 1.0 / (ppm * 12.0);
                if let Some(noise) = skia_shaders::fractal_noise((freq, freq), 3, 0.0, None) {
                    let mut noise_paint = Paint::default();
                    noise_paint.set_shader(noise);
                    noise_paint.set_blend_mode(BlendMode::Multiply);
                    noise_paint.set_alpha_f(0.0);
                    canvas.draw_rect(box_px, &noise_paint);
                }
            }
            canvas.draw_rect(frame.title_bar.to_skia(dpi), &title_paint);
        }
        CardStyle::Classic => {
            // Main color fills the entire inner area (becomes the frame rails).
            canvas.draw_rect(frame.inner_rect().to_skia(dpi), &main_paint);
            // Alt color fills only the rules text area (between type rail and bottom rail).
            let fill_top = frame.type_bar.y + frame.type_bar.h;
            let fill_bot = frame.rules_box.y + frame.rules_box.h + 0.5;
            let fill_rect = MmRect::new(frame.art_box.x, fill_top, frame.art_box.w, (fill_bot - fill_top).max(0.0));
            canvas.draw_rect(fill_rect.to_skia(dpi), &alt_paint);
            // Texture over all rail areas — real image when available, noise fallback.
            draw_classic_rail_texture(
                canvas, &frame, dpi,
                opts.rails_dir.as_deref(),
                &card.name,
            );
        }
    }

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

    // 4b. Short shadow cast onto the frame areas just outside the art edges.
    {
        let shadow_h = mm_to_px(1.0, dpi);
        let alpha = 47u8;
        let dark  = Color::from_argb(alpha, 0, 0, 0);
        let clear = Color::from_argb(0, 0, 0, 0);

        // Above art: transparent at top → dark at art edge.
        let top_y = art_rect_px.top - shadow_h;
        if let Some(shader) = gradient_shader::linear(
            ((art_rect_px.left, top_y), (art_rect_px.left, art_rect_px.top)),
            skia_safe::gradient_shader::GradientShaderColors::Colors(&[clear, dark]),
            None, TileMode::Clamp, None, None,
        ) {
            let mut sp = Paint::default();
            sp.set_anti_alias(true);
            sp.set_shader(shader);
            canvas.draw_rect(
                skia_safe::Rect::from_xywh(art_rect_px.left, top_y, art_rect_px.width(), shadow_h),
                &sp,
            );
        }
        // Below art: dark at art edge → transparent.
        if let Some(shader) = gradient_shader::linear(
            ((art_rect_px.left, art_rect_px.bottom), (art_rect_px.left, art_rect_px.bottom + shadow_h)),
            skia_safe::gradient_shader::GradientShaderColors::Colors(&[dark, clear]),
            None, TileMode::Clamp, None, None,
        ) {
            let mut sp = Paint::default();
            sp.set_anti_alias(true);
            sp.set_shader(shader);
            canvas.draw_rect(
                skia_safe::Rect::from_xywh(art_rect_px.left, art_rect_px.bottom, art_rect_px.width(), shadow_h),
                &sp,
            );
        }
    }

    // 4c. Classic only: thin black inner border around the art box.
    if frame.card_style == CardStyle::Classic {
        let mut border_paint = Paint::default();
        border_paint.set_anti_alias(true);
        border_paint.set_color(Color::BLACK);
        border_paint.set_style(paint::Style::Stroke);
        border_paint.set_stroke_width(mm_to_px(0.25, dpi));
        canvas.draw_rect(frame.art_box.to_skia(dpi), &border_paint);
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

    // 6. Title text — white on the main-color title bar, with a black drop shadow
    // (no blur, shifted 0.35 mm down-right). Windsor Roman.
    let title_padded = pad_horizontal(frame.title_bar, 1.25).to_skia(dpi);
    let title_fit_w = match mana_left_px {
        Some(left_px) => (left_px - title_padded.left - mm_to_px(3.0, dpi)).max(0.0),
        None => title_padded.width(),
    };
    let title_shadow_off = mm_to_px(0.175, dpi);
    let title_style = |color: Color| TextStyle::new(mm_to_px(3.84, dpi))
        .with_color(color)
        .with_halign(HAlign::Left)
        .with_fit(title_fit_w)
        .with_valign_frac(0.50);
    let title_shadow_rect = skia_safe::Rect::new(
        title_padded.left + title_shadow_off, title_padded.top + title_shadow_off,
        title_padded.right + title_shadow_off, title_padded.bottom + title_shadow_off,
    );
    draw_in_rect(canvas, &fonts.roman, &card.name, title_shadow_rect, title_style(Color::BLACK));
    draw_in_rect(canvas, &fonts.roman, &card.name, title_padded,      title_style(color4f_to_color(frame.title_alt_color())));

    // 7. Type line — Windsor Demi, old "Summon X" naming.
    // 7. Type line — all right-side elements (set symbol + stamp) are anchored
    // to the nominal height of the type text: cap-height top to baseline.
    let type_text = card.old_type_line();
    let type_padded = frame.type_bar.to_skia(dpi);
    let (type_font_size_px, set_symbol_color) = match frame.card_style {
        CardStyle::Basic   => (mm_to_px(2.52, dpi), frame.main_color()),
        CardStyle::Classic => (mm_to_px(3.0,  dpi), frame.alt_color()),
    };

    // Compute where draw_in_rect will place the baseline (valign_frac = 0.5 default).
    let type_font_obj = Font::from_typeface(fonts.roman.clone(), type_font_size_px);
    let (_, type_metrics) = type_font_obj.metrics();
    let type_text_h = -type_metrics.ascent + type_metrics.descent;
    let type_baseline_y = type_padded.top
        + (type_padded.height() - type_text_h) * 0.5
        - type_metrics.ascent;
    // Nominal height: baseline to cap-height top (fall back to ascent if cap_height absent).
    let cap_h = if type_metrics.cap_height > 0.0 { type_metrics.cap_height } else { -type_metrics.ascent };
    let nom_top = type_baseline_y - cap_h;
    let nom_bot = type_baseline_y;
    let nom_cy  = (nom_top + nom_bot) * 0.5;
    let nom_h   = nom_bot - nom_top;

    // Right-side sizing. Both scales are relative to nom_h (1.0 = nominal type height).
    const SET_SYMBOL_SCALE: f32 = 2.0;
    const STAMP_SCALE: f32      = 1.5;
    let set_info_gap_mm = 0.5;
    let symbol_size_px  = nom_h * SET_SYMBOL_SCALE;
    // Right edge of set symbol flush with right edge of type bar.
    let right_pad_px    = 0.0_f32;
    let sym_right       = type_padded.right - right_pad_px;
    let info_right      = sym_right - symbol_size_px - mm_to_px(set_info_gap_mm, dpi);

    // Reserve room for right-side block when fitting type text width.
    let right_reserve_px = type_padded.right - info_right;
    let type_fit_w = (type_padded.width() - right_reserve_px).max(0.0);

    match frame.card_style {
        CardStyle::Basic => {
            draw_in_rect(canvas, &fonts.roman, &type_text, type_padded,
                TextStyle::new(type_font_size_px)
                    .with_color(color4f_to_color(frame.main_color()))
                    .with_halign(HAlign::Left)
                    .with_fit(type_fit_w));
        }
        CardStyle::Classic => {
            draw_in_rect(canvas, &fonts.roman, &type_text, type_padded,
                TextStyle::new(type_font_size_px)
                    .with_color(color4f_to_color(frame.alt_color()))
                    .with_halign(HAlign::Left)
                    .with_fit(type_fit_w));
        }
    }

    // 8. Set symbol centered on nom_cy, sized by SET_SYMBOL_SCALE.
    draw_set_symbol(canvas, fonts.symbols2.as_ref(), type_padded, symbol_size_px, right_pad_px, nom_cy, set_symbol_color);

    // 8b. Stamp: two lines centered on nom_cy — Basic style only, monospaced.
    if frame.card_style == CardStyle::Basic {
        let stamp_line_h = nom_h * STAMP_SCALE * 0.5;
        let stamp_size_px = stamp_line_h * 0.82;
        let set_info_color = color4f_to_color(frame.alt2_color());
        let stamp_font = Font::from_typeface(fonts.roman.clone(), stamp_size_px);
        let (_, sm) = stamp_font.metrics();
        let text_h = -sm.ascent + sm.descent;
        let baseline_for = |rect_top: f32| rect_top + (stamp_line_h - text_h) * 0.5 - sm.ascent;
        if let Some(set_code) = &card.set_code {
            draw_mono_stamp(canvas, &stamp_font, &set_code.to_uppercase(),
                info_right, baseline_for(nom_cy - stamp_line_h), set_info_color);
        }
        if let Some(num) = &card.collector_number {
            draw_mono_stamp(canvas, &stamp_font, num,
                info_right, baseline_for(nom_cy), set_info_color);
        }
    }

    // 9. Rules text + flavor text. Both are black on the alt-color
    // background; flavor uses Windsor Light Condensed BT with a fake-italic
    // skew (the face has no italic variant).
    let rules_rect_px = frame.rules_box.to_skia(dpi);
    let oracle_owned = card.old_oracle_text();
    let oracle = oracle_owned.as_deref().filter(|s| !s.trim().is_empty());
    let flavor = card.flavor_text.as_deref().filter(|s| !s.trim().is_empty());
    let (oracle_rect, flavor_rect) = match (oracle.is_some(), flavor.is_some()) {
        (true, true) => {
            // Split proportionally to character count so oracle-heavy cards
            // (e.g. Leviathan) don't overflow into the flavor or P/T area.
            let o_chars = oracle.map_or(1, |s| s.chars().count()) as f32;
            let f_chars = flavor.map_or(1, |s| s.chars().count()) as f32;
            let oracle_frac = (o_chars / (o_chars + f_chars)).clamp(0.45, 0.78);
            let gap = mm_to_px(0.8, dpi);
            let split = rules_rect_px.top + rules_rect_px.height() * oracle_frac;
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
            symbol_cache.as_mut(),
            oracle,
            rect,
            /* base   */ mm_to_px(3.0, dpi),
            /* min    */ mm_to_px(1.7, dpi),
            /* skew   */ 0.0,
            /* color  */ Color::BLACK,
            /* center */ Some(2),
        );
    }
    if let (Some(flavor), Some(rect)) = (flavor, flavor_rect) {
        let a = frame.alt_color();
        let flavor_color = color4f_to_color(Color4f::new(a.r * 0.7, a.g * 0.7, a.b * 0.7, 1.0));
        draw_rules(
            canvas,
            &fonts.flavor,
            None,
            flavor,
            rect,
            /* base   */ mm_to_px(2.6, dpi),
            /* min    */ mm_to_px(1.4, dpi),
            /* skew   */ -0.18,
            /* color  */ flavor_color,
            /* center */ Some(1),
        );
    }

    // 10. P/T — Windsor Roman, no tab background.
    // Basic: large black text, bottom-right of inner area.
    // Classic: smaller alt_color text, vertically centred in the bottom rail (pt_box).
    if let (Some(p), Some(t)) = (&card.power, &card.toughness) {
        let pt_text = format!("{p} / {t}");
        let (pt_size_px, pt_color) = match frame.card_style {
            CardStyle::Basic   => (mm_to_px(6.0, dpi), color4f_to_color(Color4f::new(0.0, 0.0, 0.0, 1.0))),
            CardStyle::Classic => (mm_to_px(4.5, dpi), color4f_to_color(frame.alt_color())),
        };
        let mut pt_font = skia_safe::Font::from_typeface(fonts.roman.clone(), pt_size_px);
        pt_font.set_subpixel(true);
        let (_, ink) = pt_font.measure_str(&pt_text, None);
        let mut pt_paint = Paint::default();
        pt_paint.set_anti_alias(true);
        pt_paint.set_color(pt_color);
        match frame.card_style {
            CardStyle::Basic => {
                let pad_px = mm_to_px(0.5, dpi);
                let inner_right_px  = mm_to_px(CARD_WIDTH_MM - frame.border_mm, dpi);
                let inner_bottom_px = mm_to_px(CARD_HEIGHT_MM - frame.border_mm, dpi);
                let baseline_y = inner_bottom_px - pad_px;
                let x = inner_right_px - pad_px - ink.right;
                canvas.draw_str(&pt_text, (x, baseline_y), &pt_font, &pt_paint);
            }
            CardStyle::Classic => {
                // Centre in pt_box both horizontally and vertically.
                let pt_box_px = frame.pt_box.to_skia(dpi);
                let cx = pt_box_px.left + pt_box_px.width() * 0.5;
                let cy = pt_box_px.top  + pt_box_px.height() * 0.5;
                let x = cx - (ink.left + ink.right) * 0.5;
                let baseline_y = cy - (ink.top + ink.bottom) * 0.5;
                canvas.draw_str(&pt_text, (x, baseline_y), &pt_font, &pt_paint);
            }
        }
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

    let symbol_size_mm = 3.2; // 73% of original 4.4
    let gap_mm = 0.4;
    let right_pad_mm = 1.5;

    let symbol_size_px = symbol_size_mm * dpi.px_per_mm();
    let gap_px = gap_mm * dpi.px_per_mm();
    let right_pad_px = right_pad_mm * dpi.px_per_mm();

    let bar = frame.title_bar.to_skia(dpi);
    let total_w = tokens.len() as f32 * symbol_size_px
        + (tokens.len() as f32 - 1.0).max(0.0) * gap_px;
    let start_x = match frame.card_style {
        // Basic: right-align the mana row inside the title bar.
        CardStyle::Basic => bar.right - right_pad_px - total_w,
        // Classic: centre the rightmost symbol on the art box right edge.
        CardStyle::Classic => {
            let art_right_px = (frame.art_box.x + frame.art_box.w) * dpi.px_per_mm();
            art_right_px - total_w + symbol_size_px * 0.5
        }
    };
    let y = bar.top + (bar.height() - symbol_size_px) * 0.5;

    // Blurred paint for the outer edge pass.
    let mut blur_paint = Paint::default();
    blur_paint.set_anti_alias(true);
    if let Some(blur) = image_filters::blur((1.0, 1.0), TileMode::Decal, None, None) {
        blur_paint.set_image_filter(blur);
    }
    // Sharp paint for the interior overdraw.
    let mut sharp_paint = Paint::default();
    sharp_paint.set_anti_alias(true);

    // How many px inset from the pip circle edge to start the sharp overdraw.
    // Scryfall pips fill r=50 in a 100x100 viewBox, so circle r in dst = symbol_size_px/2.
    let edge_blur_px = 1.5_f32;

    let mut x = start_x;
    for token in &tokens {
        match cache.rasterize(token, symbol_size_px) {
            Ok(image) => {
                let dst = skia_safe::Rect::from_xywh(x, y, symbol_size_px, symbol_size_px);

                // Pass 1: full image blurred — gives soft antialiased edge.
                canvas.draw_image_rect(&image, None, dst, &blur_paint);

                // Pass 2: sharp image clipped to inset circle — restores interior detail.
                let cx = dst.left + dst.width()  * 0.5;
                let cy = dst.top  + dst.height() * 0.5;
                let clip_r = dst.width() * 0.5 - edge_blur_px;
                let mut clip_path = SkPath::new();
                clip_path.add_circle((cx, cy), clip_r, None);
                canvas.save();
                canvas.clip_path(&clip_path, skia_safe::ClipOp::Intersect, true);
                canvas.draw_image_rect(&image, None, dst, &sharp_paint);
                canvas.restore();
            }
            Err(e) => eprintln!("warning: skipping mana symbol {{{token}}}: {e}"),
        }
        x += symbol_size_px + gap_px;
    }

    Some(start_x)
}


/// Draw the set symbol glyph (♣ from Noto Sans Symbols 2) right-aligned in
/// the type bar.  Falls back to the white diamond outline if the font is
/// unavailable.
fn draw_set_symbol(
    canvas: &skia_safe::Canvas,
    typeface: Option<&skia_safe::Typeface>,
    type_bar_px: skia_safe::Rect,
    size_px: f32,
    right_pad_px: f32,
    center_y: f32,
    color: Color4f,
) {
    let cx = type_bar_px.right - right_pad_px - size_px * 0.5;
    let cy = center_y;

    if let Some(tf) = typeface {
        const SYMBOL: &str = "🫐";
        let mut font = Font::from_typeface(tf.clone(), size_px * 0.90);
        font.set_subpixel(true);
        let (_, ink) = font.measure_str(SYMBOL, None);
        let x = cx - (ink.left + ink.right) * 0.5;
        let baseline_y = cy - (ink.top + ink.bottom) * 0.5;
        let mut paint = Paint::new(color, None);
        paint.set_anti_alias(true);
        canvas.draw_str(SYMBOL, (x, baseline_y), &font, &paint);
    } else {
        // Diamond fallback when Noto Sans Symbols 2 isn't available.
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

        let mut stroke = Paint::new(color, None);
        stroke.set_anti_alias(true);
        stroke.set_style(skia_safe::paint::Style::Stroke);
        stroke.set_stroke_width(size_px * 0.06);
        canvas.draw_path(&path, &stroke);
    }
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

/// Textures all classic rail areas (title bar, side rails, type rail, bottom rail).
///
/// Prefers real-material images from `rails_dir` (e.g. `red-c.png`, `red-g.jpeg`).
/// Falls back to procedural Perlin noise when no images are available.
///
/// Image mode: all rails are sampled from the *same* texture crop so they look
/// continuous.  The texture is scaled to cover the inner card area (scale-to-cover)
/// and a random crop offset is chosen deterministically from `card_name`.
fn draw_classic_rail_texture(
    canvas: &skia_safe::Canvas,
    frame: &FrameSpec,
    dpi: Dpi,
    rails_dir: Option<&std::path::Path>,
    card_name: &str,
) {
    use crate::scryfall::FrameColor;

    // ── Rail rects (shared by both branches) ─────────────────────────────────
    let border   = frame.border_mm;
    let inner_w  = CARD_WIDTH_MM - 2.0 * border;
    let side_w   = frame.frame_rail_mm;

    // Side rails run from the art box top all the way down to the bottom rail,
    // so they flank both the art and the rules text area.
    let side_top = frame.art_box.y;
    let side_bot = frame.rules_box.y + frame.rules_box.h + 0.5; // top of bottom rail
    let side_h   = (side_bot - side_top).max(0.0);

    let bot_y = frame.rules_box.y + frame.rules_box.h + 0.5;
    let bot_h  = (CARD_HEIGHT_MM - border - bot_y).max(0.0);

    let rail_rects: [skia_safe::Rect; 5] = [
        // Title bar
        frame.title_bar.to_skia(dpi),
        // Left side rail — spans art AND rules area
        MmRect::new(border, side_top, side_w, side_h).to_skia(dpi),
        // Right side rail — spans art AND rules area
        MmRect::new(frame.art_box.x + frame.art_box.w, side_top, side_w, side_h).to_skia(dpi),
        // Type rail (full inner width)
        MmRect::new(border, frame.type_bar.y, inner_w, frame.type_bar.h).to_skia(dpi),
        // Bottom rail
        MmRect::new(border, bot_y, inner_w, bot_h).to_skia(dpi),
    ];

    // ── Try image-based textures ──────────────────────────────────────────────
    if let Some(dir) = rails_dir {
        if let Some(image) = load_rail_image_for_color(dir, frame.frame_color, card_name) {
            let inner_px = frame.inner_rect().to_skia(dpi);
            draw_rails_from_image(canvas, &image, &rail_rects, inner_px, card_name);
            return;
        }
    }

    // ── Perlin noise fallback ─────────────────────────────────────────────────
    let ppm = dpi.px_per_mm();

    let (turb, fx, fy, oct, low, high): (bool, f32, f32, usize, [f32; 3], [f32; 3]) =
        match frame.frame_color {
            FrameColor::White               => (false, 0.06, 0.06, 2,
                [0.700, 0.670, 0.590], [0.960, 0.945, 0.890]),
            FrameColor::Blue                => (false, 0.09, 0.09, 3,
                [0.020, 0.080, 0.270], [0.160, 0.510, 0.760]),
            FrameColor::Black               => (true,  0.55, 0.55, 6,
                [0.070, 0.055, 0.050], [0.330, 0.280, 0.260]),
            FrameColor::Red                 => (true,  0.40, 0.40, 5,
                [0.440, 0.045, 0.045], [0.910, 0.230, 0.160]),
            FrameColor::Green               => (false, 0.14, 0.58, 4,
                [0.000, 0.210, 0.090], [0.130, 0.590, 0.320]),
            FrameColor::Gold                => (false, 0.16, 0.16, 3,
                [0.520, 0.330, 0.015], [0.890, 0.710, 0.350]),
            FrameColor::Colorless
            | FrameColor::Artifact          => (true,  0.75, 0.75, 2,
                [0.390, 0.365, 0.345], [0.760, 0.730, 0.705]),
            FrameColor::Land                => (false, 0.05, 0.05, 2,
                [0.560, 0.510, 0.480], [0.780, 0.740, 0.710]),
        };

    let shader = if turb {
        skia_shaders::turbulence((fx / ppm, fy / ppm), oct, 0.0, None)
    } else {
        skia_shaders::fractal_noise((fx / ppm, fy / ppm), oct, 0.0, None)
    };
    let shader = match shader { Some(s) => s, None => return };

    let cf_arr: [f32; 20] = [
        high[0] - low[0], 0.0, 0.0, 0.0, low[0],
        high[1] - low[1], 0.0, 0.0, 0.0, low[1],
        high[2] - low[2], 0.0, 0.0, 0.0, low[2],
        0.0,              0.0, 0.0, 1.0, 0.0,
    ];
    let cf = color_filters::matrix_row_major(&cf_arr, None);

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_shader(shader);
    paint.set_color_filter(cf);
    paint.set_blend_mode(BlendMode::SoftLight);

    for r in &rail_rects {
        canvas.draw_rect(*r, &paint);
    }
}

/// Returns a colour name string for the given `FrameColor`, used to match
/// texture filenames (`red-c.png`, `blue-g.jpeg`, …).
fn frame_color_name(c: crate::scryfall::FrameColor) -> &'static str {
    use crate::scryfall::FrameColor::*;
    match c {
        White => "white", Blue => "blue", Black => "black",
        Red => "red", Green => "green", Gold => "gold",
        Colorless => "colorless", Artifact => "artifact", Land => "land",
    }
}

/// Scans `rails_dir` for files matching `{color}-*.{png,jpg,jpeg}`, picks one
/// deterministically based on `card_name`, loads it and returns the decoded image.
fn load_rail_image_for_color(
    rails_dir: &std::path::Path,
    color: crate::scryfall::FrameColor,
    card_name: &str,
) -> Option<Image> {
    let prefix = format!("{}-", frame_color_name(color));

    let mut candidates: Vec<std::path::PathBuf> = std::fs::read_dir(rails_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.starts_with(&prefix)
                && (name.ends_with(".png") || name.ends_with(".jpg") || name.ends_with(".jpeg"))
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }
    candidates.sort(); // deterministic order

    // Pick one based on a hash of the card name.
    let idx = (name_hash(card_name) as usize) % candidates.len();
    let path = &candidates[idx];

    let bytes = std::fs::read(path).ok()?;
    let data = Data::new_copy(&bytes);
    Image::from_encoded(data)
}

/// Draws the texture image over every rail rect.  All rects sample from the
/// same continuous region of the image so adjacent rails look seamless.
///
/// The mapping is: the inner card area (pixel space) corresponds to a
/// scale-to-cover crop of the texture, with a random x offset chosen from
/// the available slack.  The random offset is derived from `card_name` so
/// every card gets a unique but reproducible crop.
fn draw_rails_from_image(
    canvas: &skia_safe::Canvas,
    image: &Image,
    rail_rects: &[skia_safe::Rect],
    inner_card_px: skia_safe::Rect,
    card_name: &str,
) {
    use skia_safe::canvas::SrcRectConstraint;

    let tex_w = image.width()  as f32;
    let tex_h = image.height() as f32;
    let card_w = inner_card_px.width();
    let card_h = inner_card_px.height();

    // scale-to-cover: number of texture pixels consumed per canvas pixel.
    // Take the minimum so the texture fills the shorter card dimension exactly
    // and has slack in the other (from which we take a random crop).
    let scale = (tex_w / card_w).min(tex_h / card_h);

    // Available slack in each axis (in texture pixels).
    let slack_x = (tex_w - card_w * scale).max(0.0);
    let slack_y = (tex_h - card_h * scale).max(0.0);

    // Deterministic random offsets within the available slack.
    let h = name_hash(card_name);
    let off_x = (h         & 0xFFFF) as f32 / 65535.0 * slack_x;
    let off_y = ((h >> 16) & 0xFFFF) as f32 / 65535.0 * slack_y;

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    // Normal blend: texture directly replaces the solid base fill in rail areas.

    for &dst in rail_rects {
        // Map each rail's canvas position → texture source rect.
        let rel_x = dst.left - inner_card_px.left;
        let rel_y = dst.top  - inner_card_px.top;
        let src = skia_safe::Rect::from_xywh(
            off_x + rel_x * scale,
            off_y + rel_y * scale,
            dst.width()  * scale,
            dst.height() * scale,
        );
        canvas.draw_image_rect(image, Some((&src, SrcRectConstraint::Fast)), dst, &paint);
    }
}

/// Deterministic hash of a string, used to pick a reproducible-but-varied
/// texture crop for each card.
fn name_hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
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

fn draw_mono_stamp(
    canvas: &skia_safe::Canvas,
    font: &Font,
    text: &str,
    right_x: f32,
    baseline_y: f32,
    color: Color,
) {
    // Cell width = widest A-Z / 0-9 glyph in this font.
    let cell_w = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
        .chars()
        .map(|c| font.measure_str(c.to_string().as_str(), None).0)
        .fold(0.0_f32, f32::max);

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(color);

    let n = text.chars().count();
    let mut x = right_x - cell_w * n as f32;
    for ch in text.chars() {
        let s = ch.to_string();
        let w = font.measure_str(s.as_str(), None).0;
        canvas.draw_str(s.as_str(), (x + (cell_w - w) * 0.5, baseline_y), font, &paint);
        x += cell_w;
    }
}

fn dual_letter_alt3(letter: &str) -> Color4f {
    use crate::scryfall::FrameColor;
    let fc = match letter {
        "W" => FrameColor::White,
        "U" => FrameColor::Blue,
        "B" => FrameColor::Black,
        "R" => FrameColor::Red,
        "G" => FrameColor::Green,
        _   => FrameColor::Colorless,
    };
    FrameSpec::basic(fc).alt3_color()
}
