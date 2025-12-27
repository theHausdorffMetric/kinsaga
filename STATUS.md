# Kinsaga - Project Status

**Last Updated:** 2025-12-26

## Overview

Kinsaga is a family chronicle library and CLI for managing timestamped, categorized life events. The Phase A MVP (Library + CLI) is complete.

## Completed Features

### Core Library (`src/lib.rs`)
- **Data Model** (`src/model.rs`): Chronicle, Person, Fact, Category structs with serde serialization
- **Date Parsing** (`src/date.rs`): ISO 8601 dates with optional `?` suffix for uncertainty (e.g., "1987", "1987-03", "1987-03-15", "1987?")
- **JSON I/O** (`src/io.rs`): load/save from files, from_json/to_json for strings
- **Search/Filter** (`src/filter.rs`): Filter by category, year range, text; search across all persons; chronological sorting

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

#### Command Options
| Option | Commands | Description |
|--------|----------|-------------|
| `--format <fmt>` / `-f` | list, timeline, search | Output format: `text` (default), `csv`, `md` |
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

#### Validation Checks
| Check | Level | Description |
|-------|-------|-------------|
| Empty UUID | Warning | Fact has no ID |
| Invalid UUID format | Warning | ID doesn't parse as UUID |
| Duplicate UUID | Error | Same UUID used by multiple facts |
| Unknown category | Warning | Fact references undefined category |
| Invalid date | Warning | Date doesn't match ISO 8601 format |
| Unknown person ref | Warning | `with` field references unknown person |

### Tests
- 22 unit tests + 1 doc test (all passing)
- Test data uses fictional names (Alice Smith, Bob Johnson, Springfield, Shelbyville)

### Example Data
- `examples/sample-chronicle.json` - Sample chronicle with fictional Smith/Johnson family

## Project Structure

```
kinsaga/
├── Cargo.toml              # Single crate with lib + bin
├── STATUS.md               # This file
├── examples/
│   └── sample-chronicle.json
└── src/
    ├── lib.rs              # Library exports
    ├── main.rs             # CLI application
    ├── model.rs            # Data structures
    ├── date.rs             # Date parsing
    ├── io.rs               # JSON I/O
    └── filter.rs           # Search/filter logic
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
          "with": ["bob"]  // optional: other person IDs
        }
      ]
    }
  ]
}
```

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
./target/release/kinsaga -i examples/sample-chronicle.json timeline alice -f csv > alice.csv

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

## Privacy Note

Real family data should NOT be committed to the repository. The example file and all tests use fictional data only.
