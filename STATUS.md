# Kinsaga - Project Status

**Last Updated:** 2026-07-14 (correctness fixes + CLI contract, see CODE_REVIEW.md)

## Overview

Kinsaga is a family chronicle library and CLI for managing timestamped, categorized life events. Phase A MVP (Library + CLI) and Phase B1 (GPS validation via Nominatim) are complete.

## Completed Features

### Core Library (`src/lib.rs`)

The library is designed for reuse by different UI implementations (CLI, web, GUI).

- **Data Model** (`src/model.rs`): Chronicle, Person, Fact, Category, Location, Coordinates, Attachment structs with serde serialization
- **Date Parsing** (`src/date.rs`): ISO 8601 dates with optional `?` suffix for uncertainty (e.g., "1987", "1987-03", "1987-03-15", "1987?")
- **JSON I/O** (`src/io.rs`): load/save from files, from_json/to_json for strings
- **Search/Filter** (`src/filter.rs`): Filter by category, year range, text; search across all persons; chronological sorting
- **Validation** (`src/validate.rs`): Chronicle validation with detailed issue reporting; UUID correction
- **Merge** (`src/merge.rs`): Merge chronicles with conflict resolution strategies
- **Facts** (`src/facts.rs`): Fact creation with validation, propagation to related persons, timeline collection
- **Formatting** (`src/format.rs`): Display helpers for locations, attachments, dates, CSV escaping
- **Geocoding** (`src/geocode.rs`): Nominatim integration for GPS validation and coordinate lookup

### Library API Highlights

| Module | Key Functions | Description |
|--------|---------------|-------------|
| `validate` | `validate_chronicle()`, `correct_uuids()` | Validate chronicle structure, fix invalid UUIDs |
| `merge` | `merge_chronicles()` | Merge two chronicles with configurable conflict handling |
| `facts` | `add_fact()`, `collect_timeline_facts()` | Add facts with validation/propagation, gather timeline data |
| `format` | `format_location()`, `format_attachment()` | Format data for display |
| `filter` | `filter_facts()`, `search()` | Filter and search facts |
| `io` | `load()`, `save()` | JSON file I/O |
| `geocode` | `NominatimClient`, `fuzzy_match()` | GPS validation and forward/reverse geocoding |

### CLI (`src/main.rs`)

#### Global Options
| Option | Env Var | Description |
|--------|---------|-------------|
| `--input <file>` / `-i` | `KINSAGA_INPUT` | Path to chronicle JSON file (also reads from .env) |

#### Commands
| Command | Description |
|---------|-------------|
| `kinsaga list` | List all persons (id, name, fact count) |
| `kinsaga timeline <person>` | Show timeline for a person |
| `kinsaga search <query>` | Search text across all persons |
| `kinsaga validate` | Validate chronicle structure, references, and UUIDs |
| `kinsaga add-fact <person>` | Add a new fact to a person's timeline |
| `kinsaga edit-fact <uuid>` | Edit an existing fact by UUID |
| `kinsaga remove-fact <uuid>` | Remove a fact by UUID |
| `kinsaga add-person <id>` | Add a new person (`--name <name>`) |
| `kinsaga remove-person <id>` | Remove a person and all their facts (`--force` strips 'with' references) |
| `kinsaga merge <source>` | Merge another chronicle JSON file into the main chronicle |
| `kinsaga schema` | Print JSON Schema for chronicle files (no input file required) |

