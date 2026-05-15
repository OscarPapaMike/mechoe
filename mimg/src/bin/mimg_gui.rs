use eframe::egui::{self, ColorImage, Pos2, Rect, Sense, TextureHandle, TextureOptions};
use image::RgbImage;
use rustfft::{num_complex::Complex, FftPlanner};
use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: mimg-gui <image-path>");
        std::process::exit(1);
    }
    let path = PathBuf::from(&args[1]);

    eframe::run_native(
        "halftone filter",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("halftone filter")
                .with_inner_size([1600.0, 620.0]),
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(App::new(cc, path)))),
    )
}

// ── State ─────────────────────────────────────────────────────────────────────

struct Notch {
    fx: f32, // cyc/px, canonical (+x side of spectrum)
    fy: f32,
    sigma: f32, // Gaussian sigma in cyc/px
}

struct App {
    w: usize,
    h: usize,
    rgb_freq: [Vec<Complex<f32>>; 3], // pre-computed forward FFT per channel
    notches: Vec<Notch>,
    default_sigma: f32,
    fft_tex: TextureHandle,      // static — only built once
    filtered_tex: TextureHandle, // rebuilt on every notch change
    original_tex: TextureHandle, // static
    dirty: bool,
    planner: FftPlanner<f32>,
    drag_idx: Option<usize>, // notch being dragged
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, path: PathBuf) -> Self {
        let img = image::open(&path).expect("cannot open image").into_rgb8();
        let (w, h) = (img.width() as usize, img.height() as usize);

        let mut planner = FftPlanner::<f32>::new();
        let rgb_freq: [Vec<Complex<f32>>; 3] = std::array::from_fn(|c| {
            let mut data: Vec<Complex<f32>> = img
                .pixels()
                .map(|p| Complex::new(p[c] as f32 / 255.0, 0.0))
                .collect();
            fft2d(&mut data, w, h, true, &mut planner);
            data
        });

        let fft_tex = cc.egui_ctx.load_texture(
            "fft",
            spectrum_color_image(&rgb_freq[1], w, h),
            TextureOptions::NEAREST,
        );
        let original_tex = cc.egui_ctx.load_texture(
            "original",
            rgb_to_color_image(&img),
            TextureOptions::NEAREST,
        );
        let filtered_tex = cc.egui_ctx.load_texture(
            "filtered",
            rgb_to_color_image(&img),
            TextureOptions::NEAREST,
        );

        Self {
            w,
            h,
            rgb_freq,
            notches: Vec::new(),
            default_sigma: 0.026,
            fft_tex,
            filtered_tex,
            original_tex,
            dirty: false,
            planner,
            drag_idx: None,
        }
    }

    fn rebuild_filtered(&mut self, ctx: &egui::Context) {
        let mask = build_mask(&self.notches, self.w, self.h);
        let out = apply_mask_rgb(&self.rgb_freq, self.w, self.h, &mask, &mut self.planner);
        self.filtered_tex = ctx.load_texture(
            "filtered",
            rgb_to_color_image(&out),
            TextureOptions::NEAREST,
        );
        self.dirty = false;
    }
}

