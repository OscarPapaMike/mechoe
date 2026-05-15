use anyhow::{Context, Result};
use clap::Parser;
use image::{ImageBuffer, Rgb};
use rustfft::{num_complex::Complex, FftPlanner};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(about = "Halftone removal via FFT notch filtering for Magic card art")]
struct Cli {
    /// Path to the card list TOML (default: test_cards.toml in CWD)
    #[arg(long, default_value = "test_cards.toml")]
    config: PathBuf,

    #[arg(long, default_value = "../data")]
    data: PathBuf,

    #[arg(long, default_value = "out")]
    out: PathBuf,

    /// Gaussian notch sigma as fraction of min(width,height)
    #[arg(long, default_value_t = 0.026)]
    notch_radius: f64,

    /// Unique screen directions to notch per card (pairs de-duplicated)
    #[arg(long, default_value_t = 15)]
    num_fundamentals: usize,

    /// NMS window radius in frequency-domain pixels
    #[arg(long, default_value_t = 8)]
    nms_radius: usize,

    /// DC exclusion zone as fraction of min(width,height)
    #[arg(long, default_value_t = 0.12)]
    dc_exclude: f64,

    /// Cross-card consensus: use frequencies common to many cards instead of per-card detection
    #[arg(long)]
    consensus: bool,

    /// Fraction of cards a frequency must appear in to be treated as consensus halftone
    #[arg(long, default_value_t = 0.4)]
    consensus_fraction: f64,

    /// Clustering tolerance in cycles/pixel for consensus detection
    #[arg(long, default_value_t = 0.008)]
    consensus_tolerance: f64,

    /// Only treat peaks with period < this many pixels as halftone candidates (consensus mode)
    #[arg(long, default_value_t = 3.5)]
    max_period: f64,

    /// Save annotated spectrum PNG for each card
    #[arg(long)]
    spectra: bool,

    /// Process only this set (e.g. DRK)
    #[arg(long)]
    set: Option<String>,

    /// After consensus, save the print-run profile to this TOML file
    #[arg(long)]
    save_profile: Option<PathBuf>,

    /// Load a saved print-run profile instead of running consensus detection
    #[arg(long)]
    load_profile: Option<PathBuf>,
}

#[derive(Deserialize)]
struct Config {
    cards: Vec<CardEntry>,
}

#[derive(Deserialize, Clone)]
struct CardEntry {
    set: String,
    num: String,
    name: String,
    #[serde(default)]
    note: String,
}

struct FilterOpts {
    notch_radius: f32,
    num_fundamentals: usize,
    nms_radius: usize,
    dc_exclude: f32,
    max_period: f32, // only peaks with period < this (pixels) are treated as halftone
}

/// One halftone screen frequency entry.
#[derive(Serialize, Deserialize)]
struct FreqEntry {
    /// Frequency in cycles/pixel, x component (positive-x canonical form)
    fx: f32,
    /// Frequency in cycles/pixel, y component
    fy: f32,
}

/// Serialisable print-run halftone profile — compute once, apply to any card from the same set.
#[derive(Serialize, Deserialize)]
struct PrintProfile {
    /// Set code(s) this profile was calibrated on (informational only)
    #[serde(default)]
    calibration_set: String,
    /// Number of cards used to derive the consensus
    num_calibration_cards: usize,
    /// Consensus fraction used (fraction of cards that had to agree)
    consensus_fraction: f32,
    /// Halftone screen frequencies in cycles/pixel
    #[serde(rename = "frequency")]
    frequencies: Vec<FreqEntry>,
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config: Config = toml::from_str(
        &std::fs::read_to_string(&cli.config)
            .with_context(|| format!("could not read {}", cli.config.display()))?,
    )?;
    std::fs::create_dir_all(&cli.out)?;

    let opts = FilterOpts {
        notch_radius: cli.notch_radius as f32,
        num_fundamentals: cli.num_fundamentals,
        nms_radius: cli.nms_radius,
        dc_exclude: cli.dc_exclude as f32,
        max_period: cli.max_period as f32,
    };

