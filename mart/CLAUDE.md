# kardawelti — current state

MTG proxy card generator. Takes Scryfall-format JSON + artwork, renders a 2.5 × 3.5 in card at any DPI (default 300 → 750 × 1050 px). All visual layout is in millimeters; pixels appear only at the rasterization boundary.

Design doc: [../docs/kardawelti_design.html](../docs/kardawelti_design.html)

## Workspace layout

```
kardawelti/
├── Cargo.toml                                # workspace root
├── crates/
│   ├── kardawelti-core/                      # library
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── geometry.rs                   # Mm, Dpi, MmRect
│   │       ├── frame.rs                      # FrameSpec + main_color/alt_color palette
│   │       ├── fonts.rs                      # Windsor Roman/Demi/Light/LtCn loading
│   │       ├── text.rs                       # single-line draw_in_rect with valign + scale_x fit
│   │       ├── rules.rs                      # multi-line layout (greedy break, font-size auto-fit)
│   │       ├── symbols.rs                    # mana-cost parse + Scryfall SVG rasterization
│   │       ├── render.rs                     # the visual composition
│   │       └── scryfall/
│   │           ├── mod.rs                    # serde Card struct + old_type_line()
│   │           └── api.rs                    # ureq client + set-pull workflow
│   └── kardawelti/                           # binary (CLI)
│       └── src/main.rs
└── examples/
    ├── lightning_bolt.json
    └── birds_of_paradise.json
```

## CLI

```sh
kardawelti render <CARD_JSON> [ART] [-o OUT] [--dpi N] [--symbols-dir DIR] [--fonts-dir DIR]
kardawelti pull   <SET_CODE>  [-o DIR] [--no-art]
```

Defaults: `-o out.png`, `--dpi 300`, `--out ~/cards/<SET>` for `pull`.

## Visual style — 1993 / premodern

Two-color scheme per FrameColor: **main** (saturated card color) and **alt** (light desaturated complement).

Render order (in [render.rs](crates/kardawelti-core/src/render.rs)):
1. Outer black rounded border (corner radius 2.5 mm, border 3 mm)
2. Inner area filled with `alt_color` — becomes the bottom-half background
3. Title bar painted in `main_color`, flush against inner border on top/left/right
4. Art covers the full inner width between title bar and the alt area
5. Mana cost — right-aligned in title bar, white disc behind each Scryfall SVG symbol
6. Title text — alt color on main bar, Windsor Roman, 3.84 mm, `valign_frac=0.30` (slightly above center), 1.25 mm left pad, ≥3 mm gap to mana cost; `set_scale_x` compresses (clamped to 0.65) before falling back to font-size shrink
7. Type line — main color on alt background, Windsor Demi, 3.6 mm, "Summon X" old naming via `Card::old_type_line()`
8. Set symbol placeholder — white diamond with main-color outline, right of type line
9. Rules text + flavor text — black, in the rules box; symbols rendered as plain text (`{T}` → `T`)
   - If both present: oracle gets top 60 %, flavor gets bottom 40 % (1 mm gap)
   - Oracle: Windsor Light
   - Flavor: Windsor Light Condensed BT, fake-italic via `font.set_skew_x(-0.18)` (face has no italic variant)
10. P/T (creatures only) — black Windsor Roman, 6.0 mm, format `"{p} / {t}"` with spaces, no tab background, bottom-right

## Frame dimensions (mm)

Card 63.5 × 88.9, border 3, inner 57.5 × 82.9.

| Region      | x   | y    | w    | h    | Notes |
| ----------- | --- | ---- | ---- | ---- | ----- |
| title_bar   | 3.0 | 3.0  | 57.5 | 7.2  | main color fill |
| art_box     | 3.0 | 10.2 | 57.5 | 44.0 | full width |
| type_bar    | 5.0 | 55.7 | 53.5 | 6.0  | text band on alt; no fill |
| rules_box   | 5.0 | 62.7 | 53.5 | 15.0 | text band on alt; no fill |
| pt_box      | 44.0| 77.9 | 16.0 | 8.0  | Roman black, no tab |
| mana_anchor | 58.0| 6.6  | —    | —    | top-right of mana cost |

## Fonts

Loaded by [fonts.rs](crates/kardawelti-core/src/fonts.rs) via Skia's macOS FontMgr (picks up `~/Library/Fonts/`). User has Bitstream Windsor BT installed.