// ── egui app ──────────────────────────────────────────────────────────────────

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.dirty {
            self.rebuild_filtered(ctx);
        }

        // ── Top toolbar ───────────────────────────────────────────────────────
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Auto").on_hover_text("Detect halftone ring frequencies and populate notches").clicked() {
                    self.notches = auto_detect_notches(
                        &self.rgb_freq, self.w, self.h, self.default_sigma,
                    );
                    self.dirty = true;
                }
                if ui.button("Clear").clicked() {
                    self.notches.clear();
                    self.dirty = true;
                }
                ui.separator();
                ui.label("Radius:");
                ui.add(
                    egui::DragValue::new(&mut self.default_sigma)
                        .speed(0.0005)
                        .range(0.005..=0.15)
                        .fixed_decimals(3),
                );
                ui.label("cyc/px");
                ui.separator();
                ui.label(format!("{} notches", self.notches.len()));
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let avail = ui.available_size();
            let col_w = (avail.x / 3.0).floor();
            let img_h = avail.y - 24.0; // room for label

            ui.horizontal(|ui| {
                // ── Column 1: FFT (interactive) ───────────────────────────────
                ui.vertical(|ui| {
                    ui.set_width(col_w);
                    ui.label("FFT  [L: add notch | R: remove | scroll: resize]");

                    let tex_sz = self.fft_tex.size_vec2();
                    let scale = (col_w / tex_sz.x).min(img_h / tex_sz.y);
                    let disp = tex_sz * scale;

                    let (rect, resp) =
                        ui.allocate_exact_size(disp, Sense::click_and_drag());
                    ui.painter().image(
                        self.fft_tex.id(),
                        rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );

                    // Draw notch circles (canonical + conjugate mirror)
                    for notch in &self.notches {
                        let r_px = (notch.sigma * rect.width().min(rect.height()) * 3.0).max(4.0);
                        let stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 80, 80));
                        for sign in [1.0f32, -1.0] {
                            let cx = rect.center().x + sign * notch.fx * rect.width();
                            let cy = rect.center().y + sign * notch.fy * rect.height();
                            ui.painter().circle_stroke(Pos2::new(cx, cy), r_px, stroke);
                        }
                    }

                    // Drag: start on press, move notch, finish on release
                    if resp.drag_started() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            let (fx, fy) = pos_to_cyc(pos, rect);
                            // Begin drag if pointer is inside a notch circle
                            self.drag_idx = self.notches.iter().enumerate().find(|(_, n)| {
                                let r_cyc = n.sigma * 3.0;
                                for sign in [1.0f32, -1.0] {
                                    let dx = fx - sign * n.fx;
                                    let dy = fy - sign * n.fy;
                                    if dx * dx + dy * dy <= r_cyc * r_cyc { return true; }
                                }
                                false
                            }).map(|(i, _)| i);
                        }
                    }
                    if resp.dragged() {
                        if let Some(idx) = self.drag_idx {
                            if let Some(pos) = resp.interact_pointer_pos() {
                                let (fx, fy) = pos_to_cyc(pos, rect);
                                let (fx, fy) = if fx >= 0.0 { (fx, fy) } else { (-fx, -fy) };
                                self.notches[idx].fx = fx;
                                self.notches[idx].fy = fy;
                                self.dirty = true;
                            }
                        }
                    }
                    if resp.drag_stopped() {
                        self.drag_idx = None;
                    }

                    // Left-click on empty space: add notch
                    if resp.clicked() && self.drag_idx.is_none() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            let (fx, fy) = pos_to_cyc(pos, rect);
                            // Only add if not clicking inside an existing notch
                            let inside_notch = self.notches.iter().any(|n| {
                                let r_cyc = n.sigma * 3.0;
                                for sign in [1.0f32, -1.0] {
                                    let dx = fx - sign * n.fx;
                                    let dy = fy - sign * n.fy;
                                    if dx * dx + dy * dy <= r_cyc * r_cyc { return true; }
                                }
                                false
                            });
                            if !inside_notch {
                                let (fx, fy) = if fx >= 0.0 { (fx, fy) } else { (-fx, -fy) };
                                self.notches.push(Notch { fx, fy, sigma: self.default_sigma });
                                self.dirty = true;
                            }
                        }
                    }

                    // Right-click: remove nearest
                    if resp.secondary_clicked() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            let (fx, fy) = pos_to_cyc(pos, rect);
                            if let Some(idx) = nearest(&self.notches, fx, fy) {
                                self.notches.remove(idx);
                                if self.drag_idx == Some(idx) { self.drag_idx = None; }
                                self.dirty = true;
                            }
                        }
                    }

                    // Scroll: resize nearest notch under cursor
                    if let Some(hover) = ctx.input(|i| i.pointer.hover_pos()) {
                        if rect.contains(hover) {
                            let dy = ctx.input(|i| i.raw_scroll_delta.y);
                            if dy.abs() > 0.1 {
                                let (fx, fy) = pos_to_cyc(hover, rect);
                                if let Some(idx) = nearest(&self.notches, fx, fy) {
                                    self.notches[idx].sigma *= 1.0 + dy * 0.003;
                                    self.notches[idx].sigma =
                                        self.notches[idx].sigma.clamp(0.004, 0.15);
                                    self.dirty = true;
                                }
                            }
                        }
                    }
                });

                ui.separator();

                // ── Column 2: Filtered output ─────────────────────────────────
                ui.vertical(|ui| {
                    ui.set_width(col_w);
                    ui.label("Filtered");
                    let tex_sz = self.filtered_tex.size_vec2();
                    let scale = (col_w / tex_sz.x).min(img_h / tex_sz.y);
                    ui.image((self.filtered_tex.id(), tex_sz * scale));
                });

                ui.separator();

                // ── Column 3: Original ────────────────────────────────────────
                ui.vertical(|ui| {
                    ui.label("Original");
                    let tex_sz = self.original_tex.size_vec2();
                    let scale = (col_w / tex_sz.x).min(img_h / tex_sz.y);
                    ui.image((self.original_tex.id(), tex_sz * scale));
                });
            });
        });
    }
}

