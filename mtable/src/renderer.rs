use egui::{Color32, Painter, Pos2, Rect, Vec2};
use crate::app::Playmat;
use crate::card::{w2s, w2s_rect};
use crate::constants::*;

pub fn draw(state: &mut Playmat, ctx: &egui::Context) {
    // CentralPanel is declared first; TopBottomPanel is declared after so it
    // renders on top of the play area (intentional z-ordering for the toolbar).
    egui::CentralPanel::default().show(ctx, |ui| {
        let painter = ui.painter();
        let rect = ui.max_rect();

        painter.rect_filled(rect, 0.0, COLOR_PLAYMAT_BG);

        draw_deck_bag(painter, state.zoom, state.offset);
        draw_default_tile(painter, state.zoom, state.offset);
        draw_grid(painter, &rect, state.zoom, state.offset);
        draw_snap_indicator(painter, state);
        draw_selection_rect(painter, state);
        draw_objects(painter, state);
    });

    draw_top_bar(state, ctx);
}

fn draw_deck_bag(painter: &Painter, zoom: f32, offset: Vec2) {
    let deck_rect = w2s_rect(Rect::from_min_size(DECK_POS, Vec2::new(CARD_W, CARD_H)), zoom, offset);
    painter.rect_filled(deck_rect, ROUNDING_DEFAULT, COLOR_DECK);
    painter.rect_stroke(deck_rect, ROUNDING_DEFAULT, (STROKE_WIDTH_DEFAULT * zoom, COLOR_DEFAULT_STROKE));
    painter.text(deck_rect.center(), egui::Align2::CENTER_CENTER, "DECK",
        egui::FontId::proportional(FONT_SIZE_CARD * zoom), Color32::WHITE);

    let bag_rect = w2s_rect(Rect::from_min_size(BAG_POS, Vec2::new(COUNTER_W, COUNTER_H)), zoom, offset);
    painter.rect_filled(bag_rect, ROUNDING_DEFAULT, COLOR_BAG);
    painter.rect_stroke(bag_rect, ROUNDING_DEFAULT, (STROKE_WIDTH_DEFAULT * zoom, COLOR_DEFAULT_STROKE));
    painter.text(bag_rect.center(), egui::Align2::CENTER_CENTER, "BAG",
        egui::FontId::proportional(FONT_SIZE_CARD * zoom), Color32::WHITE);
}

fn draw_default_tile(painter: &Painter, zoom: f32, offset: Vec2) {
    let tile = w2s_rect(Rect::from_min_size(DEFAULT_CARD_POS, Vec2::new(GRID_W, GRID_H)), zoom, offset);
    painter.rect_filled(tile, ROUNDING_DEFAULT, COLOR_DEFAULT_TILE);
}

fn draw_grid(painter: &Painter, screen_rect: &Rect, zoom: f32, offset: Vec2) {
    let world_min = (screen_rect.min - offset) / zoom;
    let world_max = (screen_rect.max - offset) / zoom;

    let x0 = (world_min.x / GRID_W).floor() as i32;
    let x1 = (world_max.x / GRID_W).ceil() as i32;
    for i in x0..=x1 {
        let sx = i as f32 * GRID_W * zoom + offset.x;
        painter.line_segment(
            [Pos2::new(sx, screen_rect.min.y), Pos2::new(sx, screen_rect.max.y)],
            (STROKE_WIDTH_DEFAULT, COLOR_GRID_LINE),
        );
    }

    let y0 = (world_min.y / GRID_H).floor() as i32;
    let y1 = (world_max.y / GRID_H).ceil() as i32;
    for j in y0..=y1 {
        let sy = j as f32 * GRID_H * zoom + offset.y;
        painter.line_segment(
            [Pos2::new(screen_rect.min.x, sy), Pos2::new(screen_rect.max.x, sy)],
            (STROKE_WIDTH_DEFAULT, COLOR_GRID_LINE),
        );
    }
}

fn draw_snap_indicator(painter: &Painter, state: &Playmat) {
    let Some(drag) = &state.drag else { return };
    if drag.is_group { return }
    let Some(obj) = state.objects.iter().find(|o| o.id() == drag.lead_id) else { return };
    let Some(snap_pos) = obj.snap_target() else { return };

    let screen_snap = w2s(snap_pos, state.zoom, state.offset);
    let half = Vec2::new(CARD_W, CARD_H) * state.zoom * 0.5;
    let snap_rect = Rect::from_center_size(screen_snap + half, Vec2::new(GRID_W, GRID_H) * state.zoom);
    painter.rect_filled(snap_rect, ROUNDING_DEFAULT, COLOR_SNAP_INDICATOR);
}

fn draw_selection_rect(painter: &Painter, state: &Playmat) {
    let (Some(start), Some(end)) = (state.selection_start, state.selection_end) else { return };
    let min_p = Pos2::new(start.x.min(end.x), start.y.min(end.y));
    let max_p = Pos2::new(start.x.max(end.x), start.y.max(end.y));
    let rect = w2s_rect(Rect::from_min_max(min_p, max_p), state.zoom, state.offset);
    painter.rect_filled(rect, 0.0, COLOR_SELECTION_FILL);
    painter.rect_stroke(rect, 0.0, (STROKE_WIDTH_DEFAULT, COLOR_SELECTION_STROKE));
}

fn draw_objects(painter: &Painter, state: &Playmat) {
    let mut indices: Vec<usize> = (0..state.objects.len()).collect();
    indices.sort_unstable_by_key(|&i| state.objects[i].z());
    for i in indices {
        let obj = &state.objects[i];
        obj.draw(painter, state.zoom, state.offset, state.selected_ids.contains(&obj.id()));
    }
}

fn draw_top_bar(state: &mut Playmat, ctx: &egui::Context) {
    egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        ui.horizontal(|ui| {
            styled_text_input(ui, &mut state.new_card_label, COLOR_DECK_INPUT_BG);
            styled_text_input(ui, &mut state.new_counter_label, COLOR_BAG_INPUT_BG);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Save").clicked() {
                    state.save();
                }
                if ui.button("Load").clicked() {
                    state.load();
                }
                ui.label(format!(
                    "{}/{} @{:.2}x",
                    state.selected_ids.len(),
                    state.objects.len(),
                    state.zoom
                ));
            });
        });
    });
}

fn styled_text_input(ui: &mut egui::Ui, text: &mut String, bg: Color32) {
    ui.scope(|ui| {
        let v = ui.visuals_mut();
        // TextEdit uses extreme_bg_color for its fill; widgets.*.bg_fill applies to buttons.
        v.extreme_bg_color = bg;
        v.widgets.inactive.bg_fill = bg;
        v.widgets.hovered.bg_fill = bg;
        v.widgets.active.bg_fill = bg;
        v.override_text_color = Some(Color32::BLACK);
        ui.text_edit_singleline(text);
    });
}