    // ── Phase 1: load images, compute per-channel FFTs ────────────────────────
    struct CardData {
        entry: CardEntry,
        img: image::RgbImage,
        rgb_freq: [Vec<Complex<f32>>; 3],
        width: usize,
        height: usize,
    }

    let mut cards: Vec<CardData> = Vec::new();
    for entry in &config.cards {
        if let Some(ref s) = cli.set {
            if entry.set.to_lowercase() != s.to_lowercase() {
                continue;
            }
        }
        let src = cli.data.join(&entry.set).join(format!("{}.jpg", entry.num));
        if !src.exists() {
            eprintln!("skip  {}/{}: art not found", entry.set, entry.num);
            continue;
        }
        let img = image::open(&src)
            .with_context(|| format!("failed to open {}", src.display()))?
            .into_rgb8();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let rgb_freq = {
            let mut ch: [Vec<Complex<f32>>; 3] = [Vec::new(), Vec::new(), Vec::new()];
            for c in 0..3usize {
                let mut freq: Vec<Complex<f32>> = img
                    .pixels()
                    .map(|p| Complex::new(p[c] as f32 / 255.0, 0.0))
                    .collect();
                fft2d(&mut freq, w, h, true);
                ch[c] = freq;
            }
            ch
        };
        cards.push(CardData { entry: entry.clone(), img, rgb_freq, width: w, height: h });
    }

    // ── Phase 2: establish halftone frequencies (consensus or loaded profile) ───
    let consensus_cyc: Option<Vec<(f32, f32)>> = if let Some(ref profile_path) = cli.load_profile {
        // Load a previously saved print-run profile
        let toml_str = std::fs::read_to_string(profile_path)
            .with_context(|| format!("could not read profile {}", profile_path.display()))?;
        let profile: PrintProfile = toml::from_str(&toml_str)
            .with_context(|| format!("could not parse profile {}", profile_path.display()))?;
        let freqs: Vec<(f32, f32)> = profile.frequencies.iter().map(|e| (e.fx, e.fy)).collect();
        println!(
            "loaded profile '{}': {} frequencies (calibrated on {} cards, fraction {:.2})",
            profile_path.display(),
            freqs.len(),
            profile.num_calibration_cards,
            profile.consensus_fraction,
        );
        for &(fx, fy) in &freqs {
            let period = 1.0 / (fx * fx + fy * fy).sqrt();
            let angle = fy.atan2(fx).to_degrees();
            println!("  ({:+.4}, {:+.4}) cyc/px  period {:.2}px  angle {:+.1}°", fx, fy, period, angle);
        }
        Some(freqs)

    } else if cli.consensus || cli.save_profile.is_some() {
        // Collect peaks from all 3 channels per card, union within each card.
        // This ensures frequencies that only appear in R or B (not G) are found.
        let all_peaks: Vec<Vec<(f32, f32)>> = cards
            .iter()
            .map(|cd| {
                let merge_tol = cli.consensus_tolerance as f32;
                let mut merged: Vec<(f32, f32)> = Vec::new();
                for ch in 0..3usize {
                    for (fx, fy) in collect_peaks_cyc(&cd.rgb_freq[ch], cd.width, cd.height, &opts) {
                        let dup = merged.iter().any(|&(ex, ey)|
                            (fx - ex).powi(2) + (fy - ey).powi(2) <= merge_tol * merge_tol);
                        if !dup { merged.push((fx, fy)); }
                    }
                }
                merged
            })
            .collect();

        let freqs = find_consensus_peaks(
            &all_peaks,
            cli.consensus_fraction as f32,
            cli.consensus_tolerance as f32,
        );

        println!("consensus: {} shared halftone frequencies across {} cards:", freqs.len(), cards.len());
        for &(fx, fy) in &freqs {
            let period = 1.0 / (fx * fx + fy * fy).sqrt();
            let angle = fy.atan2(fx).to_degrees();
            println!("  ({:+.4}, {:+.4}) cyc/px  period {:.2}px  angle {:+.1}°", fx, fy, period, angle);
        }

        // Show ring radii detected per card for diagnostics
        if !cards.is_empty() {
            let cd = &cards[0];
            let rings = find_dominant_rings(&cd.rgb_freq[1], cd.width, cd.height, &opts);
            if !rings.is_empty() {
                let ring_strs: Vec<String> = rings.iter()
                    .map(|&r| format!("{:.1}px ({:.4}cyc/px)", r, r / cd.width.min(cd.height) as f32))
                    .collect();
                println!("  ring radii (first card sample): {}", ring_strs.join(", "));
            }
        }

        // Optionally persist this profile for future reuse
        if let Some(ref out_path) = cli.save_profile {
            let profile = PrintProfile {
                calibration_set: cli.set.clone().unwrap_or_default(),
                num_calibration_cards: cards.len(),
                consensus_fraction: cli.consensus_fraction as f32,
                frequencies: freqs.iter().map(|&(fx, fy)| FreqEntry { fx, fy }).collect(),
            };
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            // Build readable TOML by hand: header fields first, then [[frequency]] sections
            // with inline period/angle comments so the file is self-documenting.
            let mut out = format!(
                "calibration_set = {:?}\nnum_calibration_cards = {}\nconsensus_fraction = {}\n",
                profile.calibration_set, profile.num_calibration_cards, profile.consensus_fraction,
            );
            for e in &profile.frequencies {
                let period = 1.0 / (e.fx * e.fx + e.fy * e.fy).sqrt();
                let angle = e.fy.atan2(e.fx).to_degrees();
                out += &format!(
                    "\n# period {:.2}px  angle {:+.1}°\n[[frequency]]\nfx = {:.7}\nfy = {:.7}\n",
                    period, angle, e.fx, e.fy,
                );
            }
            std::fs::write(out_path, &out)
                .with_context(|| format!("could not write profile {}", out_path.display()))?;
            println!("saved profile → {}", out_path.display());
        }

        if cli.consensus { Some(freqs) } else { None }
    } else {
        None
    };