#### Command Options
| Option | Commands | Description |
|--------|----------|-------------|
| `--format <fmt>` / `-f` | list, timeline, search | Output format: `text` (default), `csv`, `md`, `json` |
| `--regex` / `-r` | search | Treat query as a regex pattern (case-insensitive) |
| `--category <cat>` / `-c` | timeline | Filter by category |
| `--from <year>` | timeline | Filter from year (inclusive) |
| `--to <year>` | timeline | Filter to year (inclusive) |
| `--include-shared` | timeline | Include facts from others where this person is in their `with` field |
| `--correct` | validate | Generate valid UUIDs for invalid/missing/duplicate and output JSON to stdout |
| `--in-place` | validate | Write changes back to the input file (requires `--correct` or `--apply`) |
| `--gps` | validate | Validate GPS coordinates against Nominatim (reverse geocoding) |
| `--suggest` | validate | Suggest GPS coordinates for locations without them (requires `--gps`) |
| `--apply` | validate | Apply suggested GPS coordinates to the chronicle (requires `--suggest`) |
| `--strict` | validate | Exit non-zero on warnings, not just errors |
| `--date <date>` / `-d` | add-fact | Date (ISO 8601, required) |
| `--category <cat>` / `-c` | add-fact | Category ID (required) |
| `--text <text>` / `-t` | add-fact | Event description (required) |
| `--with <ids>` / `-w` | add-fact | Comma-separated person IDs involved |
| `--dry-run` | add-fact | Preview without saving to file |
| `--propagate` | add-fact | Also create the fact for each person in `--with` (with cross-references) |
| `--country <country>` | add-fact | Country where the event occurred |
| `--place <place>` | add-fact | Place name: city, address, landmark, etc. (requires `--country`) |
| `--lat <lat>` | add-fact | GPS latitude (requires `--country` and `--lon`) |
| `--lon <lon>` | add-fact | GPS longitude (requires `--country` and `--lat`) |
| `--attach <url>` | add-fact | Attachment URL (can be specified multiple times) |
| `--attach-type <mime>` | add-fact | MIME content type for attachments |
| `--attach-title <title>` | add-fact | Title/description for attachments |
| `--dry-run` | merge | Preview changes without saving |
| `--on-conflict <strategy>` | merge | How to handle category conflicts: `skip` (default), `overwrite`, `fail` |
| `--duplicates <strategy>` | merge | How to handle duplicate facts: `skip` (default), `add` |
| `--regenerate-uuids` | merge | Generate new UUIDs for all merged facts |
| `--date <date>` / `-d` | edit-fact | New date (ISO 8601) |
| `--category <cat>` / `-c` | edit-fact | New category ID |
| `--text <text>` / `-t` | edit-fact | New description text |
| `--with <ids>` / `-w` | edit-fact | Replace 'with' list (comma-separated) |
| `--clear-with` | edit-fact | Clear all 'with' references |
| `--country <country>` | edit-fact | Set/update country |
| `--place <place>` | edit-fact | Set/update place name |
| `--lat <lat>` | edit-fact | Set/update GPS latitude |
| `--lon <lon>` | edit-fact | Set/update GPS longitude |
| `--clear-location` | edit-fact | Clear location entirely |
| `--add-attach <url>` | edit-fact | Add attachment URL (conflicts with remove/clear flags) |
| `--attach-type <mime>` | edit-fact | MIME type for added attachments (requires `--add-attach`) |
| `--attach-title <title>` | edit-fact | Title for added attachments (requires `--add-attach`) |
| `--remove-attach <url>` | edit-fact | Remove attachment by URL |
| `--clear-attachments` | edit-fact | Clear all attachments |
| `--dry-run` | edit-fact | Preview without saving |
| `--name <name>` | add-person | Display name (required) |
| `--force` | remove-person | Remove even if referenced in 'with' (references are stripped) |
| `--dry-run` | add-person, remove-person, remove-fact | Preview without saving |

#### Output Streams & Exit Codes

- **stdout carries only data:** query output (`list`/`timeline`/`search`),
  JSON/CSV/Markdown payloads, corrected/updated JSON from
  `validate --correct`/`--apply`, and `schema`. All progress, reports, and
  status messages go to **stderr**, so redirecting stdout always yields a
  clean, parseable file (e.g. `kinsaga validate --correct > corrected.json`).
- `validate --correct`/`--apply` without `--in-place` always emit the full
  chronicle JSON to stdout — even when nothing needed fixing — so redirect
  workflows produce a complete file.
- **Exit codes:** `validate` exits non-zero when error-level issues remain
  (corrections applied via `--correct` count as resolved). With `--strict`,
  warnings also cause a non-zero exit. This makes
  `kinsaga validate --strict && …` usable in scripts and CI.

