//! Single-style text drawing helpers. Mixed-run shaping (body + mana symbols)
//! arrives in M4.

use skia_safe::{Canvas, Color, Font, Paint, Typeface};

#[derive(Clone, Copy)]
pub enum HAlign { Left, Center, Right }

#[derive(Clone, Copy)]
pub struct TextStyle {
    pub size_px: f32,
    pub color:   Color,
    pub halign:  HAlign,
    /// If the rendered string is wider than `max_w_px`, the font is first
    /// compressed horizontally via `set_scale_x` (down to 0.65×) before
    /// falling back to font-size shrink. None disables fitting.
    pub fit_width_px: Option<f32>,
    /// Vertical position of the text within the rect, as a fraction of the
    /// available vertical space. 0.0 = top-aligned, 0.5 = centered (default),
    /// 1.0 = bottom-aligned.
    pub valign_frac: f32,
}

impl TextStyle {
    pub fn new(size_px: f32) -> Self {
        Self {
            size_px,
            color: Color::BLACK,
            halign: HAlign::Left,
            fit_width_px: None,
            valign_frac: 0.5,
        }
    }
    pub fn with_color(mut self, c: Color) -> Self { self.color = c; self }
    pub fn with_halign(mut self, h: HAlign) -> Self { self.halign = h; self }
    pub fn with_fit(mut self, w: f32) -> Self { self.fit_width_px = Some(w); self }
    pub fn with_valign_frac(mut self, f: f32) -> Self { self.valign_frac = f; self }
}

/// Draw `text` inside the given pixel rect. Vertically centered using font
/// metrics; horizontally aligned per `style.halign`. Returns the size actually
/// used (after any fit-shrinking), useful for downstream layout.
pub fn draw_in_rect(
    canvas: &Canvas,
    typeface: &Typeface,
    text: &str,
    rect_px: skia_safe::Rect,
    style: TextStyle,
) -> f32 {
    if text.is_empty() {
        return style.size_px;
    }

    let mut font = Font::from_typeface(typeface.clone(), style.size_px);
    font.set_subpixel(true);

    // Auto-fit: prefer horizontal compression (scale_x) before shrinking the
    // size — keeps weight/height intact for things like card titles.
    let mut size = style.size_px;
    let natural_w = font.measure_str(text, None).0;
    let mut width = natural_w;
    if let Some(max_w) = style.fit_width_px {
        if natural_w > max_w {
            let target = max_w / natural_w;
            const MIN_SCALE_X: f32 = 0.65;
            if target >= MIN_SCALE_X {
                font.set_scale_x(target);
                width = max_w;
            } else {
                font.set_scale_x(MIN_SCALE_X);
                let after_squish = natural_w * MIN_SCALE_X;
                if after_squish > max_w {
                    size = style.size_px * (max_w / after_squish);
                    font.set_size(size);
                }
                width = font.measure_str(text, None).0;
            }
        }
    }

    let (_line_height, metrics) = font.metrics();
    let text_h = -metrics.ascent + metrics.descent;
    let valign = style.valign_frac.clamp(0.0, 1.0);
    let baseline_y = rect_px.top + (rect_px.height() - text_h) * valign - metrics.ascent;
    let x = match style.halign {
        HAlign::Left   => rect_px.left,
        HAlign::Center => rect_px.left + (rect_px.width() - width) * 0.5,
        HAlign::Right  => rect_px.right - width,
    };

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(style.color);

    canvas.draw_str(text, (x, baseline_y), &font, &paint);
    size
}
