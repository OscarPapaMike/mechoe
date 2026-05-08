//! Minimal subset of Scryfall's card object needed for rendering.

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
    /// Scryfall set code (e.g. "drk"). Used for the bottom-left info stamp.
    #[serde(rename = "set", default)]
    pub set_code: Option<String>,
    #[serde(default)]
    pub collector_number: Option<String>,
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

        // Lands and artifacts get their own frame identity regardless of color count.
        let tl = &self.type_line;
        if tl.contains("Basic Land") {
            // Basic lands get the color of their land subtype.
            if tl.contains("Plains")   { return FrameColor::White; }
            if tl.contains("Island")   { return FrameColor::Blue; }
            if tl.contains("Swamp")    { return FrameColor::Black; }
            if tl.contains("Mountain") { return FrameColor::Red; }
            if tl.contains("Forest")   { return FrameColor::Green; }
            return FrameColor::Land; // e.g. Wastes
        }
        if tl.contains("Land") && colors.is_empty() {
            return FrameColor::Land;
        }
        if colors.is_empty() && tl.contains("Artifact") {
            return FrameColor::Artifact;
        }

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

    /// Rewrite land oracle text to pre-M10 wording:
    ///   "({T}: Add {U}.)"  →  "{T}: Add {U} to your mana pool."
    ///   "({T}: Add {R} or {G}.)"  →  "{T}: Add {R} or {G} to your mana pool."
    pub fn old_oracle_text(&self) -> Option<String> {
        let text = self.oracle_text.as_deref()?;
        if !self.type_line.contains("Land") {
            return Some(text.to_string());
        }
        // Strip outer parens if present, then replace trailing "." with the old wording.
        let inner = text.trim();
        let inner = inner.strip_prefix('(').and_then(|s| s.strip_suffix(')')).unwrap_or(inner);
        let rewritten = if let Some(base) = inner.strip_suffix('.') {
            format!("{base} to your mana pool.")
        } else {
            inner.to_string()
        };
        Some(rewritten)
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
    Artifact,
    Land,
}
