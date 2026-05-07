use egui::Pos2;
use rand::{Rng, SeedableRng};
use crate::card::{Card, EntityId};
use crate::constants::CARD_COLORS;

pub struct Deck {
    rng: rand::rngs::SmallRng,
    pub drawn: u32,
}

impl Deck {
    pub fn new() -> Self {
        Self {
            rng: rand::rngs::SmallRng::from_entropy(),
            drawn: 0,
        }
    }

    pub fn draw(&mut self, id: EntityId) -> Card {
        let color = CARD_COLORS[self.rng.gen_range(0..CARD_COLORS.len())];
        let artwork = Card::generate_artwork(&mut self.rng, color);
        self.drawn += 1;
        Card {
            id,
            pos: Pos2::new(100.0, 100.0),
            color,
            label: format!("Card {}", id.0),
            tapped: false,
            artwork,
            z: 0,
        }
    }
}
