use egui::{Color32, Pos2};

// Grid snapping
pub const GRID_W: f32 = 100.0;
pub const GRID_H: f32 = 140.0;
pub const SNAP_RADIUS_W: f32 = 40.0;
pub const SNAP_RADIUS_H: f32 = 60.0;

// Zoom
pub const ZOOM_SENSITIVITY: f32 = 0.001;
pub const ZOOM_MIN: f32 = 0.1;
pub const ZOOM_MAX: f32 = 10.0;

// World-space fixed positions
pub const DEFAULT_CARD_POS: Pos2 = Pos2::new(0.0, 0.0);
pub const DECK_POS: Pos2 = Pos2::new(10.0, 14.0);
pub const BAG_POS: Pos2 = Pos2::new(130.0, 50.0);

// Object dimensions
pub const CARD_W: f32 = 80.0;
pub const CARD_H: f32 = 112.0;
pub const COUNTER_W: f32 = 40.0;
pub const COUNTER_H: f32 = 40.0;

// Artwork
pub const ARTWORK_GRID_SIZE: usize = 10;
pub const ARTWORK_COLOR_JITTER: i32 = 60;

// Card layout
pub const CARD_INNER_PADDING: f32 = 4.0;
pub const CARD_ART_RATIO: f32 = 0.6;
pub const CARD_TEXT_RATIO: f32 = 0.4;

// Font sizes
pub const FONT_SIZE_CARD: f32 = 10.0;
pub const FONT_SIZE_COUNTER: f32 = 12.0;

// Stroke / rounding
pub const STROKE_WIDTH_DEFAULT: f32 = 1.0;
pub const STROKE_WIDTH_SELECTED: f32 = 3.0;
pub const ROUNDING_DEFAULT: f32 = 2.0;

// Colors — playmat / UI chrome
pub const COLOR_PLAYMAT_BG: Color32 = Color32::from_rgb(30, 80, 30);
// from_rgba_unmultiplied is not const in egui 0.27; use premultiplied equivalents.
// Premultiplied formula: r_pre = r * a / 255 (integer division).
pub const COLOR_GRID_LINE: Color32 = Color32::from_rgba_premultiplied(20, 20, 20, 20);    // (255,255,255,20)
pub const COLOR_DEFAULT_TILE: Color32 = Color32::from_rgba_premultiplied(32, 0, 0, 60);   // (139,0,0,60)
pub const COLOR_SNAP_INDICATOR: Color32 = Color32::from_rgba_premultiplied(21, 0, 0, 40); // (139,0,0,40)
pub const COLOR_SELECTION_FILL: Color32 = Color32::from_rgba_premultiplied(0, 18, 40, 40); // (0,120,255,40)
pub const COLOR_SELECTION_STROKE: Color32 = Color32::LIGHT_BLUE;
pub const COLOR_DECK: Color32 = Color32::from_rgb(0, 0, 139);
pub const COLOR_BAG: Color32 = Color32::from_rgb(139, 0, 0);
pub const COLOR_DECK_INPUT_BG: Color32 = Color32::from_rgb(173, 216, 230);
pub const COLOR_BAG_INPUT_BG: Color32 = Color32::from_rgb(255, 150, 150);

// Colors — card objects
pub const COLOR_TEXT_BOX: Color32 = Color32::BLACK;
pub const COLOR_DEFAULT_STROKE: Color32 = Color32::BLACK;
pub const COLOR_SELECTED_HIGHLIGHT: Color32 = Color32::GOLD;
pub const COLOR_COUNTER_DEFAULT: Color32 = Color32::from_rgb(200, 200, 200);

// Card color palette
pub const CARD_COLORS: [Color32; 7] = [
    Color32::from_rgb(249, 250, 244), // White
    Color32::from_rgb(14, 104, 171),  // Blue
    Color32::from_rgb(45, 45, 45),    // Black
    Color32::from_rgb(211, 32, 42),   // Red
    Color32::from_rgb(0, 115, 62),    // Green
    Color32::from_rgb(200, 168, 75),  // Gold
    Color32::from_rgb(160, 160, 160), // Colorless
];
