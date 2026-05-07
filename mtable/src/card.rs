use egui::{Color32, Painter, Pos2, Rect, Vec2};
use rand::Rng;
use serde::{Deserialize, Serialize};
use crate::constants::*;

// --- serde helpers for egui types that don't implement Serialize/Deserialize ---

mod serde_egui {
    use egui::{Color32, Pos2};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub mod color {
        use super::*;
        pub fn serialize<S: Serializer>(c: &Color32, s: S) -> Result<S::Ok, S::Error> {
            [c.r(), c.g(), c.b(), c.a()].serialize(s)
        }
        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Color32, D::Error> {
            let a = <[u8; 4]>::deserialize(d)?;
            Ok(Color32::from_rgba_unmultiplied(a[0], a[1], a[2], a[3]))
        }
    }

    pub mod color_vec {
        use super::*;
        pub fn serialize<S: Serializer>(v: &Vec<Color32>, s: S) -> Result<S::Ok, S::Error> {
            let arr: Vec<[u8; 4]> = v.iter().map(|c| [c.r(), c.g(), c.b(), c.a()]).collect();
            arr.serialize(s)
        }
        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Color32>, D::Error> {
            let arr: Vec<[u8; 4]> = Vec::deserialize(d)?;
            Ok(arr.into_iter()
                .map(|a| Color32::from_rgba_unmultiplied(a[0], a[1], a[2], a[3]))
                .collect())
        }
    }

    pub mod pos2 {
        use super::*;
        pub fn serialize<S: Serializer>(p: &Pos2, s: S) -> Result<S::Ok, S::Error> {
            [p.x, p.y].serialize(s)
        }
        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Pos2, D::Error> {
            let a = <[f32; 2]>::deserialize(d)?;
            Ok(Pos2::new(a[0], a[1]))
        }
    }
}

// --- coordinate helpers used by both Card::draw and Counter::draw ---

pub fn w2s(pos: Pos2, zoom: f32, offset: Vec2) -> Pos2 {
    pos * zoom + offset
}

pub fn w2s_rect(rect: Rect, zoom: f32, offset: Vec2) -> Rect {
    Rect::from_min_size(w2s(rect.min, zoom, offset), rect.size() * zoom)
}

// --- types ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(pub u64);

#[derive(Serialize, Deserialize)]
pub struct Card {
    pub id: EntityId,
    #[serde(with = "serde_egui::pos2")]
    pub pos: Pos2,
    #[serde(with = "serde_egui::color")]
    pub color: Color32,
    pub label: String,
    pub tapped: bool,
    #[serde(with = "serde_egui::color_vec")]
    pub artwork: Vec<Color32>,
    pub z: u64,
}

impl Card {
    pub fn generate_artwork(rng: &mut impl Rng, color: Color32) -> Vec<Color32> {
        let (br, bg, bb) = (color.r() as i32, color.g() as i32, color.b() as i32);
        (0..ARTWORK_GRID_SIZE * ARTWORK_GRID_SIZE)
            .map(|_| {
                let r = (br + rng.gen_range(-ARTWORK_COLOR_JITTER..ARTWORK_COLOR_JITTER)).clamp(0, 255) as u8;
                let g = (bg + rng.gen_range(-ARTWORK_COLOR_JITTER..ARTWORK_COLOR_JITTER)).clamp(0, 255) as u8;
                let b = (bb + rng.gen_range(-ARTWORK_COLOR_JITTER..ARTWORK_COLOR_JITTER)).clamp(0, 255) as u8;
                Color32::from_rgba_unmultiplied(r, g, b, 255)
            })
            .collect()
    }

    // Snap to grid in-place, returns translation delta if a snap occurred.
    pub fn try_snap(&mut self) -> Option<Vec2> {
        let center = self.pos + Vec2::new(CARD_W / 2.0, CARD_H / 2.0);
        let snapped_cx = (center.x / GRID_W).floor() * GRID_W + GRID_W / 2.0;
        let snapped_cy = (center.y / GRID_H).floor() * GRID_H + GRID_H / 2.0;
        if (center.x - snapped_cx).abs() < SNAP_RADIUS_W
            && (center.y - snapped_cy).abs() < SNAP_RADIUS_H
        {
            let snap_pos = Pos2::new(snapped_cx - CARD_W / 2.0, snapped_cy - CARD_H / 2.0);
            let delta = snap_pos - self.pos;
            self.pos = snap_pos;
            Some(delta)
        } else {
            None
        }
    }

