//! Frame geometry in millimeters and the two-color (main + alt) palette.

use crate::geometry::{MmRect, CARD_HEIGHT_MM, CARD_WIDTH_MM};
use crate::scryfall::FrameColor;
use skia_safe::Color4f;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardStyle {
    /// Flat two-color layout: art fills the full inner width, alt-color bottom.
    #[default]
    Basic,
    /// 90s premodern-style: colored frame rails flank the art on left/right,
    /// a strip of frame color sits below the art, text area is inset.
    Classic,
}

#[derive(Debug, Clone)]
pub struct FrameSpec {
    pub border_mm: f32,
    pub corner_radius_mm: f32,
    pub card_style: CardStyle,
    /// Width of the colored frame rails on each side (0 for Basic).
    pub frame_rail_mm: f32,

    pub title_bar: MmRect,        // filled with main_color
    pub mana_anchor: (f32, f32),  // top-right corner of mana cost
    pub art_box: MmRect,
    pub type_bar: MmRect,
    pub rules_box: MmRect,
    pub pt_box: MmRect,

    pub frame_color: FrameColor,
    /// Overrides the title bar color independently of frame_color.
    /// Defaults to frame_color; set to Land for basic lands.
    pub title_frame_color: FrameColor,
}

impl FrameSpec {
    /// Flat two-color layout (current default).
    pub fn basic(frame_color: FrameColor) -> Self {
        let border_mm = 3.0;
        let inner_w = CARD_WIDTH_MM - 2.0 * border_mm;   // 57.5
        let title_h = 5.25;
        let title_y = border_mm;                           // 3.0
        let art_y   = title_y + title_h;                   // 8.25
        let art_h   = 45.95;
        let alt_top = art_y + art_h;                       // 54.2

        let type_y = alt_top + 0.75;                       // 54.95
        let type_h = 4.2;
        let rules_gap = 0.7;
        let rules_y = type_y + type_h + rules_gap;         // 59.85
        let pt_y = CARD_HEIGHT_MM - border_mm - 8.0 - 0.5; // 77.4
        let rules_h = (pt_y - rules_y - 0.45).max(0.0);

        let pt_h = 8.0;
        let pt_w = 16.0;
        let pt_y = CARD_HEIGHT_MM - border_mm - pt_h - 0.5;
        let pt_x = CARD_WIDTH_MM  - border_mm - pt_w - 0.5;

        Self {
            border_mm,
            corner_radius_mm: 2.5,
            card_style: CardStyle::Basic,
            frame_rail_mm: 0.0,

            title_bar:   MmRect::new(border_mm, title_y, inner_w, title_h),
            mana_anchor: (CARD_WIDTH_MM - border_mm - 1.5, title_y + title_h * 0.5),
            art_box:     MmRect::new(border_mm, art_y, inner_w, art_h),
            type_bar:    MmRect::new(border_mm + 2.0, type_y, inner_w - 4.0, type_h),
            rules_box:   MmRect::new(border_mm + 2.0, rules_y, inner_w - 4.0, rules_h),
            pt_box:      MmRect::new(pt_x, pt_y, pt_w, pt_h),

            frame_color,
            title_frame_color: frame_color,
        }
    }

    /// 90s-style layout with explicit frame rails on all four sides.
    ///
    /// Rail widths:  top 5mm (title bar) · sides 4mm · type band 4mm · bottom 6mm
    ///
    /// The type line and set symbol live in the horizontal type-rail band
    /// (drawn in alt_color over the main-color rail). The P/T box sits
    /// vertically centred inside the bottom rail. No stamp.
    pub fn classic(frame_color: FrameColor) -> Self {
        let border_mm   = 3.0;
        let top_rail    = 5.0;
        let side_rail   = 4.0;
        let type_rail_h = 4.0;
        let bottom_rail = 6.0;

        let inner_w  = CARD_WIDTH_MM  - 2.0 * border_mm;   // 57.5
        let inner_bot = CARD_HEIGHT_MM - border_mm;          // 85.9

        let art_x = border_mm + side_rail;                  // 7.0
        let art_w = inner_w   - 2.0 * side_rail;            // 49.5
        let art_y = border_mm + top_rail;                   // 8.0
        let art_h = 44.0;
        let art_bot = art_y + art_h;                        // 52.0

        // Type rail sits flush below the art.
        let type_y = art_bot;                               // 52.0

        // Rules text area: from below type rail to above bottom rail.
        let rules_top    = type_y + type_rail_h + 0.5;     // 56.5
        let bottom_top   = inner_bot - bottom_rail;         // 79.9
        let rules_h      = (bottom_top - 0.5 - rules_top).max(0.0); // 22.9

        // Text inset: 1.5mm inside the side rails so text has breathing room.
        let text_inset = 1.5;
        let text_x = art_x + text_inset;                    // 8.5
        let text_w = art_w - 2.0 * text_inset;              // 46.5

        // P/T box: vertically centred in the 6mm bottom rail,
        // right-aligned to art box right edge.
        let pt_h = 5.0;
        let pt_w = 16.0;
        let pt_y = bottom_top + (bottom_rail - pt_h) * 0.5; // 80.4
        let pt_x = art_x + art_w - pt_w;                    // 40.5

        Self {
            border_mm,
            corner_radius_mm: 2.5,
            card_style: CardStyle::Classic,
            frame_rail_mm: side_rail,

            title_bar:   MmRect::new(border_mm, border_mm, inner_w, top_rail),
            mana_anchor: (art_x + art_w, border_mm + top_rail * 0.5),
            art_box:     MmRect::new(art_x, art_y, art_w, art_h),
            type_bar:    MmRect::new(text_x, type_y, text_w, type_rail_h),
            rules_box:   MmRect::new(text_x, rules_top, text_w, rules_h),
            pt_box:      MmRect::new(pt_x, pt_y, pt_w, pt_h),

            frame_color,
            title_frame_color: frame_color,
        }
    }

