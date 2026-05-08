use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use eframe::egui::{self, scroll_area::ScrollBarVisibility, Color32, RichText, ScrollArea, TextEdit};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use mart_core::symbols::{parse_mana_cost_inline, InlineSpan};
use mdata_core::{
    http::Client,
    index::CardRecord,
    paths as mdata_paths,
    store::ensure_card,
    Database,
};
use mart_core::{render_png, Card, CardStyle, Dpi, RenderOptions};

// ── types ────────────────────────────────────────────────────────────────────

struct SearchResult {
    record: CardRecord,
    /// True when data/<SET>/<NUM>.json already exists locally.
    has_local: bool,
}

struct CardInfo {
    name: String,
    set_code: String,
    collector_number: String,
    released_at: String,
    mana_cost: Option<String>,
    type_line: Option<String>,
    oracle_text: Option<String>,
    flavor_text: Option<String>,
    power: Option<String>,
    toughness: Option<String>,
    artist: Option<String>,
    rarity: Option<String>,
}

impl CardInfo {
    fn from_record(record: &CardRecord) -> Self {
        let v: serde_json::Value =
            serde_json::from_str(&record.card_json).unwrap_or(serde_json::Value::Null);
        Self {
            name: record.name.clone(),
            set_code: record.set_code.clone(),
            collector_number: record.collector_number.clone(),
            released_at: record.released_at.clone(),
            mana_cost: v["mana_cost"].as_str().map(String::from),
            type_line: v["type_line"].as_str().map(String::from),
            oracle_text: v["oracle_text"].as_str().map(String::from),
            flavor_text: v["flavor_text"].as_str().map(String::from),
            power: v["power"].as_str().map(String::from),
            toughness: v["toughness"].as_str().map(String::from),
            artist: v["artist"].as_str().map(String::from),
            rarity: v["rarity"].as_str().map(String::from),
        }
    }
}

enum RenderState {
    Empty,
    Rendering,
    Done(egui::TextureHandle),
    Error(String),
}

// ── app ──────────────────────────────────────────────────────────────────────

struct CardApp {
    // Search
    query: String,
    last_query: String,
    results: Vec<SearchResult>,
    selected_idx: Option<usize>,

    // Card display
    card_info: Option<CardInfo>,

    // Render
    render_state: RenderState,
    render_rx: Option<mpsc::Receiver<Result<Vec<u8>, String>>>,

    // History (newest first, max 10)
    history: Vec<CardRecord>,

    // Symbol textures for oracle text display
    symbol_textures: HashMap<String, egui::TextureHandle>,
    symbols_loaded: bool,
    symbols_dir: Option<PathBuf>,

    // Data
    db: Option<Database>,
    data_dir: PathBuf,
    db_error: Option<String>,
    matcher: SkimMatcherV2,
}

impl CardApp {
    fn new() -> Self {
        let data_dir = mdata_paths::data_dir(None);
        let symbols_dir = {
            let p = mdata_paths::symbols_dir(&data_dir);
            if p.is_dir() { Some(p) } else { None }
        };
        let (db, db_error) = match Database::open(&data_dir) {
            Ok(db) => (Some(db), None),
            Err(e) => (None, Some(e.to_string())),
        };
        Self {
            query: String::new(),
            last_query: String::new(),
            results: Vec::new(),
            selected_idx: None,
            card_info: None,
            render_state: RenderState::Empty,
            render_rx: None,
            history: Vec::new(),
            symbol_textures: HashMap::new(),
            symbols_loaded: false,
            symbols_dir,
            db,
            data_dir,
            db_error,
            matcher: SkimMatcherV2::default(),
        }
    }

    /// Load all SVGs from the symbols directory as egui textures (done once).
    fn load_all_symbols(&mut self, ctx: &egui::Context) {
        let Some(dir) = self.symbols_dir.clone() else { return };
        let Ok(entries) = std::fs::read_dir(&dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("svg") {
                continue;
            }
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            if self.symbol_textures.contains_key(&stem) {
                continue;
            }
            if let Some(tex) = load_svg_texture(ctx, &path, 32, &stem) {
                self.symbol_textures.insert(stem, tex);
            }
        }
    }