#### Validation Checks
| Check | Level | Description |
|-------|-------|-------------|
| Empty UUID | Warning | Fact has no ID |
| Invalid UUID format | Warning | ID doesn't parse as UUID |
| Duplicate UUID | Error | Same UUID used by multiple facts |
| Duplicate person ID | Error | Same person ID used by multiple persons |
| Duplicate category ID | Error | Same category ID defined multiple times |
| Unknown category | Warning | Fact references undefined category |
| Invalid date | Warning | Date doesn't match ISO 8601 format |
| Unknown person ref | Warning | `with` field references unknown person |
| Empty country | Warning | Location has empty country field |
| Invalid GPS coordinates | Warning | Coordinates outside valid range (lat: -90..90, lon: -180..180) |
| Invalid MIME type | Warning | Attachment content_type not in type/subtype format |
| Invalid attachment URL | Warning | Attachment URL doesn't parse (missing scheme etc.) |
| Invalid ID pattern | Warning | Person/category ID doesn't match `^[a-z][a-z0-9_-]*$` |

### Tests
- 130 unit tests + 22 CLI integration tests + 1 doc test (all passing)
- Test coverage across all library modules; integration tests cover exit
  codes, stream separation, and mutation roundtrips (`tests/cli.rs`)
- Test data uses fictional names (Alice Smith, Bob Johnson, Springfield, Shelbyville)

### Example Data
- `examples/sample-chronicle.json` - Sample chronicle with fictional Smith/Johnson family

## Project Structure

```
kinsaga/
├── Cargo.toml              # Single crate with lib + bin
├── STATUS.md               # This file
├── schema.json             # JSON Schema for chronicle files (embedded in CLI)
├── examples/
│   └── sample-chronicle.json
└── src/
    ├── lib.rs              # Library exports
    ├── main.rs             # CLI application (thin wrapper over library)
    ├── model.rs            # Data structures (Chronicle, Person, Fact, etc.)
    ├── date.rs             # Date parsing (ChronicleDate)
    ├── io.rs               # JSON I/O (load, save)
    ├── filter.rs           # Search/filter logic (FactFilter, search)
    ├── validate.rs         # Validation (validate_chronicle, correct_uuids)
    ├── merge.rs            # Merge operations (merge_chronicles)
    ├── facts.rs            # Fact operations (add_fact, collect_timeline_facts)
    ├── format.rs           # Display formatting (format_location, escape_csv)
    └── geocode.rs          # Nominatim geocoding (NominatimClient, fuzzy_match)
```

## Configuration

- **Edition:** Rust 2024
- **License:** GPL-3.0-or-later
- **Author:** Daniel Probst <daniel@probst.dev>
- **Repository:** https://git.sr.ht/~danprobst/kinsaga

## Dependencies

```toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "2.0"
uuid = { version = "1.23", features = ["v4"] }
clap = { version = "4.6", features = ["derive", "env"] }
dotenvy = "0.15"
colored = "3.1"
anyhow = "1.0"
log = "0.4"
env_logger = "0.11"
chrono = "0.4"
url = { version = "2.5", features = ["serde"] }
regex = "1.13"
tempfile = "3.27"
ureq = "3.3"
urlencoding = "2.1"

[dev-dependencies]
assert_cmd = "2.0"
predicates = "3.1"
```

## Schema Design

### JSON Structure
```json
{
  "version": "1.0",
  "title": "Optional Chronicle Title",
  "last_updated": "2025-12-26T14:30:00Z",
  "categories": [
    { "id": "education", "label": "Education & Job", "color": "#4A90D9" }
  ],
  "persons": [
    {
      "id": "alice",
      "name": "Alice Smith",
      "facts": [
        {
          "id": "uuid-here",
          "date": "1990-05-15",
          "category": "education",
          "text": "Event description",
          "with": ["bob"],
          "location": {
            "country": "France",
            "place": "Eiffel Tower, Paris",
            "coordinates": { "lat": 48.8566, "lon": 2.3522 }
          },
          "attachments": [
            {
              "url": "file:///photos/event.jpg",
              "content_type": "image/jpeg",
              "title": "Event photo"
            }
          ]
        }
      ]
    }
  ]
}
```

