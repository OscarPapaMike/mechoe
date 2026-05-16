# kardawelti

MTG proxy card generator. Takes Scryfall-format JSON + artwork PNG, renders a 2.5 × 3.5 in card at any DPI (default 300).

See [../docs/kardawelti_design.html](../docs/kardawelti_design.html) for design and architecture.

## Status

**M4 (current):** rules text rendered in the rules box with mixed body + inline mana-symbol runs, paragraph breaks, and font-size auto-fit (binary search until layout fits the box). Mana cost is rendered in the title bar via Scryfall SVG symbols. Title and P/T use Windsor Roman; type line and rules text use Windsor Light (loaded from system fonts on macOS, falling back to Helvetica/Times if Windsor isn't installed).

## Quick start

```sh
# First build is slow — skia-safe compiles Skia C++ (~10 min).
cargo build --release

# Render with a placeholder art box (no art file):
./target/release/kardawelti render examples/lightning_bolt.json -o bolt.png

# With your own artwork:
./target/release/kardawelti render examples/lightning_bolt.json my_art.png -o bolt.png

# With Beleren installed in a directory:
./target/release/kardawelti render examples/lightning_bolt.json my_art.png \
    --fonts-dir assets/fonts -o bolt.png

# With Scryfall mana SVGs (fetch one-off into the directory first):
./target/release/kardawelti render examples/lightning_bolt.json my_art.png \
    --symbols-dir /path/to/scryfall_symbols -o bolt.png
```

## Layout

```
kardawelti/
├── Cargo.toml                         # workspace
├── crates/
│   ├── kardawelti-core/               # library: geometry, scryfall, render
│   └── kardawelti/                    # binary: CLI
└── examples/
    └── lightning_bolt.json
```