    // ── Phase 3: filter and save ───────────────────────────────────────────────
    for cd in &cards {
        let label = if cd.entry.note.is_empty() {
            format!("{}/{}", cd.entry.set, cd.entry.num)
        } else {
            format!("{}/{} — {}", cd.entry.set, cd.entry.num, cd.entry.note)
        };
        println!("processing {}  \"{}\"", label, cd.entry.name);
        println!("  image: {}×{}", cd.width, cd.height);

        // Build per-channel notch masks.
        // In consensus mode: common frequencies applied to all channels uniformly.
        // In per-card mode: each channel gets its own mask built from its own peaks.
        let (masks, reported_freqs): ([Vec<f32>; 3], Vec<(i32, i32)>) =
            if let Some(ref cyc) = consensus_cyc {
                // Convert cyc/px → FFT bins for the diagnostic print
                let bins: Vec<(i32, i32)> = cyc
                    .iter()
                    .map(|&(fx, fy)| {
                        ((fx * cd.width as f32).round() as i32,
                         (fy * cd.height as f32).round() as i32)
                    })
                    .collect();
                // Use the green channel FFT for per-peak SNR estimation (representative channel)
                let m = build_mask_from_freqs_cyc(cd.width, cd.height, cyc, &opts, Some(&cd.rgb_freq[1]));
                ([m.clone(), m.clone(), m], bins)
            } else {
                // Per-card: detect and mask each channel independently
                let mut all_bins: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
                let channel_masks: Vec<Vec<f32>> = (0..3usize)
                    .map(|c| {
                        let (m, bins) = build_notch_mask(&cd.rgb_freq[c], cd.width, cd.height, &opts);
                        for b in &bins { all_bins.insert(*b); }
                        m
                    })
                    .collect();
                let reported: Vec<(i32, i32)> = all_bins.into_iter().collect();
                ([channel_masks[0].clone(), channel_masks[1].clone(), channel_masks[2].clone()], reported)
            };

        // Print which fundamentals are being applied
        let halfw = (cd.width / 2) as i32;
        let halfh = (cd.height / 2) as i32;
        let mut sorted_freqs = reported_freqs.clone();
        sorted_freqs.sort_by_key(|&(dx, dy)| -(dx * dx + dy * dy));
        println!("  {} fundamentals applied:", sorted_freqs.len());
        for (fdx, fdy) in &sorted_freqs {
            let n_harm = {
                let mx = if *fdx != 0 { halfw / fdx.abs() } else { i32::MAX };
                let my = if *fdy != 0 { halfh / fdy.abs() } else { i32::MAX };
                mx.min(my).min(64)
            };
            let px = if *fdx != 0 { cd.width as f32 / fdx.abs() as f32 } else { f32::INFINITY };
            let py = if *fdy != 0 { cd.height as f32 / fdy.abs() as f32 } else { f32::INFINITY };
            println!("    ({:+5},{:+5})  period≈({:.1}px,{:.1}px)  harmonics:{}", fdx, fdy, px, py, n_harm);
        }

        // Filter each channel with its own mask
        let filtered: Vec<Vec<f32>> = (0..3)
            .map(|c| {
                let ch: Vec<f32> = cd.img.pixels().map(|p| p[c] as f32 / 255.0).collect();
                apply_mask_channel(&ch, cd.width, cd.height, &masks[c])
            })
            .collect();

        // Save original + filtered
        let mut out_img: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::new(cd.width as u32, cd.height as u32);
        for (i, pixel) in out_img.pixels_mut().enumerate() {
            *pixel = Rgb([
                (filtered[0][i].clamp(0.0, 1.0) * 255.0) as u8,
                (filtered[1][i].clamp(0.0, 1.0) * 255.0) as u8,
                (filtered[2][i].clamp(0.0, 1.0) * 255.0) as u8,
            ]);
        }
        let stem = format!("{}-{}", cd.entry.set, cd.entry.num);
        cd.img.save(cli.out.join(format!("{}_orig.jpg", stem)))?;
        out_img.save(cli.out.join(format!("{}_filtered.jpg", stem)))?;

        if cli.spectra {
            save_spectrum(
                &cd.rgb_freq[1], // green channel for display
                cd.width,
                cd.height,
                &masks[1],
                &cli.out.join(format!("{}_spectrum.png", stem)),
            )?;
        }

        println!(
            "  -> {}_filtered.jpg{}",
            stem,
            if cli.spectra { format!("  {}_spectrum.png", stem) } else { String::new() }
        );
    }