| Field   | Face                       | Used for                |
| ------- | -------------------------- | ----------------------- |
| `roman` | Windsor BT Roman           | title, P/T              |
| `demi`  | Windsor (Demi)             | type line               |
| `light` | Windsor Light BT           | oracle / rules text     |
| `flavor`| Windsor LtCn BT (Light Cond.) | flavor text (skewed) |

Each has a generic system fallback (Helvetica/Times) when Windsor isn't installed.

## Mana / set symbols

Mana symbols are pre-fetched Scryfall SVGs; `--symbols-dir` points at a directory of `<TOKEN>.svg` files (e.g. `R.svg`, `WU.svg`, `2W.svg`). Filename convention: strip `{`, `}`, `/` from the symbol code.

[symbols.rs](crates/kardawelti-core/src/symbols.rs) lazy-loads SVGs into a `SymbolCache` (caches parsed `usvg::Tree`) and rasterizes via `resvg` to a `tiny_skia::Pixmap` → `skia_safe::Image`.

Inline rules-text rendering passes `cache: None` to force plain-text symbols (matches the premodern visual). The mana cost in the title bar still uses SVGs (one per token), each behind a white disc for visibility on the dark title bar.

Set symbol is a placeholder shape (filled white diamond w/ outline) drawn in Skia at the right end of the type line.

## Scryfall pull

[scryfall/api.rs](crates/kardawelti-core/src/scryfall/api.rs) — blocking `ureq` client.
- 150 ms minimum gap between requests (Scryfall asks ≥50 ms; we're polite)
- HTTP 429 → typed `ApiError::RateLimited`; `pull_set` aborts immediately, leaving partial output in place. Re-running resumes by skipping existing files.
- `pull DRK --out ~/cards/DRK` → `<num>.json` + `<num>.jpg` per card, plus `_manifest.json`. Test pull of The Dark: 119 cards, ~13 MB, ~20 s wall time.
- Double-faced fallback: `<num>_face<i>.jpg` if the card has `card_faces[].image_uris.art_crop`.

## Test cards (in /tmp/ from various sessions)

JSON + art for each, all fetched as the **oldest-art printing** via `/cards/search?q=!"NAME"&unique=art&order=released&dir=asc`.

| Slug                  | Set | Year | Notes |
| --------------------- | --- | ---- | ----- |
| lightning_bolt        | LEA | 1993 | single-mana, simple oracle |
| grizzly_bears         | LEA | 1993 | vanilla creature, no oracle/flavor |
| birds_of_paradise     | LEA | 1993 | two paragraphs, inline `{T}` |
| force_of_nature       | LEA | 1993 | 5-symbol mana cost ({2}{G}{G}{G}{G}), tall body |
| leviathan             | DRK | 1994 | 5-symbol cost ({5}{U}{U}{U}{U}), 4 paragraphs |
| flowstone_salamander  | TMP | 1997 | flavor text present — exercises italic-skew flavor render |

Plus the full DRK archive at `~/cards/DRK/` (119 cards) — any of these can be rendered: `kardawelti render ~/cards/DRK/<NUM>.json ~/cards/DRK/<NUM>.jpg --symbols-dir /tmp/kardawelti_symbols -o /tmp/<name>.png`.

## Status / milestones

| Milestone | Status | Deliverable |
| --------- | ------ | ----------- |
| M1 | ✅ | Workspace builds; renders 750×1050 PNG with frame + art |
| M2 | ✅ | M15-style frame, title / type-line / P/T text |
| M3 | ✅ | Mana cost via Scryfall SVGs, right-aligned title bar |
| M4 | ✅ | Rules text with mixed body + inline symbols, font-size auto-fit |
| M5 | — | Live preview window (eframe + notify) — not started |
| M5b | partial | `pull` subcommand done; interactive `fetch <NAME>` not yet wired |
| M6+ | — | Per-color frame variants polish, edge cases, batch/print sheet |

Style overhaul (post-M4): premodern two-color frame, full-width art, "Summon X" old naming, white-disc mana cost, plain-text inline symbols, fake-italic flavor text.

## Build

First build is slow (~10 min — `skia-safe` compiles Skia C++); subsequent builds are seconds.

```sh
cd kardawelti
cargo build              # debug
cargo build --release    # release
```

Cargo isn't on the non-interactive PATH for some shell sessions; use `~/.cargo/bin/cargo` if `cargo` isn't found.