    // Read-only snap preview — returns target position without mutating.
    pub fn snap_target(&self) -> Option<Pos2> {
        let center = self.pos + Vec2::new(CARD_W / 2.0, CARD_H / 2.0);
        let snapped_cx = (center.x / GRID_W).floor() * GRID_W + GRID_W / 2.0;
        let snapped_cy = (center.y / GRID_H).floor() * GRID_H + GRID_H / 2.0;
        if (center.x - snapped_cx).abs() < SNAP_RADIUS_W
            && (center.y - snapped_cy).abs() < SNAP_RADIUS_H
        {
            Some(Pos2::new(snapped_cx - CARD_W / 2.0, snapped_cy - CARD_H / 2.0))
        } else {
            None
        }
    }

    pub fn draw(&self, painter: &Painter, zoom: f32, offset: Vec2, is_selected: bool) {
        let card_rect = if self.tapped {
            let screen_center = w2s(self.pos + Vec2::new(CARD_W / 2.0, CARD_H / 2.0), zoom, offset);
            Rect::from_center_size(screen_center, Vec2::new(CARD_H, CARD_W) * zoom)
        } else {
            w2s_rect(Rect::from_min_size(self.pos, Vec2::new(CARD_W, CARD_H)), zoom, offset)
        };

        painter.rect_filled(card_rect, ROUNDING_DEFAULT, self.color);

        let inner = card_rect.shrink(CARD_INNER_PADDING * zoom);
        let (art_rect, txt_rect) = if !self.tapped {
            (
                Rect::from_min_size(inner.min, Vec2::new(inner.width(), inner.height() * CARD_ART_RATIO)),
                Rect::from_min_size(
                    Pos2::new(inner.min.x, inner.min.y + inner.height() * CARD_ART_RATIO),
                    Vec2::new(inner.width(), inner.height() * CARD_TEXT_RATIO),
                ),
            )
        } else {
            // Clockwise rotation: text box moves to left, artwork to right
            (
                Rect::from_min_size(
                    Pos2::new(inner.min.x + inner.width() * CARD_TEXT_RATIO, inner.min.y),
                    Vec2::new(inner.width() * CARD_ART_RATIO, inner.height()),
                ),
                Rect::from_min_size(inner.min, Vec2::new(inner.width() * CARD_TEXT_RATIO, inner.height())),
            )
        };

        let cell_w = art_rect.width() / ARTWORK_GRID_SIZE as f32;
        let cell_h = art_rect.height() / ARTWORK_GRID_SIZE as f32;
        for i in 0..ARTWORK_GRID_SIZE {
            for j in 0..ARTWORK_GRID_SIZE {
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(art_rect.min.x + i as f32 * cell_w, art_rect.min.y + j as f32 * cell_h),
                        Vec2::new(cell_w, cell_h),
                    ),
                    0.0,
                    self.artwork[i * ARTWORK_GRID_SIZE + j],
                );
            }
        }

        painter.rect_filled(txt_rect, 0.0, COLOR_TEXT_BOX);
        painter.text(
            txt_rect.center(),
            egui::Align2::CENTER_CENTER,
            &self.label,
            egui::FontId::proportional(FONT_SIZE_CARD * zoom),
            Color32::WHITE,
        );

        let (stroke_color, stroke_width) = if is_selected {
            (COLOR_SELECTED_HIGHLIGHT, STROKE_WIDTH_SELECTED * zoom)
        } else {
            (COLOR_DEFAULT_STROKE, STROKE_WIDTH_DEFAULT * zoom)
        };
        painter.rect_stroke(card_rect, ROUNDING_DEFAULT, (stroke_width, stroke_color));
    }
}

