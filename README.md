# Kinsaga

A family chronicle library and CLI for managing timestamped, categorized life events.

## Overview

Kinsaga (kin + saga) provides data structures and utilities for creating and managing family chronicles where life events are:

- **Timestamped** with flexible date precision (year, month, day, or uncertain with `?` suffix)
- **Categorized** into customizable life themes (education, family, travel, hobbies, etc.)
- **Person-centric** - each family member has their own timeline
- **Location-aware** - optional GPS coordinates with Nominatim geocoding support
- **Attachment-friendly** - link photos, documents, and other media to events

## Installation

```bash
cargo build --release
```

## Quick Start

```bash
# Set your chronicle file
export KINSAGA_INPUT=examples/sample-chronicle.json

# List all persons
kinsaga list

# View someone's timeline
kinsaga timeline alice

# Search across all persons
kinsaga search "Tokyo"

# Search with regex
kinsaga search "Tokyo|Kyoto" --regex

# Validate chronicle structure
kinsaga validate

# Validate GPS coordinates against Nominatim
kinsaga validate --gps

# Add a new fact
kinsaga add-fact alice -d 2024-07-15 -c travel -t "Trip to Paris" --country France --place Paris
```

## Features

### CLI Commands

| Command | Description |
|---------|-------------|
| `list` | List all persons with fact counts |
| `timeline <person>` | Show chronological timeline for a person |
| `search <query>` | Search text across all persons (supports `--regex`) |
| `validate` | Validate structure, UUIDs, references, and GPS coordinates |
| `add-fact <person>` | Add a new fact with date, category, text, location, attachments |
| `edit-fact <uuid>` | Edit an existing fact by UUID |
| `merge <file>` | Merge another chronicle into this one |
| `schema` | Print JSON Schema for chronicle files |

### Output Formats

All query commands support `--format` / `-f` with: `text` (default), `csv`, `md`, `json`

### Scripting

stdout carries only data; all progress and reports go to stderr, so redirects
always yield clean output. `validate` exits non-zero when errors are found
(`--strict` also fails on warnings):

```bash
kinsaga validate --strict && echo "chronicle ok"
kinsaga validate --correct > corrected.json
```

### GPS Validation

Validate and enrich location data using OpenStreetMap's Nominatim:

```bash
# Check existing coordinates match stored country/place
kinsaga validate --gps

# Suggest coordinates for locations without GPS
kinsaga validate --gps --suggest

# Apply top suggestions to chronicle
kinsaga validate --gps --suggest --apply --in-place
```

Supports ISO country codes (CH, JP, US, etc.) and native language names (Schweiz, 日本, etc.).

## Chronicle Format

```json
{
  "version": "1.0",
  "title": "Family Chronicle",
  "categories": [
    { "id": "travel", "label": "Travels", "color": "#81C784" }
  ],
  "persons": [
    {
      "id": "alice",
      "name": "Alice Smith",
      "facts": [
        {
          "id": "uuid-here",
          "date": "2024-07-15",
          "category": "travel",
          "text": "Trip to Paris",
          "location": {
            "country": "France",
            "place": "Paris",
            "coordinates": { "lat": 48.8566, "lon": 2.3522 }
          }
        }
      ]
    }
  ]
}
```

### Date Formats

| Format | Example | Meaning |
|--------|---------|---------|
| Year | `1987` | Sometime in 1987 |
| Year-month | `1987-03` | March 1987 |
| Full date | `1987-03-15` | March 15, 1987 |
| Uncertain | `1987?` | Approximately 1987 |

## Documentation

See [STATUS.md](STATUS.md) for detailed documentation including:
- Complete CLI reference with all options
- Library API documentation
- Validation checks
- Project roadmap

## License

Licensed under GPL-3.0-or-later.
