//! Millimeter-native geometry. Pixels appear only at the rasterization boundary.

pub const CARD_WIDTH_MM: f32 = 63.5;
pub const CARD_HEIGHT_MM: f32 = 88.9;
pub const MM_PER_INCH: f32 = 25.4;
pub const DEFAULT_DPI: f32 = 300.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mm(pub f32);

impl Mm {
    pub fn to_px(self, dpi: Dpi) -> f32 {
        self.0 * dpi.px_per_mm()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dpi(pub f32);

impl Dpi {
    pub fn px_per_mm(self) -> f32 {
        self.0 / MM_PER_INCH
    }
}

impl Default for Dpi {
    fn default() -> Self {
        Dpi(DEFAULT_DPI)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MmRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl MmRect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn to_skia(self, dpi: Dpi) -> skia_safe::Rect {
        let s = dpi.px_per_mm();
        skia_safe::Rect::from_xywh(self.x * s, self.y * s, self.w * s, self.h * s)
    }
}

pub fn card_size_px(dpi: Dpi) -> (i32, i32) {
    let s = dpi.px_per_mm();
    (
        (CARD_WIDTH_MM * s).round() as i32,
        (CARD_HEIGHT_MM * s).round() as i32,
    )
}
