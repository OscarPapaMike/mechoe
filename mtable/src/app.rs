use eframe::egui;
use egui::{PointerButton, Pos2, Rect, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use crate::card::{BoardObject, Counter, DragState, EntityId};
use crate::constants::*;
use crate::deck::Deck;
use crate::renderer;

// Persisted state written to / read from mtable_save.json.
#[derive(Serialize, Deserialize)]
struct SaveState {
    objects: Vec<BoardObject>,
    next_id: u64,
    next_z: u64,
}

pub struct Playmat {
    pub objects: Vec<BoardObject>,
    pub deck: Deck,
    pub drag: Option<DragState>,
    pub next_id: EntityId,
    pub next_z: u64,
    pub selection_start: Option<Pos2>,
    pub selection_end: Option<Pos2>,
    pub selected_ids: HashSet<EntityId>,
    pub new_card_label: String,
    pub new_counter_label: String,
    pub offset: Vec2,
    pub zoom: f32,
}

impl Playmat {
    pub fn screen_to_world(&self, pos: Pos2) -> Pos2 {
        (pos - self.offset) / self.zoom
    }

    pub fn save(&self) {
        #[derive(Serialize)]
        struct S<'a> {
            objects: &'a Vec<BoardObject>,
            next_id: u64,
            next_z: u64,
        }
        let state = S { objects: &self.objects, next_id: self.next_id.0, next_z: self.next_z };
        if let Ok(json) = serde_json::to_string_pretty(&state) {
            let _ = std::fs::write("mtable_save.json", json);
        }
    }

    pub fn load(&mut self) {
        if let Ok(json) = std::fs::read_to_string("mtable_save.json") {
            if let Ok(state) = serde_json::from_str::<SaveState>(&json) {
                self.objects = state.objects;
                self.next_id = EntityId(state.next_id);
                self.next_z = state.next_z;
                self.selected_ids.clear();
                self.drag = None;
            }
        }
    }

    // --- input handling ---

    fn handle_input(&mut self, ctx: &egui::Context) {
        let mouse_screen = ctx.input(|i| i.pointer.hover_pos());
        let world_pos = mouse_screen.map(|p| self.screen_to_world(p));
        let wants_ui = ctx.wants_pointer_input();

        self.handle_zoom(ctx, mouse_screen);
        self.handle_pan(ctx);
        self.handle_keyboard(ctx);
        self.handle_drag_release(ctx);

        if !wants_ui {
            self.handle_double_click(ctx, world_pos);
            self.handle_selection(ctx, world_pos);
            self.handle_drag_start(ctx, world_pos);
            self.handle_drag_movement(world_pos);
        }
    }

    fn handle_zoom(&mut self, ctx: &egui::Context, mouse_screen: Option<Pos2>) {
        let delta = ctx.input(|i| i.raw_scroll_delta.y);
        if delta == 0.0 { return; }
        let old_zoom = self.zoom;
        self.zoom = (self.zoom * (delta * ZOOM_SENSITIVITY).exp()).clamp(ZOOM_MIN, ZOOM_MAX);
        if let Some(p) = mouse_screen {
            let world_p = self.screen_to_world(p);
            self.offset += world_p.to_vec2() * (old_zoom - self.zoom);
        }
    }

    fn handle_pan(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.pointer.button_down(PointerButton::Middle)) {
            self.offset += ctx.input(|i| i.pointer.delta());
        }
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(egui::Key::Q) || i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
            self.objects.retain(|obj| !self.selected_ids.contains(&obj.id()));
            self.selected_ids.clear();
            self.drag = None;
        }
    }

    fn handle_drag_release(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.pointer.any_released()) { return; }

        // Extract what we need before dropping the borrow on self.drag.
        let snap_info = self.drag.as_ref().and_then(|state| {
            if state.is_group { return None; }
            let dragged_ids: HashSet<EntityId> = state.offsets.iter().map(|(id, _)| *id).collect();
            Some((state.lead_id, dragged_ids))
        });

        if let Some((lead_id, dragged_ids)) = snap_info {
            // Capture pre-snap position, then snap in-place.
            let pre_snap_pos = self.objects.iter().find(|o| o.id() == lead_id).map(|o| o.pos());
            let snap_delta = self.objects.iter_mut().find(|o| o.id() == lead_id).and_then(|o| o.try_snap());

            // Move non-dragged counters that were sitting on the card before it snapped.
            if let (Some(delta), Some(old_pos)) = (snap_delta, pre_snap_pos) {
                let old_bounds = Rect::from_min_size(old_pos, Vec2::new(CARD_W, CARD_H));
                for obj in &mut self.objects {
                    if let Some(counter) = obj.as_counter_mut() {
                        if !dragged_ids.contains(&counter.id) && old_bounds.contains(counter.pos) {
                            counter.pos += delta;
                        }
                    }
                }
            }
        }

        self.drag = None;
    }

    fn handle_double_click(&mut self, ctx: &egui::Context, world_pos: Option<Pos2>) {
        if !ctx.input(|i| i.pointer.button_double_clicked(PointerButton::Primary)) { return; }
        let Some(pos) = world_pos else { return };
        if let Some(obj) = self.objects.iter_mut()
            .filter(|o| o.rect().contains(pos))
            .max_by_key(|o| o.z())
        {
            obj.tap();
        }
    }

    fn handle_selection(&mut self, ctx: &egui::Context, world_pos: Option<Pos2>) {
        let Some(pos) = world_pos else { return };

        if ctx.input(|i| i.pointer.button_pressed(PointerButton::Secondary)) {
            self.selection_start = Some(pos);
            self.selection_end = Some(pos);
            self.selected_ids.clear();
        }
        if ctx.input(|i| i.pointer.button_down(PointerButton::Secondary)) {
            self.selection_end = Some(pos);
        }
        if ctx.input(|i| i.pointer.button_released(PointerButton::Secondary)) {
            if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
                let min_p = Pos2::new(start.x.min(end.x), start.y.min(end.y));
                let max_p = Pos2::new(start.x.max(end.x), start.y.max(end.y));
                let sel_rect = Rect::from_min_max(min_p, max_p);
                self.selected_ids = self.objects.iter()
                    .filter(|obj| sel_rect.intersects(obj.rect()))
                    .map(|obj| obj.id())
                    .collect();
            }
            self.selection_start = None;
            self.selection_end = None;
        }
    }

    fn handle_drag_start(&mut self, ctx: &egui::Context, world_pos: Option<Pos2>) {
        let Some(pos) = world_pos else { return };
        if !ctx.input(|i| i.pointer.button_pressed(PointerButton::Primary)) || self.drag.is_some() {
            return;
        }

        let deck_rect = Rect::from_min_size(DECK_POS, Vec2::new(CARD_W, CARD_H));
        let bag_rect = Rect::from_min_size(BAG_POS, Vec2::new(COUNTER_W, COUNTER_H));

        if deck_rect.contains(pos) {
            self.spawn_card(pos);
        } else if bag_rect.contains(pos) {
            self.spawn_counter(pos);
        } else {
            self.begin_object_drag(pos);
        }
    }

    fn spawn_card(&mut self, cursor: Pos2) {
        let label = if self.new_card_label.is_empty() {
            format!("Card {}", self.next_id.0)
        } else {
            self.new_card_label.clone()
        };
        let mut card = self.deck.draw(self.next_id);
        card.label = label;
        card.pos = cursor - Vec2::new(CARD_W / 2.0, CARD_H / 2.0);
        card.z = self.next_z;
        self.next_z += 1;
        let id = card.id;
        self.objects.push(BoardObject::Card(card));
        self.next_id.0 += 1;
        self.drag = Some(DragState {
            lead_id: id,
            is_group: false,
            offsets: vec![(id, Vec2::new(-CARD_W / 2.0, -CARD_H / 2.0))],
        });
    }

    fn spawn_counter(&mut self, cursor: Pos2) {
        let label = if self.new_counter_label.is_empty() {
            format!("Counter {}", self.next_id.0)
        } else {
            self.new_counter_label.clone()
        };
        let counter = Counter {
            id: self.next_id,
            pos: cursor - Vec2::new(COUNTER_W / 2.0, COUNTER_H / 2.0),
            label,
            color: COLOR_COUNTER_DEFAULT,
            z: self.next_z,
        };
        self.next_z += 1;
        let id = counter.id;
        self.objects.push(BoardObject::Counter(counter));
        self.next_id.0 += 1;
        self.drag = Some(DragState {
            lead_id: id,
            is_group: false,
            offsets: vec![(id, Vec2::new(-COUNTER_W / 2.0, -COUNTER_H / 2.0))],
        });
    }

    fn begin_object_drag(&mut self, cursor: Pos2) {
        // Pick the topmost object (highest z) under the cursor.
        let Some(lead_id) = self.objects.iter()
            .filter(|o| o.rect().contains(cursor))
            .max_by_key(|o| o.z())
            .map(|o| o.id())
        else { return };

        let is_group = self.selected_ids.contains(&lead_id);
        if !is_group {
            self.selected_ids.clear();
        }

        let mut ids: Vec<EntityId> = if is_group {
            self.selected_ids.iter().copied().collect()
        } else {
            vec![lead_id]
        };

        // Auto-attach counters sitting on any card being dragged.
        let mut extra: HashSet<EntityId> = HashSet::new();
        for id in &ids {
            if let Some(card) = self.objects.iter().find(|o| o.id() == *id).and_then(|o| o.as_card()) {
                let card_rect = Rect::from_min_size(card.pos, Vec2::new(CARD_W, CARD_H));
                for obj in &self.objects {
                    if let Some(counter) = obj.as_counter() {
                        if card_rect.contains(counter.pos) {
                            extra.insert(counter.id);
                        }
                    }
                }
            }
        }
        for cid in extra {
            if !ids.contains(&cid) {
                ids.push(cid);
            }
        }

        let offsets: Vec<(EntityId, Vec2)> = ids.iter()
            .filter_map(|id| self.objects.iter().find(|o| o.id() == *id).map(|o| (*id, o.pos() - cursor)))
            .collect();

        // Bump z of all dragged objects so they render on top (replaces splice).
        for id in &ids {
            if let Some(obj) = self.objects.iter_mut().find(|o| o.id() == *id) {
                obj.set_z(self.next_z);
                self.next_z += 1;
            }
        }

        self.drag = Some(DragState { lead_id, is_group, offsets });
    }

    fn handle_drag_movement(&mut self, world_pos: Option<Pos2>) {
        let Some(pos) = world_pos else { return };
        let Some(state) = &self.drag else { return };
        let updates: Vec<(EntityId, Pos2)> = state.offsets.iter()
            .map(|(id, off)| (*id, pos + *off))
            .collect();
        for (id, new_pos) in updates {
            if let Some(obj) = self.objects.iter_mut().find(|o| o.id() == id) {
                obj.set_pos(new_pos);
            }
        }
    }
}

impl Default for Playmat {
    fn default() -> Self {
        Self {
            objects: Vec::new(),
            deck: Deck::new(),
            drag: None,
            next_id: EntityId(0),
            next_z: 0,
            selection_start: None,
            selection_end: None,
            selected_ids: HashSet::new(),
            new_card_label: String::new(),
            new_counter_label: String::new(),
            offset: Vec2::ZERO,
            zoom: 1.0,
        }
    }
}

impl eframe::App for Playmat {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_input(ctx);
        renderer::draw(self, ctx);
    }
}