// ── Auto-detection ────────────────────────────────────────────────────────────

/// Detect halftone screen frequencies from the green-channel FFT via ring analysis.
/// Builds a radial power profile, finds dominant ring(s) in the halftone band,
/// then picks angular local maxima on each ring.
fn sf(u: usize, n: usize) -> i32 {
    let (u, n) = (u as i32, n as i32);
    if u <= n / 2 { u } else { u - n }
}

/// Detect halftone screen frequencies using all three RGB channels.
/// R sees M+Y screens, G sees C+Y, B sees C+M — unioning finds the full CMYK set.
fn auto_detect_notches(
    rgb_freq: &[Vec<Complex<f32>>; 3],
    width: usize,
    height: usize,
    sigma: f32,
) -> Vec<Notch> {
    let merge_tol = 0.012f32;
    let mut merged: Vec<(f32, f32)> = Vec::new();
    for freq in rgb_freq.iter() {
        for (fx, fy) in ring_peaks_cyc(freq, width, height) {
            let dup = merged.iter().any(|&(ex, ey)| {
                (fx - ex).powi(2) + (fy - ey).powi(2) <= merge_tol * merge_tol
            });
            if !dup { merged.push((fx, fy)); }
        }
    }
    merged.into_iter().map(|(fx, fy)| Notch { fx, fy, sigma }).collect()
}

