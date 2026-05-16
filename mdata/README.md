# mdata

Scryfall data manager for the mechoe toolchain. Maintains a local cache of card JSON and art-crop images so downstream tools (like `mcard`) can work fully offline after an initial sync.

## Overview

`mdata` downloads Scryfall's bulk card data once, builds a SQLite search index from it, then serves individual card files on demand. The bulk file is discarded after indexing; all card JSON is stored inside the index. Art crops are fetched lazily from Scryfall when a card is first requested.

```
mdata sync                        # one-time setup (~540 MB download)
mdata fetch "Pouncing Jaguar"     # → data/USG/269.json + data/USG/269.jpg
mcard render data/USG/269.json data/USG/269.jpg -o jaguar.png
```

## Architecture

```
mechoe/
  mdata/                          ← this workspace
    crates/
      mdata-core/                 ← library: all data logic
      mdata/                      ← binary: CLI front-end
  data/                           ← gitignored local cache
    _meta/
      index.db                    ← SQLite index (all card metadata + JSON)
    USG/
      269.json                    ← Scryfall card JSON (extracted from index)
      269.jpg                     ← art-crop image (fetched from Scryfall)
    DRK/
      30.json
      30.jpg
```

### Data flow

```
Scryfall bulk API
      │
      ▼
 mdata sync
      │  downloads default_cards.json.gz (~540 MB)
      │  stream-parses JSON array (O(1 card) memory)
      │  inserts each card into SQLite
      │  deletes bulk file
      ▼
 data/_meta/index.db              ← persistent, ~200 MB

 mdata fetch / get / pull
      │  queries index for metadata + stored card JSON
      │  writes data/<SET>/<NUM>.json from index
      │  fetches data/<SET>/<NUM>.jpg from Scryfall image server (if not cached)
      ▼
 data/<SET>/<NUM>.json + .jpg     ← ready for mcard render
```

### Why `default_cards` bulk data

Scryfall offers several bulk exports. `default_cards` (~540 MB compressed) is chosen because it contains every English printing of every card — enabling oldest-printing lookup, per-set pulls, and full card JSON extraction, all without additional API calls after sync.

| Bulk type | Size | Content |
|---|---|---|
| `oracle_cards` | 173 MB | One per oracle ID (default printing only) |
| `unique_artwork` | 253 MB | One per unique artwork |
| **`default_cards`** | **540 MB** | **Every English printing — used by mdata** |
| `all_cards` | 2.5 GB | Every card in every language |

## Data types

### `CardRecord` (in `mdata-core::index`)

The in-memory representation of one row in the SQLite index.

| Field | Type | Description |
|---|---|---|
| `id` | `String` | Scryfall UUID — globally unique per printing |
| `oracle_id` | `Option<String>` | Groups all printings of the same card |
| `name` | `String` | Full card name, e.g. `"Pouncing Jaguar"` |
| `set_code` | `String` | Uppercase set code, e.g. `"USG"` |
| `collector_number` | `String` | Collector number string, e.g. `"269"` |
| `released_at` | `String` | ISO date `"YYYY-MM-DD"` — used for oldest-first sorting |
| `art_crop_url` | `Option<String>` | Scryfall art-crop image URL; `None` for cards with no art |
| `card_json` | `String` | Full Scryfall card JSON text — written to disk on demand |

### `CardPaths` (in `mdata-core::store`)

Returned by `ensure_card` after files are confirmed on disk.

| Field | Type | Description |
|---|---|---|
| `json` | `PathBuf` | Absolute path to `data/<SET>/<NUM>.json` |
| `art` | `Option<PathBuf>` | Absolute path to `data/<SET>/<NUM>.jpg`, or `None` |

### SQLite schema (`data/_meta/index.db`)

```sql
CREATE TABLE cards (
    id               TEXT PRIMARY KEY,
    oracle_id        TEXT,
    name             TEXT NOT NULL,
    set_code         TEXT NOT NULL,
    collector_number TEXT NOT NULL,
    released_at      TEXT NOT NULL,   -- ISO date, sortable as text
    art_crop_url     TEXT,
    card_json        TEXT NOT NULL    -- full Scryfall JSON for mcard rendering
);
CREATE INDEX idx_name   ON cards(name COLLATE NOCASE);
CREATE INDEX idx_set    ON cards(set_code, collector_number);
CREATE INDEX idx_oracle ON cards(oracle_id, released_at);
```

Rows are keyed on Scryfall's `id` (per-printing UUID), so the same card name from different sets appears as separate rows. The `idx_name` index enables fast case-insensitive name lookup. `idx_oracle` enables grouping all printings of a card and selecting the oldest by `released_at`.

## Workspace structure

```
mdata/
  Cargo.toml                        workspace root

  crates/
    mdata-core/
      Cargo.toml
      src/
        lib.rs                      public re-exports
        error.rs                    MdataError enum
        paths.rs                    data dir resolution; path helpers
        http.rs                     Scryfall HTTP client (rate-limited, streaming)
        bulk.rs                     bulk manifest fetch, download, stream-parse
        index.rs                    SQLite open/create/schema/query
        store.rs                    ensure_card: write JSON + fetch art lazily

    mdata/
      Cargo.toml
      src/
        main.rs                     CLI (clap): sync, fetch, get, pull, info
```

