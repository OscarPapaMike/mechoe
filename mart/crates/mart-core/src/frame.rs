//! Frame geometry in millimeters and the two-color (main + alt) palette.

use crate::geometry::{MmRect, CARD_HEIGHT_MM, CARD_WIDTH_MM};
use crate::scryfall::FrameColor;
use skia_safe::Color4f;

#[derive(Debug, Clone)]
pub struct FrameSpec {
    pub border_mm: f32,
    pub corner_radius_mm: f32,

    pub title_bar: MmRect,        // filled with main_color
    pub mana_anchor: (f32, f32),  // top-right corner of mana cost
    pub art_box: MmRect,
    /// Text-only band where the type line is drawn over the alt-color bottom.
    pub type_bar: MmRect,
    pub rules_box: MmRect,
    pub pt_box: MmRect,

    pub frame_color: FrameColor,
}

impl FrameSpec {
    pub fn m15(frame_color: FrameColor) -> Self {
        Self::premodern(frame_color)
    }

    /// 1993/premodern-style layout, two-color (main + alt).
    ///   * title bar (main color) flush against the inner border on top, left, right
    ///   * art fills the full inner width below the title
    ///   * everything below the art sits on the alt-color background — no
    ///     separate type bar; the type line is just larger text on alt
    ///   * P/T sits in the bottom-right with no tab behind it
    pub fn premodern(frame_color: FrameColor) -> Self {
        let border_mm = 3.0;
        let inner_w = CARD_WIDTH_MM - 2.0 * border_mm; // 57.5
        let title_h = 7.2;                                        // was 9.0 — 20% smaller
        let title_y = border_mm;                                  // 3.0
        let art_y   = title_y + title_h;                          // 10.2
        let art_h   = 44.0;
        let alt_top = art_y + art_h;                              // 54.2

        // Type line text band (drawn directly onto alt color, no bar fill).
        let type_y = alt_top + 1.5;                               // 55.7
        let type_h = 6.0;

        let rules_y = type_y + type_h + 1.0;                      // 62.7
        let rules_h = 15.0;

        let pt_h = 8.0;
        let pt_w = 16.0;
        let pt_y = CARD_HEIGHT_MM - border_mm - pt_h;             // 77.9
        let pt_x = CARD_WIDTH_MM - border_mm - pt_w - 0.5;        // 44.0

        Self {
            border_mm,
            corner_radius_mm: 2.5,

            title_bar:   MmRect::new(border_mm, title_y, inner_w, title_h),
            mana_anchor: (CARD_WIDTH_MM - border_mm - 1.5, title_y + title_h * 0.5),
            art_box:     MmRect::new(border_mm, art_y, inner_w, art_h),
            type_bar:    MmRect::new(border_mm + 2.0, type_y, inner_w - 4.0, type_h),
            rules_box:   MmRect::new(border_mm + 2.0, rules_y, inner_w - 4.0, rules_h),
            pt_box:      MmRect::new(pt_x, pt_y, pt_w, pt_h),

            frame_color,
        }
    }

    pub fn outer_rect(&self) -> MmRect {
        MmRect::new(0.0, 0.0, CARD_WIDTH_MM, CARD_HEIGHT_MM)
    }

    pub fn inner_rect(&self) -> MmRect {
        MmRect::new(
            self.border_mm,
            self.border_mm,
            CARD_WIDTH_MM - 2.0 * self.border_mm,
            CARD_HEIGHT_MM - 2.0 * self.border_mm,
        )
    }

    /// The bold, saturated card color. Used for the title bar and for type-line text.
    pub fn main_color(&self) -> Color4f {
        match self.frame_color {
            FrameColor::White     => Color4f::new(0.62, 0.55, 0.42, 1.0),
            FrameColor::Blue      => Color4f::new(0.28, 0.42, 0.58, 1.0),
            FrameColor::Black     => Color4f::new(0.22, 0.20, 0.22, 1.0),
            FrameColor::Red       => Color4f::new(0.62, 0.30, 0.25, 1.0),
            FrameColor::Green     => Color4f::new(0.36, 0.50, 0.38, 1.0),
            FrameColor::Gold      => Color4f::new(0.66, 0.52, 0.28, 1.0),
            FrameColor::Colorless => Color4f::new(0.50, 0.50, 0.52, 1.0),
        }
    }

    /// The light, desaturated complement. Used for the bottom half background
    /// and for title text laid over the main color.
    pub fn alt_color(&self) -> Color4f {
        match self.frame_color {
            FrameColor::White     => Color4f::new(0.96, 0.94, 0.86, 1.0),
            FrameColor::Blue      => Color4f::new(0.78, 0.86, 0.92, 1.0),
            FrameColor::Black     => Color4f::new(0.78, 0.76, 0.76, 1.0),
            FrameColor::Red       => Color4f::new(0.94, 0.80, 0.74, 1.0),
            FrameColor::Green     => Color4f::new(0.74, 0.82, 0.70, 1.0),
            FrameColor::Gold      => Color4f::new(0.94, 0.86, 0.66, 1.0),
            FrameColor::Colorless => Color4f::new(0.86, 0.86, 0.88, 1.0),
        }
    }

    // Backwards-compat aliases retained briefly for callers that still call
    // these names.
    pub fn frame_paint_color(&self) -> Color4f { self.alt_color() }
    pub fn bar_paint_color(&self) -> Color4f { self.main_color() }
}