/// Ring-based peak detection for one FFT channel.
/// O(n) via angle-binning: bucket all ring candidates into 5° bins,
/// keep per-bin max, then NMS over ~72 representatives instead of all bins.
fn ring_peaks_cyc(freq: &[Complex<f32>], width: usize, height: usize) -> Vec<(f32, f32)> {
    let r_min = 1.0f32 / 3.5;
    let r_max = 0.70f32;
    let n_rbins = 400usize;
    let r_step = r_max / n_rbins as f32;

    // Radial mean log-power
    let mut rpower = vec![0.0f32; n_rbins];
    let mut rcount = vec![0usize; n_rbins];
    for i in 0..freq.len() {
        let sdx = sf(i % width, width);
        let sdy = sf(i / width, height);
        if sdx.abs() < 4 || sdy.abs() < 4 { continue; } // exclude axis-aligned (JPEG artefacts)
        let r = ((sdx as f32 / width as f32).powi(2)
            + (sdy as f32 / height as f32).powi(2)).sqrt();
        let ri = (r / r_step) as usize;
        if ri < n_rbins {
            rpower[ri] += freq[i].norm_sqr().ln_1p();
            rcount[ri] += 1;
        }
    }
    for i in 0..n_rbins { if rcount[i] > 0 { rpower[i] /= rcount[i] as f32; } }

    // Local-maximum rings above 50 % of band peak
    let bin_min = (r_min / r_step) as usize;
    let bin_max = ((r_max / r_step) as usize).min(n_rbins - 1);
    let band_peak = rpower[bin_min..=bin_max].iter().cloned().fold(0.0f32, f32::max);
    let threshold = band_peak * 0.50;

    let mut ring_radii: Vec<f32> = Vec::new();
    for i in bin_min..=bin_max {
        if rpower[i] < threshold { continue; }
        let prev = if i > bin_min { rpower[i - 1] } else { 0.0 };
        let next = if i < bin_max { rpower[i + 1] } else { 0.0 };
        if rpower[i] >= prev && rpower[i] >= next
            && ring_radii.last().map_or(true, |&r| (i as f32 * r_step - r).abs() > r_step * 5.0)
        {
            ring_radii.push(i as f32 * r_step);
        }
    }

    // Angular NMS: angle-bin → per-bin max → small NMS set
    const N_ABINS: usize = 72; // 5° resolution
    let angular_window = std::f32::consts::PI / 10.0; // ±18° half-window

    let mut seen: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    let mut out: Vec<(f32, f32)> = Vec::new();

    for ring_r in ring_radii {
        let tol = ring_r * 0.10;
        let mut abins: Vec<Option<(f32, f32, f32)>> = vec![None; N_ABINS];

        for i in 0..freq.len() {
            let sdx = sf(i % width, width);
            let sdy = sf(i / width, height);
            if sdx.abs() < 4 || sdy.abs() < 4 { continue; } // skip axis-aligned (JPEG artefacts)
            let fx = sdx as f32 / width as f32;
            let fy = sdy as f32 / height as f32;
            if ((fx * fx + fy * fy).sqrt() - ring_r).abs() > tol { continue; }
            let mag = freq[i].norm_sqr().ln_1p();
            let angle = fy.atan2(fx); // [-π, π]
            let bin = ((angle + std::f32::consts::PI) / std::f32::consts::TAU
                * N_ABINS as f32) as usize % N_ABINS;
            if abins[bin].map_or(true, |(m, _, _)| mag > m) {
                abins[bin] = Some((mag, fx, fy));
            }
        }

        let reps: Vec<(f32, f32, f32)> = abins.into_iter().flatten().collect();
        for &(mag, fx, fy) in &reps {
            let angle = fy.atan2(fx);
            let is_peak = reps.iter().all(|&(m2, fx2, fy2)| {
                angle_diff(angle, fy2.atan2(fx2)) >= angular_window || m2 <= mag
            });
            if !is_peak { continue; }
            let kx = (fx * 10_000.0).round() as i32;
            let ky = (fy * 10_000.0).round() as i32;
            let canon = if kx > 0 || (kx == 0 && ky > 0) { (kx, ky) } else { (-kx, -ky) };
            if seen.insert(canon) {
                out.push((canon.0 as f32 / 10_000.0, canon.1 as f32 / 10_000.0));
            }
        }
    }
    out
}

fn angle_diff(a: f32, b: f32) -> f32 {
    let d = (a - b).abs();
    if d > std::f32::consts::PI { 2.0 * std::f32::consts::PI - d } else { d }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Map a screen position inside `rect` to cycles/pixel (DC = center of rect).
fn pos_to_cyc(pos: Pos2, rect: Rect) -> (f32, f32) {
    (
        (pos.x - rect.center().x) / rect.width(),
        (pos.y - rect.center().y) / rect.height(),
    )
}

/// Index of the notch whose canonical position (or its conjugate mirror) is
/// closest to (fx, fy).
fn nearest(notches: &[Notch], fx: f32, fy: f32) -> Option<usize> {
    notches
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let d2 = |n: &Notch| {
                ((n.fx - fx).powi(2) + (n.fy - fy).powi(2))
                    .min((n.fx + fx).powi(2) + (n.fy + fy).powi(2))
            };
            d2(a).partial_cmp(&d2(b)).unwrap()
        })
        .map(|(i, _)| i)
}

// ── Mask & filtering ──────────────────────────────────────────────────────────

fn build_mask(notches: &[Notch], width: usize, height: usize) -> Vec<f32> {
    let mut mask = vec![1.0f32; width * height];
    let halfw = (width / 2) as i32;
    let halfh = (height / 2) as i32;
    for n in notches {
        let sigma_px = n.sigma * width.min(height) as f32;
        let s2 = sigma_px * sigma_px;
        for &(sfx, sfy) in &[(n.fx, n.fy), (-n.fx, -n.fy)] {
            let fdx = (sfx * width as f32).round() as i32;
            let fdy = (sfy * height as f32).round() as i32;
            if fdx.abs() > halfw || fdy.abs() > halfh { continue; }
            let ux = fdx.rem_euclid(width as i32) as usize;
            let uy = fdy.rem_euclid(height as i32) as usize;
            apply_gaussian_notch(&mut mask, width, height, ux, uy, s2);
        }
    }
    mask
}