### Module responsibilities

**`paths`** — single source of truth for where files live. All other modules call into here rather than constructing paths ad hoc.

**`http`** — wraps `ureq` with a 150 ms inter-request floor (Scryfall's guideline is ≥50 ms; we're conservative). HTTP 429 surfaces as `MdataError::RateLimited` and halts the current operation cleanly.

**`bulk`** — uses `serde_json::Deserializer::deserialize_seq` with a custom `Visitor` to stream-parse the JSON array one card at a time. Peak memory during sync is O(one card + SQLite write buffer), not O(full file).

**`index`** — all SQLite operations. `create_index` creates the DB and schema; `open_index` opens an existing one and errors with `MdataError::NoIndex` if it doesn't exist yet (prompting the user to run `mdata sync`). Inserts run inside an explicit `BEGIN`/`COMMIT` transaction for speed (~30 k rows would be very slow in autocommit mode).

**`store`** — `ensure_card` is the main integration point: given a `CardRecord` from the index and a `Client`, it writes the card JSON to disk (from the stored `card_json` field — no extra API call) and fetches the art crop if not already cached. Both writes are idempotent: existing files are never overwritten.

## CLI reference

```
mdata [--data-dir <DIR>] <COMMAND>
```

`--data-dir` overrides the `MECHOE_DATA` environment variable. Default is `./data` relative to the current working directory.

### `mdata sync`

Downloads the `default_cards` bulk export from Scryfall, stream-parses it into the local SQLite index, then deletes the bulk file. Must be run at least once before other commands work. Safe to re-run — the old index is replaced atomically.

```
mdata sync
# [sync] fetching bulk-data manifest ...
# [sync] default_cards updated 2026-05-07T09:13:37.519+00:00
# [mdata] downloading bulk data → data/_meta/default_cards.json.gz
# [mdata] downloaded 538 MB
# [sync] building index → data/_meta/index.db ...
# [mdata] indexed 10000 cards ...
# [mdata] indexed 20000 cards ...
# [sync] indexed 31247 cards
# [sync] removed bulk file (card JSON preserved in index)
```

### `mdata fetch "<name>"`

Look up a card by exact name (case-insensitive) and fetch the oldest printing. Prints the paths of the written files.

```
mdata fetch "Pouncing Jaguar"
# Pouncing Jaguar (USG) #269
# json: data/USG/269.json
# art:  data/USG/269.jpg

mdata fetch "Pouncing Jaguar" --info-only
# Pouncing Jaguar — 3 printing(s)
#   USG #269    1998-10-12
#   ...
```

### `mdata get <SET> <NUM>`

Fetch a specific printing by set code and collector number.

```
mdata get DRK 30
# Leviathan (DRK) #30 — 1994-04-01
# json: data/DRK/30.json
# art:  data/DRK/30.jpg
```

### `mdata pull <SET>`

Fetch JSON and art for every card in a set. Skips files already on disk.

```
mdata pull DRK
# [pull] 119 cards in set DRK
#   [1/119] DRK #1 — Amnesia
#   ...
# set DRK — 119 cards (119 new JSON, 119 new art, 0 skipped)
```

### `mdata info "<name>"`

Show all printings of a card from the index without downloading anything. A `✓` marks printings already cached locally.

```
mdata info "Lightning Bolt"
# Lightning Bolt — 18 printing(s)
#   [✓] LEA #161   1993-08-05
#   [ ] LEB #162   1993-10-04
#   ...
```

## Environment

| Variable | Default | Description |
|---|---|---|
| `MECHOE_DATA` | `./data` | Root of the local data cache |

## Development phases

### Phase 1 — Bulk sync and index (current) ✅
- Download `default_cards` bulk export from Scryfall
- Stream-parse JSON array into SQLite index
- `sync`, `fetch`, `get`, `pull`, `info` CLI commands
- Offline-first: once synced, name→card lookup requires no network
- Art fetched lazily per card from Scryfall image server

### Phase 2 — Fuzzy name search
- Add SQLite FTS5 virtual table on `name`
- `mdata fetch "pouncing jagr"` works via full-text search
- Ranked results when multiple cards match

### Phase 3 — Incremental sync
- Track `updated_at` timestamp from the bulk manifest
- Skip re-download if local index is already current
- `mdata sync --force` to override

### Phase 4 — `mdata` as a library dependency
- `mcard` takes an optional `mdata-core` path dep so `mcard fetch-render` can combine lookup + render in one command
- Shared `CardRecord` type replaces mcard's local Scryfall serde struct

### Phase 5 — Set metadata
- Download Scryfall set objects (`/sets`) into a second `sets` table
- `mdata sets` command: list all sets with name, release date, card count
- `mdata pull --set-name "Urza's Saga"` lookup by full name

## Build

```sh
cd mechoe/mdata
cargo build --release
```

First build downloads and compiles `rusqlite` with bundled SQLite (fast — seconds, not minutes). The resulting binary is at `target/release/mdata`.

## Rate limiting

All Scryfall requests observe a 150 ms minimum gap. HTTP 429 responses surface as an error and halt the current operation immediately; re-running resumes safely because all file writes are idempotent (existing files are skipped).
