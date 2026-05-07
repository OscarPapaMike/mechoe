//! Minimal subset of Scryfall's card object. We deserialize more than M1 needs
//! so later milestones don't require a schema change.

pub mod api;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Card {
    pub name: String,
    #[serde(default)]
    pub mana_cost: Option<String>,
    pub type_line: String,
    #[serde(default)]
    pub oracle_text: Option<String>,
    #[serde(default)]
    pub flavor_text: Option<String>,
    #[serde(default)]
    pub power: Option<String>,
    #[serde(default)]
    pub toughness: Option<String>,
    #[serde(default)]
    pub colors: Option<Vec<String>>,
    #[serde(default)]
    pub color_identity: Option<Vec<String>>,
}

impl Card {
    /// Pick a single representative color letter (W/U/B/R/G), gold for multicolor,
    /// or None for colorless. Used by M1 to choose a frame tint.
    pub fn frame_color(&self) -> FrameColor {
        let colors = self
            .colors
            .as_deref()
            .or(self.color_identity.as_deref())
            .unwrap_or(&[]);
        match colors.len() {
            0 => FrameColor::Colorless,
            1 => match colors[0].as_str() {
                "W" => FrameColor::White,
                "U" => FrameColor::Blue,
                "B" => FrameColor::Black,
                "R" => FrameColor::Red,
                "G" => FrameColor::Green,
                _ => FrameColor::Colorless,
            },
            _ => FrameColor::Gold,
        }
    }

    /// Rewrite the modern type line into the 1993/Alpha "Summon X" style.
    /// Examples:
    ///   "Creature \u{2014} Bird"           → "Summon Bird"
    ///   "Legendary Creature \u{2014} Wall" → "Summon Legend Wall"
    ///   "Instant" / "Sorcery" / "Land"     → unchanged
    pub fn old_type_line(&self) -> String {
        let t = self.type_line.as_str();
        const DASH: &str = " \u{2014} ";
        if let Some(rest) = strip_prefix_then_dash(t, "Legendary Creature", DASH) {
            return format!("Summon Legend {rest}");
        }
        if let Some(rest) = strip_prefix_then_dash(t, "Creature", DASH) {
            return format!("Summon {rest}");
        }
        if let Some(rest) = strip_prefix_then_dash(t, "Artifact Creature", DASH) {
            return format!("Summon {rest}");
        }
        t.to_string()
    }
}

fn strip_prefix_then_dash<'a>(s: &'a str, prefix: &str, dash: &str) -> Option<&'a str> {
    s.strip_prefix(prefix).and_then(|rest| rest.strip_prefix(dash))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameColor {
    White,
    Blue,
    Black,
    Red,
    Green,
    Gold,
    Colorless,
}
