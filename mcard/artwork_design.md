# mcard — artwork design reference

This document describes how every visual element on a rendered card is sized and positioned. All layout is done in millimeters; pixels appear only at the final rasterization step. The source of truth for each value is noted so changes can be made in the right place.

---

## Coordinate system

Origin is the top-left corner of the card. X increases right, Y increases down. All constants live in `crates/mcard-core/src/geometry.rs` and `frame.rs`.

```
(0, 0) ─────────────────────────────► X
  │  ┌──────────────────────────────┐
  │  │  outer black border (3 mm)   │
  │  │  ┌────────────────────────┐  │
  │  │  │   title bar (main)     │  │
  │  │  ├────────────────────────┤  │
  │  │  │                        │  │
  │  │  │       art box          │  │
  │  │  │                        │  │
  │  │  ├────────────────────────┤  │
  │  │  │  type line             │  │
  │  │  │  rules box             │  │
  │  │  │                   P/T  │  │
  │  │  └────────────────────────┘  │
  │  └──────────────────────────────┘
  ▼ Y
```

---

## Card stock

| Property | Value | Source |
|---|---|---|
| Width | 63.5 mm | `geometry.rs` `CARD_WIDTH_MM` |
| Height | 88.9 mm | `geometry.rs` `CARD_HEIGHT_MM` |
| Default DPI | 300 | `geometry.rs` `DEFAULT_DPI` |
| Default output px | 750 × 1050 | computed: `card_size_px(dpi)` |

At 300 DPI, 1 mm = 11.811 px (`dpi / 25.4`).

---

## Outer border

Drawn as a black rounded rectangle that covers the full card.

| Property | Value | Source |
|---|---|---|
| Border thickness | 3.0 mm | `frame.rs` `border_mm` |
| Corner radius | 2.5 mm | `frame.rs` `corner_radius_mm` |
| Color | black | `render.rs` step 1 |

The inner area begins at `(border_mm, border_mm)` = `(3.0, 3.0)` and measures `57.5 × 82.9 mm`.

---

## Color scheme

Each card color has two values: **main** (saturated, used for the title bar and type-line text) and **alt** (light desaturated complement, used for the bottom-half background and title text).

| Color | Main (R G B) | Alt (R G B) |
|---|---|---|
| White | 0.62 0.55 0.42 | 0.96 0.94 0.86 |
| Blue | 0.28 0.42 0.58 | 0.78 0.86 0.92 |
| Black | 0.22 0.20 0.22 | 0.78 0.76 0.76 |
| Red | 0.62 0.30 0.25 | 0.94 0.80 0.74 |
| Green | 0.36 0.50 0.38 | 0.74 0.82 0.70 |
| Gold | 0.66 0.52 0.28 | 0.94 0.86 0.66 |
| Colorless | 0.50 0.50 0.52 | 0.86 0.86 0.88 |

Source: `frame.rs` `main_color()` / `alt_color()`.

---

## Layout regions

All values are in mm. `x` and `y` are the top-left corner of each region.

```
┌──────────────────────────────────────┐  y=0
│  outer border (3 mm)                 │
│  ┌────────────────────────────────┐  │  y=3.0
│  │  TITLE BAR  h=5.25 mm          │  │
│  └────────────────────────────────┘  │  y=8.25
│  ┌────────────────────────────────┐  │
│  │                                │  │
│  │  ART BOX  h=45.95 mm           │  │
│  │                                │  │
│  └────────────────────────────────┘  │  y=54.2
│  (alt-color fill from y=8.25 down)   │
│  ┌────────────────────────────────┐  │  y=54.95
│  │  TYPE LINE  h=6.0 mm           │  │
│  └────────────────────────────────┘  │  y=60.95
│  ┌────────────────────────────────┐  │  y=61.95
│  │  RULES BOX  h=15.0 mm          │  │
│  └────────────────────────────────┘  │  y=76.95
│                         ┌──────────┐ │  y=76.4
│                         │   P/T    │ │
│                         └──────────┘ │  y=84.4
│  outer border (3 mm)                 │
└──────────────────────────────────────┘  y=88.9
```