#[derive(Serialize, Deserialize)]
pub struct Counter {
    pub id: EntityId,
    #[serde(with = "serde_egui::pos2")]
    pub pos: Pos2,
    pub label: String,
    #[serde(with = "serde_egui::color")]
    pub color: Color32,
    pub z: u64,
}

impl Counter {
    pub fn draw(&self, painter: &Painter, zoom: f32, offset: Vec2, is_selected: bool) {
        let rect = w2s_rect(
            Rect::from_min_size(self.pos, Vec2::new(COUNTER_W, COUNTER_H)),
            zoom,
            offset,
        );
        painter.rect_filled(rect, ROUNDING_DEFAULT, self.color);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &self.label,
            egui::FontId::proportional(FONT_SIZE_COUNTER * zoom),
            COLOR_DEFAULT_STROKE,
        );
        let (stroke_color, stroke_width) = if is_selected {
            (COLOR_SELECTED_HIGHLIGHT, STROKE_WIDTH_SELECTED * zoom)
        } else {
            (COLOR_DEFAULT_STROKE, STROKE_WIDTH_DEFAULT * zoom)
        };
        painter.rect_stroke(rect, ROUNDING_DEFAULT, (stroke_width, stroke_color));
    }
}

#[derive(Serialize, Deserialize)]
pub enum BoardObject {
    Card(Card),
    Counter(Counter),
}

impl BoardObject {
    pub fn id(&self) -> EntityId {
        match self {
            Self::Card(c) => c.id,
            Self::Counter(c) => c.id,
        }
    }

    pub fn pos(&self) -> Pos2 {
        match self {
            Self::Card(c) => c.pos,
            Self::Counter(c) => c.pos,
        }
    }

    pub fn set_pos(&mut self, pos: Pos2) {
        match self {
            Self::Card(c) => c.pos = pos,
            Self::Counter(c) => c.pos = pos,
        }
    }

    pub fn rect(&self) -> Rect {
        match self {
            Self::Card(c) => Rect::from_min_size(c.pos, Vec2::new(CARD_W, CARD_H)),
            Self::Counter(c) => Rect::from_min_size(c.pos, Vec2::new(COUNTER_W, COUNTER_H)),
        }
    }

    pub fn z(&self) -> u64 {
        match self {
            Self::Card(c) => c.z,
            Self::Counter(c) => c.z,
        }
    }

    pub fn set_z(&mut self, z: u64) {
        match self {
            Self::Card(c) => c.z = z,
            Self::Counter(c) => c.z = z,
        }
    }

    // Toggle tapped on cards; no-op on counters.
    pub fn tap(&mut self) {
        if let Self::Card(c) = self {
            c.tapped = !c.tapped;
        }
    }

    // Snap card to grid in-place, returning delta. Counter always returns None.
    pub fn try_snap(&mut self) -> Option<Vec2> {
        match self {
            Self::Card(c) => c.try_snap(),
            Self::Counter(_) => None,
        }
    }

    // Read-only snap preview for the drag indicator.
    pub fn snap_target(&self) -> Option<Pos2> {
        match self {
            Self::Card(c) => c.snap_target(),
            Self::Counter(_) => None,
        }
    }

    pub fn as_card(&self) -> Option<&Card> {
        match self {
            Self::Card(c) => Some(c),
            _ => None,
        }
    }

    pub fn as_counter(&self) -> Option<&Counter> {
        match self {
            Self::Counter(c) => Some(c),
            _ => None,
        }
    }

    pub fn as_counter_mut(&mut self) -> Option<&mut Counter> {
        match self {
            Self::Counter(c) => Some(c),
            _ => None,
        }
    }

    pub fn draw(&self, painter: &Painter, zoom: f32, offset: Vec2, is_selected: bool) {
        match self {
            Self::Card(c) => c.draw(painter, zoom, offset, is_selected),
            Self::Counter(c) => c.draw(painter, zoom, offset, is_selected),
        }
    }
}

pub struct DragState {
    pub lead_id: EntityId,
    pub is_group: bool,
    pub offsets: Vec<(EntityId, Vec2)>,
}