    fn do_search(&mut self) {
        let query = self.query.trim().to_string();
        if query == self.last_query {
            return;
        }
        self.last_query = query.clone();
        self.results.clear();
        self.selected_idx = None;

        let db = match &self.db {
            Some(db) => db,
            None => return,
        };
        if query.is_empty() {
            return;
        }

        let candidates = db.search(&query, 300).unwrap_or_default();
        let mut scored: Vec<(i64, CardRecord)> = candidates
            .into_iter()
            .filter_map(|r| self.matcher.fuzzy_match(&r.name, &query).map(|s| (s, r)))
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.truncate(50);

        let data_dir = &self.data_dir;
        self.results = scored
            .into_iter()
            .map(|(_, record)| {
                let json_path = mdata_paths::card_json_path(
                    data_dir,
                    &record.set_code,
                    &record.collector_number,
                );
                SearchResult { has_local: json_path.exists(), record }
            })
            .collect();
    }

    fn select_record(&mut self, record: CardRecord, ctx: &egui::Context) {
        self.card_info = Some(CardInfo::from_record(&record));

        let data_dir = self.data_dir.clone();
        let symbols_dir = mdata_paths::symbols_dir(&data_dir);
        let ctx_clone = ctx.clone();
        let (tx, rx) = mpsc::channel();
        self.render_rx = Some(rx);
        self.render_state = RenderState::Rendering;

        let record_clone = record.clone();
        std::thread::spawn(move || {
            let result = render_card_thread(record_clone, data_dir, symbols_dir);
            let _ = tx.send(result);
            ctx_clone.request_repaint();
        });

        // Move to front of history, dedup by set+number.
        self.history.retain(|h| {
            !(h.set_code == record.set_code && h.collector_number == record.collector_number)
        });
        self.history.insert(0, record);
        self.history.truncate(10);
    }

    fn select_idx(&mut self, idx: usize, ctx: &egui::Context) {
        if self.selected_idx == Some(idx) {
            return;
        }
        self.selected_idx = Some(idx);
        let record = self.results[idx].record.clone();
        self.select_record(record, ctx);
    }

    fn poll_render(&mut self, ctx: &egui::Context) {
        let done = self.render_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(result) = done {
            self.render_rx = None;
            self.render_state = match result {
                Ok(png) => match load_texture(ctx, &png) {
                    Ok(tex) => RenderState::Done(tex),
                    Err(e) => RenderState::Error(e),
                },
                Err(e) => RenderState::Error(e),
            };
        }
    }
}

// ── egui app ─────────────────────────────────────────────────────────────────