| Region | x | y | w | h | Notes |
|---|---|---|---|---|---|
| inner area | 3.0 | 3.0 | 57.5 | 82.9 | alt-color fill from title bar bottom down |
| title_bar | 3.0 | 3.0 | 57.5 | 5.25 | main-color fill |
| art_box | 3.0 | 8.25 | 57.5 | 45.95 | full inner width |
| type_bar | 5.0 | 54.95 | 53.5 | 6.0 | no fill; text on alt background |
| rules_box | 5.0 | 61.95 | 53.5 | 15.0 | no fill; text on alt background |
| pt_box | 44.0 | 77.4 | 16.0 | 8.0 | 0.5 mm from inner right + bottom edge; text bottom-aligned |

Source: `frame.rs` `premodern()`.

**Key relationships:**
- `art_y = border_mm + title_h` = 3.0 + 5.25 = 8.25
- `alt_top = art_y + art_h` = 8.25 + 45.95 = 54.2 (where alt-color fill begins)
- `type_y = alt_top + 0.75` — 0.75 mm gap between art bottom and type line
- `rules_y = type_y + type_h + 1.0` — 1.0 mm gap between type and rules
- `pt_x = CARD_WIDTH_MM - border_mm - pt_w - 0.5` — equal 0.5 mm inner-edge offset
- `pt_y = CARD_HEIGHT_MM - border_mm - pt_h - 0.5` — equal 0.5 mm inner-edge offset

---

## Title bar elements

### Mana cost symbols

Right-aligned inside the title bar. Each symbol sits behind a white disc.

| Property | Value | Source |
|---|---|---|
| Symbol diameter | 3.2 mm | `render.rs` `symbol_size_mm` |
| Gap between symbols | 0.4 mm | `render.rs` `gap_mm` |
| Right padding | 1.5 mm | `render.rs` `right_pad_mm` |
| White disc radius | symbol diameter / 2 | `render.rs` `draw_mana_cost` |
| Vertical position | centered in title bar | computed: `bar.top + (bar.height() - symbol_size_px) * 0.5` |

Symbol SVGs are loaded from the mdata cache at `data/_meta/symbols/<TOKEN>.svg`. Filename convention: strip `{`, `}`, `/` from the symbol code (e.g. `{W/U}` → `WU.svg`).

### Card name text

| Property | Value | Source |
|---|---|---|
| Font | Windsor Roman | `fonts.rs` `roman` |
| Size | 3.84 mm | `render.rs` step 6 |
| Color | alt color (light) | `render.rs` step 6 |
| Horizontal align | left | `render.rs` `HAlign::Left` |
| Left padding | 1.25 mm | `render.rs` `pad_horizontal(frame.title_bar, 1.25)` |
| Right clearance | 3.0 mm gap to mana cost left edge | `render.rs` step 6 |
| Vertical align | centered (0.50) | `render.rs` `with_valign_frac(0.50)` |
| Overflow handling | horizontal scale_x compression (min 0.65×) then font-size shrink | `text.rs` `draw_in_rect` |

---

## Type line

| Property | Value | Source |
|---|---|---|
| Font | Windsor Demi | `fonts.rs` `demi` |
| Size | 3.6 mm | `render.rs` step 7 |
| Color | main color (saturated) | `render.rs` step 7 |
| Horizontal align | left | `render.rs` `HAlign::Left` |
| Naming convention | "Summon X" old style for creatures | `scryfall/mod.rs` `old_type_line()` |
| Right clearance | set symbol width (3.6 mm) + 1.0 mm pad + 1.0 mm gap | `render.rs` step 7 |

### Set symbol placeholder

A filled white diamond with a main-color outline, drawn at the right end of the type bar.

| Property | Value | Source |
|---|---|---|
| Width (diamond radius) | 3.6 mm | `render.rs` `set_symbol_w_mm` |
| Right padding | 1.0 mm | `render.rs` `set_symbol_right_pad_mm` |
| Fill | white | `render.rs` `draw_set_symbol_placeholder` |
| Outline | main color | `render.rs` `draw_set_symbol_placeholder` |
| Stroke width | symbol size × 0.06 | `render.rs` `draw_set_symbol_placeholder` |

---

## Rules text

Rules text and flavor text share the rules box. If both are present the box is split 60 / 40 (oracle / flavor) with a 0.8 mm gap.

### Oracle text