    // Aliases kept for any remaining callers.
    pub fn premodern(frame_color: FrameColor) -> Self { Self::basic(frame_color) }
    pub fn m15(frame_color: FrameColor)       -> Self { Self::basic(frame_color) }

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

    pub fn main_color(&self) -> Color4f {
        match self.frame_color {
            FrameColor::White     => Color4f::new(0.718, 0.667, 0.612, 1.0), // #B7AA9C
            FrameColor::Blue      => Color4f::new(0.396, 0.584, 0.690, 1.0), // #6595B0
            FrameColor::Black     => Color4f::new(0.212, 0.216, 0.196, 1.0), // #363732
            FrameColor::Red       => Color4f::new(0.494, 0.259, 0.275, 1.0), // #7E4246
            FrameColor::Green     => Color4f::new(0.259, 0.325, 0.286, 1.0), // #425349
            FrameColor::Gold      => Color4f::new(0.671, 0.639, 0.384, 1.0), // #ABA362
            FrameColor::Colorless => Color4f::new(0.412, 0.341, 0.314, 1.0), // #695750
            FrameColor::Artifact  => Color4f::new(0.412, 0.341, 0.314, 1.0), // #695750
            FrameColor::Land      => Color4f::new(0.514, 0.451, 0.420, 1.0), // #83736B
        }
    }

    pub fn title_color(&self) -> Color4f {
        Self { frame_color: self.title_frame_color, ..*self }.main_color()
    }

    /// Alt color derived from title_frame_color (for card name text on the title bar).
    pub fn title_alt_color(&self) -> Color4f {
        Self { frame_color: self.title_frame_color, ..*self }.alt_color()
    }

    /// Main color brightened and desaturated: 60% toward grey, then 50% toward white.
    pub fn alt_color(&self) -> Color4f {
        let c = self.main_color();
        let grey = 0.299 * c.r + 0.587 * c.g + 0.114 * c.b;
        let r = c.r + 0.60 * (grey - c.r);
        let g = c.g + 0.60 * (grey - c.g);
        let b = c.b + 0.60 * (grey - c.b);
        Color4f::new(
            r + 0.50 * (1.0 - r),
            g + 0.50 * (1.0 - g),
            b + 0.50 * (1.0 - b),
            1.0,
        )
    }

    /// Halfway between main_color and alt_color.
    pub fn alt2_color(&self) -> Color4f {
        let m = self.main_color();
        let a = self.alt_color();
        Color4f::new(
            (m.r + a.r) * 0.5,
            (m.g + a.g) * 0.5,
            (m.b + a.b) * 0.5,
            1.0,
        )
    }

    /// 40% alt + 60% alt2: slightly more saturated than alt, lighter than alt2.
    pub fn alt3_color(&self) -> Color4f {
        let a = self.alt_color();
        let a2 = self.alt2_color();
        Color4f::new(
            a.r * 0.4 + a2.r * 0.6,
            a.g * 0.4 + a2.g * 0.6,
            a.b * 0.4 + a2.b * 0.6,
            1.0,
        )
    }

    pub fn frame_paint_color(&self) -> Color4f { self.alt_color() }
    pub fn bar_paint_color(&self)   -> Color4f { self.main_color() }
}

fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let v = max;
    let s = if max == 0.0 { 0.0 } else { delta / max };
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    (h, s, v)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    if s == 0.0 { return (v, v, v); }
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r1 + m, g1 + m, b1 + m)
}