impl eframe::App for CardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_render(ctx);
        if !self.symbols_loaded {
            self.symbols_loaded = true;
            self.load_all_symbols(ctx);
        }

        // ── Left panel: search + history ───────────────────────────────────
        egui::SidePanel::left("search_panel")
            .min_width(220.0)
            .max_width(300.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let resp = ui.add(
                        TextEdit::singleline(&mut self.query)
                            .hint_text("Card name…")
                            .desired_width(ui.available_width() - 56.0),
                    );
                    if (resp.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        || ui.button("Search").clicked()
                    {
                        self.do_search();
                    }
                });
                ui.separator();

                if let Some(err) = &self.db_error {
                    ui.colored_label(
                        Color32::from_rgb(220, 80, 80),
                        format!("No index:\n{err}\n\nRun: mdata sync"),
                    );
                    return;
                }

                // Allocate heights: results get 2/3, history 1/3.
                let avail_h = ui.available_height();
                let history_h = (avail_h / 3.0).max(80.0).min(250.0);
                let results_h = avail_h - history_h - 36.0; // 36 = separator + "Recent" label

                // ── Results ──
                ScrollArea::vertical()
                    .id_salt("results_scroll")
                    .max_height(results_h)
                    .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        if self.results.is_empty() && !self.last_query.is_empty() {
                            ui.colored_label(Color32::GRAY, "No results.");
                            return;
                        }
                        let mut clicked = None;
                        for (i, hit) in self.results.iter().enumerate() {
                            let selected = self.selected_idx == Some(i);
                            let label = format!(
                                "{}\n{} #{}",
                                hit.record.name,
                                hit.record.set_code,
                                hit.record.collector_number
                            );
                            let text = if hit.has_local {
                                RichText::new(label)
                            } else {
                                RichText::new(label).color(Color32::GRAY)
                            };
                            if ui.selectable_label(selected, text).clicked() {
                                clicked = Some(i);
                            }
                            ui.add_space(2.0);
                        }
                        if let Some(i) = clicked {
                            self.select_idx(i, ctx);
                        }
                    });

                ui.separator();
                ui.label(RichText::new("Recent").small().color(Color32::GRAY));

                // ── History ──
                let history = self.history.clone();
                ScrollArea::vertical()
                    .id_salt("history_scroll")
                    .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
                    .max_height(history_h)
                    .show(ui, |ui| {
                        if history.is_empty() {
                            ui.colored_label(Color32::DARK_GRAY, "No history yet.");
                            return;
                        }
                        let mut clicked: Option<CardRecord> = None;
                        for record in &history {
                            let is_selected = self.card_info.as_ref().map_or(false, |info| {
                                info.set_code == record.set_code
                                    && info.collector_number == record.collector_number
                            });
                            let label = format!(
                                "{}\n{} #{}",
                                record.name, record.set_code, record.collector_number
                            );
                            if ui.selectable_label(is_selected, RichText::new(label)).clicked() {
                                clicked = Some(record.clone());
                            }
                            ui.add_space(2.0);
                        }
                        if let Some(record) = clicked {
                            self.select_record(record, ctx);
                        }
                    });
            });

        // ── Right panel: card render ────────────────────────────────────────
        egui::SidePanel::right("render_panel")
            .min_width(280.0)
            .default_width(400.0)
            .show(ctx, |ui| match &self.render_state {
                RenderState::Empty => {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new("Select a card").color(Color32::GRAY));
                    });
                }
                RenderState::Rendering => {
                    ui.centered_and_justified(|ui| {
                        ui.spinner();
                    });
                }
                RenderState::Done(texture) => {
                    let avail = ui.available_size();
                    let aspect = 750.0_f32 / 1050.0;
                    let w = avail.x.min(avail.y * aspect);
                    let h = w / aspect;
                    ui.centered_and_justified(|ui| {
                        ui.add(egui::Image::new(egui::load::SizedTexture::new(
                            texture.id(),
                            egui::vec2(w, h),
                        )));
                    });
                }
                RenderState::Error(e) => {
                    ui.colored_label(
                        Color32::from_rgb(220, 80, 80),
                        format!("Render error:\n{e}"),
                    );
                }
            });

        // ── Central panel: card info ────────────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(info) = &self.card_info else {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new("Search for a card and select it.")
                            .color(Color32::GRAY),
                    );
                });
                return;
            };

            // Clone fields we need to avoid borrow conflict with symbol_textures.
            let name = info.name.clone();
            let set_str = format!(
                "{} #{} · {}",
                info.set_code, info.collector_number, info.released_at
            );
            let mana_cost = info.mana_cost.clone();
            let type_line = info.type_line.clone();
            let rarity = info.rarity.clone();
            let oracle_text = info.oracle_text.clone();
            let flavor_text = info.flavor_text.clone();
            let power = info.power.clone();
            let toughness = info.toughness.clone();
            let artist = info.artist.clone();

            let label_color = Color32::from_rgb(130, 130, 180);
            let mono = egui::FontId::monospace(13.0);

            ScrollArea::vertical().show(ui, |ui| {
                let plain = |ui: &mut egui::Ui, heading: &str, value: &str| {
                    ui.label(RichText::new(heading).small().color(label_color));
                    ui.add(egui::Label::new(RichText::new(value).font(mono.clone())).wrap());
                    ui.add_space(10.0);
                };

                plain(ui, "NAME", &name);
                plain(ui, "SET / NUMBER", &set_str);

                if let Some(v) = mana_cost {
                    ui.label(RichText::new("MANA COST").small().color(label_color));
                    show_text_with_symbols(ui, &v, &self.symbol_textures, &mono);
                    ui.add_space(10.0);
                }
                if let Some(v) = type_line {
                    plain(ui, "TYPE", &v);
                }
                if let Some(v) = rarity {
                    plain(ui, "RARITY", &v);
                }
                if let Some(v) = oracle_text {
                    ui.label(RichText::new("ORACLE TEXT").small().color(label_color));
                    show_text_with_symbols(ui, &v, &self.symbol_textures, &mono);
                    ui.add_space(10.0);
                }
                if let Some(v) = flavor_text {
                    ui.label(RichText::new("FLAVOR TEXT").small().color(label_color));
                    show_text_with_symbols(ui, &v, &self.symbol_textures, &mono);
                    ui.add_space(10.0);
                }
                if let (Some(p), Some(t)) = (power, toughness) {
                    plain(ui, "POWER / TOUGHNESS", &format!("{p} / {t}"));
                }
                if let Some(v) = artist {
                    plain(ui, "ARTIST", &v);
                }
            });
        });
    }
}

// ── oracle text with inline symbols ─────────────────────────────────────────