    println!("done.");
    Ok(())
}

// ── FFT helpers ───────────────────────────────────────────────────────────────

fn fft2d(data: &mut Vec<Complex<f32>>, width: usize, height: usize, forward: bool) {
    let mut planner = FftPlanner::<f32>::new();

    let row_fft = if forward { planner.plan_fft_forward(width) } else { planner.plan_fft_inverse(width) };
    for row in data.chunks_mut(width) { row_fft.process(row); }

    *data = transpose(data, height, width);

    let col_fft = if forward { planner.plan_fft_forward(height) } else { planner.plan_fft_inverse(height) };
    for col in data.chunks_mut(height) { col_fft.process(col); }

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

/// Apply a frequency-domain mask to a single image channel and return filtered pixels.
fn apply_mask_channel(data: &[f32], width: usize, height: usize, mask: &[f32]) -> Vec<f32> {
    let mut freq: Vec<Complex<f32>> = data.iter().map(|&v| Complex::new(v, 0.0)).collect();
    fft2d(&mut freq, width, height, true);
    for (f, m) in freq.iter_mut().zip(mask) { *f *= *m; }
    fft2d(&mut freq, width, height, false);
    let norm = (width * height) as f32;
    freq.iter().map(|c| c.re / norm).collect()
}

// ── Peak detection ────────────────────────────────────────────────────────────

/// Collect NMS local-maxima outside the DC zone from an FFT buffer.
/// Returns (log_magnitude, signed_fdx, signed_fdy) sorted by magnitude desc.
fn find_nms_candidates(
    freq: &[Complex<f32>],
    width: usize,
    height: usize,
    opts: &FilterOpts,
) -> Vec<(f32, i32, i32)> {
    let dc_px = width.min(height) as f32 * opts.dc_exclude;
    let log_mag: Vec<f32> = freq.iter().map(|c| c.norm_sqr().ln_1p()).collect();
    let nms = opts.nms_radius as i32;
    let mut candidates: Vec<(f32, i32, i32)> = Vec::new();

    for i in 0..freq.len() {
        let ux = i % width;
        let uy = i / width;
        let sdx = signed_freq(ux, width);
        let sdy = signed_freq(uy, height);
        if ((sdx as f32).powi(2) + (sdy as f32).powi(2)).sqrt() <= dc_px { continue; }

        let v = log_mag[i];
        let mut is_max = true;
        'outer: for dy in -nms..=nms {
            for dx in -nms..=nms {
                if dx == 0 && dy == 0 { continue; }
                let nx = (ux as i32 + dx).rem_euclid(width as i32) as usize;
                let ny = (uy as i32 + dy).rem_euclid(height as i32) as usize;
                if log_mag[ny * width + nx] >= v { is_max = false; break 'outer; }
            }
        }
        if is_max { candidates.push((v, sdx, sdy)); }
    }
    candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    candidates
}

/// Per-card mode: detect halftone peaks from this card's spectrum, build mask.
/// Returns (mask, confirmed_fundamentals_in_FFT_bins).
fn build_notch_mask(
    freq: &[Complex<f32>],
    width: usize,
    height: usize,
    opts: &FilterOpts,
) -> (Vec<f32>, Vec<(i32, i32)>) {
    let candidates = find_nms_candidates(freq, width, height, opts);

    let freq_set: std::collections::HashSet<(i32, i32)> =
        candidates.iter().map(|&(_, dx, dy)| (dx, dy)).collect();

    let mut seen: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    let mut fundamentals: Vec<(i32, i32)> = Vec::new();
    for &(_, dx, dy) in &candidates {
        if fundamentals.len() >= opts.num_fundamentals { break; }
        if dx == 0 || dy == 0 { continue; }           // skip axis-aligned (JPEG artefacts)
        if !freq_set.contains(&(-dx, -dy)) { continue; } // require conjugate pair
        let canon = if dx > 0 { (dx, dy) } else { (-dx, -dy) };
        if seen.insert(canon) { fundamentals.push((dx, dy)); }
    }

    let mask = build_mask_from_bins(width, height, &fundamentals, opts, Some(freq));
    (mask, fundamentals)
}

/// Consensus mode: collect peaks from one card using NMS, normalised to cycles/pixel.
/// Uses the same conjugate-pair and axis filter as per-card mode for reliability.
/// Optionally gates to peaks whose radial distance falls within ±15% of a detected ring.
fn collect_peaks_cyc(
    freq: &[Complex<f32>],
    width: usize,
    height: usize,
    opts: &FilterOpts,
) -> Vec<(f32, f32)> {
    let candidates = find_nms_candidates(freq, width, height, opts);
    let freq_set: std::collections::HashSet<(i32, i32)> =
        candidates.iter().map(|&(_, dx, dy)| (dx, dy)).collect();

    let ring_radii = find_dominant_rings(freq, width, height, opts);
    let min_cyc = if opts.max_period > 0.0 { 1.0 / opts.max_period } else { 0.0 };

    let mut seen: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    let mut out: Vec<(f32, f32)> = Vec::new();
    for &(_, dx, dy) in &candidates {
        if dx == 0 || dy == 0 { continue; }              // skip axis-aligned (JPEG artefacts)
        if !freq_set.contains(&(-dx, -dy)) { continue; } // require conjugate pair
        let fx = dx as f32 / width as f32;
        let fy = dy as f32 / height as f32;
        if (fx * fx + fy * fy).sqrt() < min_cyc { continue; }

        // Ring-radius gate: only accept peaks within ±15% of a dominant ring
        if !ring_radii.is_empty() {
            let r = ((dx as f32).powi(2) + (dy as f32).powi(2)).sqrt();
            let on_ring = ring_radii.iter().any(|&rr| (r - rr).abs() <= rr * 0.15);
            if !on_ring { continue; }
        }

        let canon = if dx > 0 { (dx, dy) } else { (-dx, -dy) };
        if seen.insert(canon) {
            out.push((canon.0 as f32 / width as f32, canon.1 as f32 / height as f32));
        }
    }
    out
}

/// Identify dominant ring radii in the FFT magnitude spectrum (in FFT-bin pixels).
/// Builds a radial power profile (excluding DC zone and axis-aligned JPEG artifacts),
/// then finds local maxima above 50% of the band peak.
fn find_dominant_rings(
    freq: &[Complex<f32>],
    width: usize,
    height: usize,
    opts: &FilterOpts,
) -> Vec<f32> {
    let dc_px = width.min(height) as f32 * opts.dc_exclude;
    let max_r = (width.min(height) / 2) as f32;
    let min_cyc = if opts.max_period > 0.0 { 1.0 / opts.max_period } else { 0.0 };
    let min_r = min_cyc * width.min(height) as f32;

    let n_bins = max_r.ceil() as usize + 2;
    let mut profile = vec![0.0f32; n_bins];
    let mut counts = vec![0usize; n_bins];

    for i in 0..freq.len() {
        let ux = i % width;
        let uy = i / width;
        let sdx = signed_freq(ux, width);
        let sdy = signed_freq(uy, height);
        let r = ((sdx as f32).powi(2) + (sdy as f32).powi(2)).sqrt();
        if r <= dc_px { continue; }
        if sdx.abs() < 4 || sdy.abs() < 4 { continue; } // exclude JPEG DCT axis artifacts
        let bin = r as usize;
        if bin < n_bins {
            profile[bin] += freq[i].norm_sqr().ln_1p();
            counts[bin] += 1;
        }
    }

    for i in 0..n_bins {
        if counts[i] > 0 { profile[i] /= counts[i] as f32; }
    }

    // Smooth profile with a 3-bin running average to reduce noise
    let smoothed: Vec<f32> = (0..n_bins)
        .map(|i| {
            let lo = i.saturating_sub(1);
            let hi = (i + 1).min(n_bins - 1);
            let w = (hi - lo + 1) as f32;
            profile[lo..=hi].iter().sum::<f32>() / w
        })
        .collect();

    let r_start = (min_r.ceil() as usize).max(1);
    let r_end = (max_r as usize).min(n_bins.saturating_sub(2));
    let band_max = smoothed[r_start..=r_end].iter().cloned().fold(0.0f32, f32::max);
    if band_max == 0.0 { return Vec::new(); }
    let threshold = band_max * 0.5;

    let window = 4usize;
    let mut rings: Vec<f32> = Vec::new();
    for r in (r_start + window)..r_end.saturating_sub(window) {
        if smoothed[r] < threshold { continue; }
        let is_local_max = (1..=window)
            .all(|d| smoothed[r] >= smoothed[r - d] && smoothed[r] >= smoothed[r + d]);
        if is_local_max { rings.push(r as f32); }
    }
    rings
}

/// Find frequencies that appear in at least `min_fraction` of cards (within `tolerance` cyc/px).
/// Returns consensus frequencies in cycles/pixel, sorted by support (most-common first).
fn find_consensus_peaks(
    all_peaks: &[Vec<(f32, f32)>],
    min_fraction: f32,
    tolerance: f32,
) -> Vec<(f32, f32)> {
    let n_cards = all_peaks.len();
    let min_count = ((n_cards as f32 * min_fraction).ceil() as usize).max(2);

    // Flatten: (fx, fy, card_index)
    let mut flat: Vec<(f32, f32, usize)> = all_peaks
        .iter()
        .enumerate()
        .flat_map(|(ci, peaks)| peaks.iter().map(move |&(fx, fy)| (fx, fy, ci)))
        .collect();
    flat.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut used = vec![false; flat.len()];
    let mut clusters: Vec<(f32, f32, usize)> = Vec::new(); // (avg_fx, avg_fy, card_count)

    for i in 0..flat.len() {
        if used[i] { continue; }
        let (fx0, fy0, _) = flat[i];
        let mut sum_x = fx0;
        let mut sum_y = fy0;
        let mut cnt = 1usize;
        let mut cards = std::collections::HashSet::new();
        cards.insert(flat[i].2);

        for j in (i + 1)..flat.len() {
            if flat[j].0 - fx0 > tolerance * 2.0 { break; }
            let (fx2, fy2, ci2) = flat[j];
            if ((fx2 - fx0).powi(2) + (fy2 - fy0).powi(2)).sqrt() <= tolerance {
                sum_x += fx2; sum_y += fy2; cnt += 1;
                cards.insert(ci2);
                used[j] = true;
            }
        }
        used[i] = true;

        if cards.len() >= min_count {
            clusters.push((sum_x / cnt as f32, sum_y / cnt as f32, cards.len()));
        }
    }

    // Sort by how many cards support this frequency
    clusters.sort_by(|a, b| b.2.cmp(&a.2));

    // De-duplicate conjugate pairs (keep positive-x representative)
    let mut seen: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    let mut result: Vec<(f32, f32)> = Vec::new();
    for (fx, fy, _) in clusters {
        let key_x = (fx * 10000.0).round() as i32;
        let key_y = (fy * 10000.0).round() as i32;
        let canon = if key_x >= 0 { (key_x, key_y) } else { (-key_x, -key_y) };
        if seen.insert(canon) {
            result.push((fx, fy));
        }
    }
    result
}

/// Estimate log-magnitude SNR of the FFT peak at (px, py) vs. a background ring
/// sampled at `sample_r` pixels away. Returns nats above median background (≥ 0).
fn peak_snr(
    freq: &[Complex<f32>],
    width: usize,
    height: usize,
    px: usize,
    py: usize,
    sample_r: i32,
) -> f32 {
    let peak_lm = freq[py * width + px].norm_sqr().ln_1p();
    let mut bg: Vec<f32> = Vec::with_capacity(16);
    for k in 0..16usize {
        let angle = k as f32 * std::f32::consts::PI / 8.0;
        let bx = (px as i32 + (sample_r as f32 * angle.cos()).round() as i32)
            .rem_euclid(width as i32) as usize;
        let by = (py as i32 + (sample_r as f32 * angle.sin()).round() as i32)
            .rem_euclid(height as i32) as usize;
        bg.push(freq[by * width + bx].norm_sqr().ln_1p());
    }
    bg.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median_bg = bg[bg.len() / 2];
    (peak_lm - median_bg).max(0.0)
}

/// Build a notch mask from FFT-bin fundamentals (per-card mode).
fn build_mask_from_bins(
    width: usize,
    height: usize,
    fundamentals: &[(i32, i32)],
    opts: &FilterOpts,
    freq: Option<&[Complex<f32>]>,
) -> Vec<f32> {
    let cyc: Vec<(f32, f32)> = fundamentals
        .iter()
        .map(|&(dx, dy)| (dx as f32 / width as f32, dy as f32 / height as f32))
        .collect();
    build_mask_from_freqs_cyc(width, height, &cyc, opts, freq)
}

/// Build a notch mask from frequencies in cycles/pixel (works for any image size).
/// For each fundamental (fx, fy), also notches the perpendicular companion (-fy, fx)
/// at the same period — both are basis vectors of the same 2D halftone screen grid.
///
/// When `freq` is provided, each notch's sigma is scaled by the peak's local SNR:
/// high-SNR peaks (confident halftone) get up to 1.3× wider notches;
/// low-SNR peaks (uncertain) get down to 0.7× narrower notches.
fn build_mask_from_freqs_cyc(
    width: usize,
    height: usize,
    fundamentals_cyc: &[(f32, f32)],
    opts: &FilterOpts,
    freq: Option<&[Complex<f32>]>,
) -> Vec<f32> {
    let base_sigma = opts.notch_radius * width.min(height) as f32;
    let dc_px = width.min(height) as f32 * opts.dc_exclude;
    let halfw = (width / 2) as i32;
    let halfh = (height / 2) as i32;
    let sample_r = (opts.nms_radius as i32 * 2).max(8);

    // Expand each fundamental to include its perpendicular companion.
    let mut expanded: Vec<(f32, f32)> = Vec::with_capacity(fundamentals_cyc.len() * 2);
    let mut seen_keys: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    for &(fx, fy) in fundamentals_cyc {
        for (ex, ey) in [(fx, fy), (-fy, fx)] {
            let kx = (ex * 10000.0).round() as i32;
            let ky = (ey * 10000.0).round() as i32;
            let canon = if kx >= 0 { (kx, ky) } else { (-kx, -ky) };
            if seen_keys.insert(canon) { expanded.push((ex, ey)); }
        }
    }

    let mut mask = vec![1.0f32; width * height];

    for &(fx_cyc, fy_cyc) in &expanded {
        let fdx = (fx_cyc * width as f32).round() as i32;
        let fdy = (fy_cyc * height as f32).round() as i32;

        for n in 1i32..=64 {
            let hx = n * fdx;
            let hy = n * fdy;
            if hx.abs() > halfw || hy.abs() > halfh { break; }

            let dist = ((hx as f32).powi(2) + (hy as f32).powi(2)).sqrt();
            if dist <= dc_px { continue; }

            let ux_h = hx.rem_euclid(width as i32) as usize;
            let uy_h = hy.rem_euclid(height as i32) as usize;

            let sigma = if let Some(f) = freq {
                let snr = peak_snr(f, width, height, ux_h, uy_h, sample_r);
                // 0.7× at snr=0, saturates toward 1.3× for very strong peaks
                let factor = (0.7 + 0.6 * (snr / 4.0).tanh()).clamp(0.7, 1.3);
                base_sigma * factor
            } else {
                base_sigma
            };
            let sigma2 = sigma * sigma;

            apply_gaussian_notch(&mut mask, width, height, ux_h, uy_h, sigma2);
            let mx = (width - ux_h) % width;
            let my = (height - uy_h) % height;
            apply_gaussian_notch(&mut mask, width, height, mx, my, sigma2);
        }
    }
    mask
}

#[inline]
fn signed_freq(u: usize, n: usize) -> i32 {
    let u = u as i32;
    let n = n as i32;
    if u <= n / 2 { u } else { u - n }
}

fn apply_gaussian_notch(mask: &mut [f32], width: usize, height: usize, px: usize, py: usize, sigma2: f32) {
    let radius = (sigma2.sqrt() * 3.0).ceil() as i32;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let d2 = (dx * dx + dy * dy) as f32;
            if d2 > sigma2 * 9.0 { continue; }
            let nx = (px as i32 + dx).rem_euclid(width as i32) as usize;
            let ny = (py as i32 + dy).rem_euclid(height as i32) as usize;
            mask[ny * width + nx] *= 1.0 - (-d2 / (2.0 * sigma2)).exp();
        }
    }
}