| Property | Value | Source |
|---|---|---|
| Font | Windsor Light | `fonts.rs` `light` |
| Base size | 3.0 mm | `render.rs` step 9 |
| Minimum size | 2.0 mm | `render.rs` step 9 |
| Color | black | `rules.rs` |
| Line height | font size × 1.18 | `rules.rs` `try_size` |
| Paragraph gap | font size × 0.55 | `rules.rs` `try_size` |
| Inline symbol size | font size × 0.95 | `rules.rs` `try_size` |
| Inline symbols | plain text (e.g. `{T}` → `T`) | premodern style; no SVG cache passed |
| Overflow handling | binary-search font-size reduction down to minimum (7 iterations) | `rules.rs` `draw_rules` |

### Flavor text

| Property | Value | Source |
|---|---|---|
| Font | Windsor Light Condensed BT | `fonts.rs` `flavor` |
| Base size | 2.6 mm | `render.rs` step 9 |
| Minimum size | 1.7 mm | `render.rs` step 9 |
| Italic effect | fake skew via `font.set_skew_x(-0.18)` | `render.rs` step 9 (font has no italic variant) |
| Color | black | `rules.rs` |

---

## Power / toughness

Creatures only. Drawn right-aligned with equal offset from the inner card edges.

| Property | Value | Source |
|---|---|---|
| Font | Windsor Roman | `fonts.rs` `roman` |
| Size | 6.0 mm | `render.rs` step 10 |
| Color | black | `render.rs` step 10 |
| Format | `"{power} / {toughness}"` with spaces | `render.rs` step 10 |
| Horizontal align | right | `render.rs` `HAlign::Right` |
| Vertical align | bottom (1.0) | `render.rs` `with_valign_frac(1.0)` |
| Right offset from inner edge | 0.5 mm | `frame.rs` `pt_x` |
| Bottom offset from inner edge | 0.5 mm | `frame.rs` `pt_y` |

---

## Fonts

Loaded by `crates/mcard-core/src/fonts.rs` via Skia's macOS font manager, which picks up fonts installed in `~/Library/Fonts/`. Each face has a system fallback.

| Field | Face | Used for | Fallback |
|---|---|---|---|
| `roman` | Windsor BT Roman | card name, P/T | Times New Roman |
| `demi` | Windsor (Demi) | type line | Helvetica Bold |
| `light` | Windsor Light BT | oracle / rules text | Helvetica |
| `flavor` | Windsor LtCn BT | flavor text (fake-italic) | Helvetica |

Optional `--fonts-dir` loads faces by filename from a directory (useful for CI or non-macOS builds).

---

## Text fitting algorithm

Used for single-line text (card name). Implemented in `text.rs` `draw_in_rect`.

1. Measure natural string width at the requested font size.
2. If width fits within the available box, draw as-is.
3. If it overflows, compute the required `scale_x = max_w / natural_w`.
4. If `scale_x ≥ 0.65` (minimum), apply horizontal compression only — font weight and height are preserved.
5. If compression alone is not enough, clamp `scale_x` to 0.65 and additionally reduce font size proportionally until the string fits.

---

## Rules text fitting algorithm

Used for multi-line text (oracle, flavor). Implemented in `rules.rs` `draw_rules`.

1. Attempt layout at `base_size`.
2. If total text height fits within the box, draw at `base_size`.
3. If it overflows, binary-search between `min_size` and `base_size` (7 iterations ≈ 1% precision) for the largest size that fits.
4. If even `min_size` overflows, draw at `min_size` (text clips the box).

Line breaking is greedy: words are added to a line until the next word would overflow, then a new line starts. Paragraph breaks (`\n` in oracle text) add an extra `paragraph_gap_px` of vertical space.

---

## Render order

Elements are drawn in this order (later draws appear on top):

1. Outer black rounded border
2. Inner area filled with alt color
3. Title bar filled with main color
4. Art (or dark placeholder if no art file provided)
5. Mana cost — white discs + SVG symbols, right-aligned in title bar
6. Card name — alt color text on title bar
7. Type line — main color text on alt background
8. Set symbol placeholder — white diamond with main-color outline
9. Oracle text — black Windsor Light on alt background
10. Flavor text — black Windsor Light Condensed (fake-italic) on alt background
11. Power / toughness — black Windsor Roman, bottom-right (creatures only)

Source: `render.rs` `render_png`.