fn apply_mask_rgb(
    rgb_freq: &[Vec<Complex<f32>>; 3],
    width: usize,
    height: usize,
    mask: &[f32],
    planner: &mut FftPlanner<f32>,
) -> RgbImage {
    let norm = (width * height) as f32;
    let channels: Vec<Vec<u8>> = rgb_freq
        .iter()
        .map(|freq| {
            let mut data = freq.clone();
            for (f, m) in data.iter_mut().zip(mask) {
                *f *= *m;
            }
            fft2d(&mut data, width, height, false, planner);
            data.iter()
                .map(|c| ((c.re / norm).clamp(0.0, 1.0) * 255.0 + 0.5) as u8)
                .collect()
        })
        .collect();

    let mut out = image::ImageBuffer::new(width as u32, height as u32);
    for (i, px) in out.pixels_mut().enumerate() {
        *px = image::Rgb([channels[0][i], channels[1][i], channels[2][i]]);
    }
    out
}

// ── Image → egui ─────────────────────────────────────────────────────────────

fn rgb_to_color_image(img: &RgbImage) -> ColorImage {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let pixels: Vec<egui::Color32> = img
        .pixels()
        .map(|p| egui::Color32::from_rgb(p[0], p[1], p[2]))
        .collect();
    ColorImage { size: [w, h], pixels }
}

fn spectrum_color_image(freq: &[Complex<f32>], width: usize, height: usize) -> ColorImage {
    let mut log_mag = vec![0.0f32; freq.len()];
    for (i, c) in freq.iter().enumerate() {
        // fftshift: move DC to center
        let sx = (i % width + width / 2) % width;
        let sy = (i / width + height / 2) % height;
        log_mag[sy * width + sx] = c.norm_sqr().ln_1p();
    }
    let max_v = log_mag.iter().cloned().fold(0.0f32, f32::max);
    let scale = if max_v > 0.0 { 255.0 / max_v } else { 1.0 };
    let pixels: Vec<egui::Color32> = log_mag
        .iter()
        .map(|&v| {
            let g = (v * scale) as u8;
            egui::Color32::from_gray(g)
        })
        .collect();
    ColorImage { size: [width, height], pixels }
}

// ── FFT helpers ───────────────────────────────────────────────────────────────

fn fft2d(
    data: &mut Vec<Complex<f32>>,
    width: usize,
    height: usize,
    forward: bool,
    planner: &mut FftPlanner<f32>,
) {
    let row_fft = if forward {
        planner.plan_fft_forward(width)
    } else {
        planner.plan_fft_inverse(width)
    };
    for row in data.chunks_mut(width) {
        row_fft.process(row);
    }
    *data = transpose(data, height, width);

    let col_fft = if forward {
        planner.plan_fft_forward(height)
    } else {
        planner.plan_fft_inverse(height)
    };
    for col in data.chunks_mut(height) {
        col_fft.process(col);
    }
    *data = transpose(data, width, height);
}

fn transpose(data: &[Complex<f32>], rows: usize, cols: usize) -> Vec<Complex<f32>> {
    let mut out = vec![Complex::new(0.0f32, 0.0); data.len()];
    for r in 0..rows {
        for c in 0..cols {
            out[c * rows + r] = data[r * cols + c];
        }
    }
    out
}

fn apply_gaussian_notch(
    mask: &mut [f32],
    width: usize,
    height: usize,
    px: usize,
    py: usize,
    sigma2: f32,
) {
    let radius = (sigma2.sqrt() * 3.0).ceil() as i32;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let d2 = (dx * dx + dy * dy) as f32;
            if d2 > sigma2 * 9.0 {
                continue;
            }
            let nx = (px as i32 + dx).rem_euclid(width as i32) as usize;
            let ny = (py as i32 + dy).rem_euclid(height as i32) as usize;
            mask[ny * width + nx] *= 1.0 - (-d2 / (2.0 * sigma2)).exp();
        }
    }
}
