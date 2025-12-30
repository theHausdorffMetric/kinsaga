# Kinsaga - Project Status

**Last Updated:** 2025-12-30

## Overview

Kinsaga is a family chronicle library and CLI for managing timestamped, categorized life events. The Phase A MVP (Library + CLI) is complete.

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

### Library API Highlights

| Module | Key Functions | Description |
|--------|---------------|-------------|
| `validate` | `validate_chronicle()`, `correct_uuids()` | Validate chronicle structure, fix invalid UUIDs |
| `merge` | `merge_chronicles()` | Merge two chronicles with configurable conflict handling |
| `facts` | `add_fact()`, `collect_timeline_facts()` | Add facts with validation/propagation, gather timeline data |
| `format` | `format_location()`, `format_attachment()` | Format data for display |
| `filter` | `filter_facts()`, `search()` | Filter and search facts |
| `io` | `load()`, `save()` | JSON file I/O |

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
| `kinsaga merge <source>` | Merge another chronicle JSON file into the main chronicle |
| `kinsaga schema` | Print JSON Schema for chronicle files (no input file required) |

#### Command Options
| Option | Commands | Description |
|--------|----------|-------------|
| `--format <fmt>` / `-f` | list, timeline, search | Output format: `text` (default), `csv`, `md`, `json` |
| `--category <cat>` / `-c` | timeline | Filter by category |
| `--from <year>` | timeline | Filter from year (inclusive) |
| `--to <year>` | timeline | Filter to year (inclusive) |
| `--include-shared` | timeline | Include facts from others where this person is in their `with` field |
| `--correct` | validate | Generate valid UUIDs for invalid/missing/duplicate and output JSON to stdout |
| `--in-place` | validate | Write corrected JSON back to the input file (requires `--correct`) |
| `--date <date>` / `-d` | add-fact | Date (ISO 8601, required) |
| `--category <cat>` / `-c` | add-fact | Category ID (required) |
| `--text <text>` / `-t` | add-fact | Event description (required) |
| `--with <ids>` / `-w` | add-fact | Comma-separated person IDs involved |
| `--dry-run` | add-fact | Preview without saving to file |
| `--propagate` | add-fact | Also create the fact for each person in `--with` (with cross-references) |
| `--country <country>` | add-fact | Country where the event occurred |
| `--name <name>` | add-fact | Place name: city, address, landmark, etc. (requires `--country`) |
| `--lat <lat>` | add-fact | GPS latitude (requires `--country` and `--lon`) |
| `--lon <lon>` | add-fact | GPS longitude (requires `--country` and `--lat`) |
| `--attach <url>` | add-fact | Attachment URL (can be specified multiple times) |
| `--attach-type <mime>` | add-fact | MIME content type for attachments |
| `--attach-title <title>` | add-fact | Title/description for attachments |
| `--dry-run` | merge | Preview changes without saving |
| `--on-conflict <strategy>` | merge | How to handle category conflicts: `skip` (default), `overwrite`, `fail` |
| `--duplicates <strategy>` | merge | How to handle duplicate facts: `skip` (default), `add` |
| `--regenerate-uuids` | merge | Generate new UUIDs for all merged facts |

#### Validation Checks
| Check | Level | Description |
|-------|-------|-------------|
| Empty UUID | Warning | Fact has no ID |
| Invalid UUID format | Warning | ID doesn't parse as UUID |
| Duplicate UUID | Error | Same UUID used by multiple facts |
| Unknown category | Warning | Fact references undefined category |
| Invalid date | Warning | Date doesn't match ISO 8601 format |
| Unknown person ref | Warning | `with` field references unknown person |
| Empty country | Warning | Location has empty country field |
| Invalid GPS coordinates | Warning | Coordinates outside valid range (lat: -90..90, lon: -180..180) |
| Invalid MIME type | Warning | Attachment content_type not in type/subtype format |

### Tests
- 63 unit tests + 1 doc test (all passing)
- Test coverage across all library modules
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
    └── format.rs           # Display formatting (format_location, escape_csv)
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
thiserror = "1.0"
uuid = { version = "1.0", features = ["v4"] }
clap = { version = "4.4", features = ["derive", "env"] }
dotenvy = "0.15"
colored = "2.1"
anyhow = "1.0"
log = "0.4"
env_logger = "0.11"
chrono = "0.4"
url = "2.5"

[dev-dependencies]
tempfile = "3.15"
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
            "name": "Eiffel Tower, Paris",
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
| `name` | No | Place name (city, address, landmark, venue, etc.) |
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

## Phase B (Future Work)

Not yet started. Planned features:

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
./target/release/kinsaga -i examples/sample-chronicle.json add-fact alice -d 2024-07 -c travel -t "Visited Paris" --country France --name Paris --lat 48.8566 --lon 2.3522

# Add fact with attachments
./target/release/kinsaga -i examples/sample-chronicle.json add-fact alice -d 2024-08-20 -c family -t "Birthday party" --attach "file:///photos/birthday.jpg" --attach-type image/jpeg --attach-title "Birthday cake"

# Merge chronicles
./target/release/kinsaga -i examples/sample-chronicle.json merge other-chronicle.json --dry-run
./target/release/kinsaga -i examples/sample-chronicle.json merge other-chronicle.json --on-conflict overwrite
./target/release/kinsaga -i examples/sample-chronicle.json merge other-chronicle.json --duplicates add --regenerate-uuids

# Print JSON Schema (no input file required)
./target/release/kinsaga schema
./target/release/kinsaga schema > chronicle-schema.json
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