### Location Object
| Field | Required | Description |
|-------|----------|-------------|
| `country` | Yes | Country name or ISO code |
| `place` | No | Place name (city, address, landmark, venue, etc.) |
| `coordinates` | No | GPS coordinates object |
| `coordinates.lat` | Yes (if coordinates) | Latitude (-90 to 90) |
| `coordinates.lon` | Yes (if coordinates) | Longitude (-180 to 180) |

### Attachment Object
| Field | Required | Description |
|-------|----------|-------------|
| `url` | Yes | URL with scheme (file://, https://, s3://, etc.) |
| `content_type` | No | MIME type (e.g., "image/jpeg", "application/pdf") |
| `title` | No | Human-readable description |

### Date Formats
| Format | Example | Meaning |
|--------|---------|---------|
| Year only | `1987` | Sometime in 1987 |
| Year-month | `1987-03` | March 1987 |
| Full date | `1987-03-15` | March 15, 1987 |
| Uncertain | `1987?` | Approximately 1987 |

## Phase B (Geocoding & Future Work)

### B1: GPS Validation (Completed)

GPS validation via Nominatim (OpenStreetMap) is now implemented.

**Features:**
- Reverse geocoding: Validate existing GPS coordinates against stored country/place
- Forward geocoding: Suggest coordinates for locations without GPS
- Apply suggestions: Automatically add top-ranked coordinates to chronicle
- ISO country code matching: CH↔Switzerland/Schweiz, JP↔Japan/日本, etc.
- Rate limiting: 1 request/second per Nominatim ToS

**Implementation:**
- New module: `src/geocode.rs`
- Sync HTTP via `ureq` (lightweight, no async runtime needed)
- `NominatimClient` with `reverse_geocode()` and `forward_geocode()` methods
- `fuzzy_match()` for country/place name comparison with ISO code support

**CLI flags:**
| Flag | Description |
|------|-------------|
| `--gps` | Validate existing GPS coordinates via reverse geocoding |
| `--gps --suggest` | Also suggest coordinates for locations without GPS |
| `--gps --suggest --apply` | Apply top suggestions and output JSON to stdout |
| `--gps --suggest --apply --in-place` | Apply top suggestions and save to file |

**Example usage:**
```bash
# Validate existing GPS coordinates
kinsaga validate --gps

# Suggest coordinates for locations without GPS
kinsaga validate --gps --suggest

# Apply top suggestions (preview - outputs JSON to stdout)
kinsaga validate --gps --suggest --apply

# Apply top suggestions and save to file
kinsaga validate --gps --suggest --apply --in-place

# Redirect to new file
kinsaga validate --gps --suggest --apply > updated-chronicle.json
```

### B2: Interactive Editing (TUI)

Terminal-based editing with ratatui for:
- Adding/editing facts with forms
- Selecting from geocoding results
- Browsing timeline interactively

### B3: Web Interface

1. **WASM Bindings** - Compile core library to WebAssembly
2. **Web App** - Yew-based web interface for viewing/editing

Note: CSV export is now available via `--format csv` flag on list, timeline, and search commands.

## How to Build & Test

```bash
# Build
cargo build --release

# Run tests
cargo test

# Try CLI (using -i flag)
./target/release/kinsaga -i examples/sample-chronicle.json list
./target/release/kinsaga -i examples/sample-chronicle.json timeline alice
./target/release/kinsaga -i examples/sample-chronicle.json search "Springfield"
./target/release/kinsaga -i examples/sample-chronicle.json search "Springfield|Shelbyville" --regex
./target/release/kinsaga -i examples/sample-chronicle.json validate

# Or set environment variable
export KINSAGA_INPUT=examples/sample-chronicle.json
./target/release/kinsaga list
./target/release/kinsaga timeline alice --from 1990 --to 2000

# Or use .env file
echo "KINSAGA_INPUT=examples/sample-chronicle.json" > .env
./target/release/kinsaga list

# Output formats
./target/release/kinsaga -i examples/sample-chronicle.json list -f csv
./target/release/kinsaga -i examples/sample-chronicle.json list -f md
./target/release/kinsaga -i examples/sample-chronicle.json list -f json
./target/release/kinsaga -i examples/sample-chronicle.json timeline alice -f csv > alice.csv
./target/release/kinsaga -i examples/sample-chronicle.json search "Tokyo" -f json

# Validate and correct UUIDs (output to stdout)
./target/release/kinsaga -i examples/sample-chronicle.json validate --correct > corrected.json

# Validate and correct UUIDs (modify file in place)
./target/release/kinsaga -i examples/sample-chronicle.json validate --correct --in-place

# Enable logging
RUST_LOG=warn ./target/release/kinsaga -i examples/sample-chronicle.json validate

# Add facts
./target/release/kinsaga -i examples/sample-chronicle.json add-fact alice -d 2020-06-15 -c family -t "Graduated from university"
./target/release/kinsaga -i examples/sample-chronicle.json add-fact alice -d 2021 -c travel -t "Trip to Paris" -w bob
./target/release/kinsaga -i examples/sample-chronicle.json add-fact alice -d 1995? -c education -t "Started school" --dry-run

# Add fact and propagate to all persons in --with
./target/release/kinsaga -i examples/sample-chronicle.json add-fact alice -d 2024-06-15 -c family -t "Family reunion" -w bob --propagate

# Add fact with location
./target/release/kinsaga -i examples/sample-chronicle.json add-fact alice -d 2024-07 -c travel -t "Visited Paris" --country France --place Paris --lat 48.8566 --lon 2.3522

# Add fact with attachments
./target/release/kinsaga -i examples/sample-chronicle.json add-fact alice -d 2024-08-20 -c family -t "Birthday party" --attach "file:///photos/birthday.jpg" --attach-type image/jpeg --attach-title "Birthday cake"

# Edit existing fact (by UUID)
./target/release/kinsaga -i examples/sample-chronicle.json edit-fact a7b8c9d0-e1f2-3456-0123-567890123456 --text "Amazing trip to Japan" --dry-run
./target/release/kinsaga -i examples/sample-chronicle.json edit-fact a7b8c9d0-e1f2-3456-0123-567890123456 --date 2018-03 --place "Kyoto"
./target/release/kinsaga -i examples/sample-chronicle.json edit-fact a7b8c9d0-e1f2-3456-0123-567890123456 --clear-location
./target/release/kinsaga -i examples/sample-chronicle.json edit-fact a7b8c9d0-e1f2-3456-0123-567890123456 --add-attach "file:///photos/2018/mt-fuji.jpg"

# Merge chronicles
./target/release/kinsaga -i examples/sample-chronicle.json merge other-chronicle.json --dry-run
./target/release/kinsaga -i examples/sample-chronicle.json merge other-chronicle.json --on-conflict overwrite
./target/release/kinsaga -i examples/sample-chronicle.json merge other-chronicle.json --duplicates add --regenerate-uuids

# Print JSON Schema (no input file required)
./target/release/kinsaga schema
./target/release/kinsaga schema > chronicle-schema.json

# GPS validation (requires network access, rate-limited to 1 req/sec)
./target/release/kinsaga -i examples/sample-chronicle.json validate --gps
./target/release/kinsaga -i examples/sample-chronicle.json validate --gps --suggest
./target/release/kinsaga -i examples/sample-chronicle.json validate --gps --suggest --apply
```

## Design Decisions

| Topic | Decision |
|-------|----------|
| Date format | ISO 8601 + `?` suffix for uncertainty |
| Fact IDs | UUID |
| Shared facts | Duplicate per person + optional `with` field |
| Validation | Rust structs (no separate JSON Schema file) |
| Categories | User-defined in JSON |
| Storage | Local JSON file |
| Editing | `add-fact` command or text editor for manual edits |
| Last updated | Auto-set to UTC timestamp on every save |
| Library/CLI separation | All business logic in library modules; CLI only handles I/O and formatting |

## Privacy Note

Real family data should NOT be committed to the repository. The example file and all tests use fictional data only.