/// Render oracle/flavor/mana text with inline symbol images where available.
/// Handles `{T}`, `{U}`, `{2/B}` etc. by looking them up in symbol_textures.
fn show_text_with_symbols(
    ui: &mut egui::Ui,
    text: &str,
    symbol_textures: &HashMap<String, egui::TextureHandle>,
    mono: &egui::FontId,
) {
    let sym_px = mono.size + 2.0;

    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            ui.add_space(2.0);
        }
        if line.trim().is_empty() {
            ui.add_space(4.0);
            continue;
        }

        let spans = parse_mana_cost_inline(line);

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.spacing_mut().item_spacing.y = 2.0;

            for span in &spans {
                match span {
                    InlineSpan::Text(t) => {
                        // Emit each word as a separate label so horizontal_wrapped
                        // can break between words.
                        for word in t.split_whitespace() {
                            ui.label(RichText::new(word).font(mono.clone()));
                        }
                    }
                    InlineSpan::Symbol(token) => {
                        // Strip "/" for hybrid mana: "W/U" → "WU"
                        let key: String = token.chars().filter(|c| *c != '/').collect();
                        if let Some(tex) = symbol_textures.get(&key) {
                            ui.add(egui::Image::new(egui::load::SizedTexture::new(
                                tex.id(),
                                egui::vec2(sym_px, sym_px),
                            )));
                        } else {
                            ui.label(
                                RichText::new(format!("{{{token}}}")).font(mono.clone()),
                            );
                        }
                    }
                }
            }
        });
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn render_card_thread(
    record: CardRecord,
    data_dir: PathBuf,
    symbols_dir: PathBuf,
) -> Result<Vec<u8>, String> {
    let client = Client::new();
    let paths = ensure_card(&client, &data_dir, &record).map_err(|e| e.to_string())?;
    let json_bytes = std::fs::read(&paths.json).map_err(|e| e.to_string())?;
    let card: Card = serde_json::from_slice(&json_bytes).map_err(|e| e.to_string())?;
    let rails_dir = {
        let p = data_dir.join("_meta").join("rails");
        if p.is_dir() { Some(p) } else { None }
    };
    let opts = RenderOptions {
        dpi: Dpi(300.0),
        fonts_dir: None,
        symbols_dir: if symbols_dir.is_dir() { Some(symbols_dir) } else { None },
        rails_dir,
        card_style: CardStyle::Basic,
    };
    render_png(&card, paths.art.as_deref(), &opts).map_err(|e| e.to_string())
}

fn load_texture(ctx: &egui::Context, png: &[u8]) -> Result<egui::TextureHandle, String> {
    let img = image::load_from_memory(png).map_err(|e| e.to_string())?;
    let rgba = img.to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    let color_image = egui::ColorImage::from_rgba_unmultiplied([w, h], rgba.as_raw());
    Ok(ctx.load_texture("card_render", color_image, egui::TextureOptions::LINEAR))
}

/// Rasterize a single SVG file to a square egui texture of `size × size` px.
fn load_svg_texture(
    ctx: &egui::Context,
    path: &Path,
    size: u32,
    name: &str,
) -> Option<egui::TextureHandle> {
    let data = std::fs::read(path).ok()?;
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(&data, &opt).ok()?;
    let svg = tree.size();
    let scale_x = size as f32 / svg.width();
    let scale_y = size as f32 / svg.height();
    let scale = scale_x.min(scale_y);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let color_image =
        egui::ColorImage::from_rgba_unmultiplied([size as usize, size as usize], pixmap.data());
    Some(ctx.load_texture(format!("sym_{name}"), color_image, egui::TextureOptions::LINEAR))
}

fn setup_fonts(cc: &eframe::CreationContext<'_>) {
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        PathBuf::from("fonts/GoogleCodeSans-Regular.ttf"),
        PathBuf::from("fonts/GoogleCodeSans.ttf"),
        PathBuf::from(format!("{home}/Library/Fonts/GoogleCodeSans-Regular.ttf")),
        PathBuf::from("/Library/Fonts/GoogleCodeSans-Regular.ttf"),
    ];
    let mut fonts = egui::FontDefinitions::default();
    for path in &candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "google_code_sans".to_owned(),
                egui::FontData::from_owned(bytes).into(),
            );
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "google_code_sans".to_owned());
            break;
        }
    }
    cc.egui_ctx.set_fonts(fonts);
}

// ── main ─────────────────────────────────────────────────────────────────────

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("mechoe — card generator"),
        ..Default::default()
    };
    eframe::run_native(
        "mechoe",
        options,
        Box::new(|cc| {
            setup_fonts(cc);
            Ok(Box::new(CardApp::new()))
        }),
    )
}