// ── Spectrum visualisation ────────────────────────────────────────────────────

/// Grayscale log-magnitude spectrum (fftshift, DC at centre) with red tint
/// showing notch depth: bright red = frequency heavily attenuated.
fn save_spectrum(
    freq: &[Complex<f32>],
    width: usize,
    height: usize,
    mask: &[f32],
    path: &Path,
) -> Result<()> {
    // Log-magnitude in fftshift layout
    let mut spectrum = vec![0.0f32; freq.len()];
    for (i, c) in freq.iter().enumerate() {
        let sx = (i % width + width / 2) % width;
        let sy = (i / width + height / 2) % height;
        spectrum[sy * width + sx] = c.norm_sqr().ln_1p();
    }
    let max_val = spectrum.iter().cloned().fold(0.0f32, f32::max);
    let scale = if max_val > 0.0 { 255.0 / max_val } else { 1.0 };

    // Mask in fftshift layout
    let mut mask_shifted = vec![1.0f32; mask.len()];
    for (i, &m) in mask.iter().enumerate() {
        let sx = (i % width + width / 2) % width;
        let sy = (i / width + height / 2) % height;
        mask_shifted[sy * width + sx] = m;
    }

    let mut pixels: Vec<u8> = Vec::with_capacity(width * height * 3);
    for i in 0..spectrum.len() {
        let gray = spectrum[i] * scale;
        let depth = (1.0 - mask_shifted[i]).clamp(0.0, 1.0);
        let r = (gray * (1.0 - depth) + 230.0 * depth).clamp(0.0, 255.0) as u8;
        let g = (gray * (1.0 - depth * 0.9)).clamp(0.0, 255.0) as u8;
        let b = g;
        pixels.extend_from_slice(&[r, g, b]);
    }

    let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_raw(width as u32, height as u32, pixels)
            .expect("spectrum buffer size mismatch");
    img.save(path)?;
    Ok(())
}
