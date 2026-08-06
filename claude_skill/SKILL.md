---
name: kinsaga
description: >
  Use when the user wants to manage a family chronicle with kinsaga —
  timestamped, categorized life events (facts) per person in a JSON file.
  Triggers for "add a fact/event", "show a timeline", "search the
  chronicle", "validate the chronicle", "merge chronicles", "add/remove a
  person", "fix fact UUIDs", or GPS/location validation of chronicle data.
allowed-tools: Bash(kinsaga *)
metadata:
  # Crate version this reference was last verified against. The
  # `skill_doc_sync` integration test fails if this drifts from
  # Cargo.toml — bump it here when cutting a new kinsaga version.
  documents-version: "0.5.0"
---

# kinsaga — family chronicle CLI (persons × timestamped facts)

One JSON file is the whole chronicle. Every command needs it, resolved in
this order: `--input <FILE>` / `-i` → `KINSAGA_INPUT` env var → `.env` in
the working directory. `kinsaga schema` is the one exception (prints the
JSON Schema, no file needed). Saves are atomic (temp file + rename) and
auto-stamp `last_updated`.

## Scripting contract (why kinsaga is safe to pipe)

- **stdout carries only data** (tables, CSV/MD/JSON payloads, corrected
  JSON); all progress/reports go to **stderr**. Redirecting stdout always
  yields a clean, parseable file.
- Query commands with `--format json|csv` emit their payload even for
  zero results (`[]` / bare header row).
- `kinsaga validate` exits non-zero when **error**-level issues remain;
  `--strict` fails on warnings too (`kinsaga validate --strict && …` is
  the CI gate). GPS outcomes are informational unless `--strict --gps`.
- Every mutating command takes `--dry-run` (preview on stderr, file
  untouched).
- Mutating commands echo the affected fact **UUID** on stderr — capture it
  from that output instead of re-searching.

## Command reference

### Query

```
kinsaga list [-f text|csv|md|json]                     # Persons + fact counts
kinsaga timeline <PERSON> [-f FMT] [-c CATEGORY] [--from YEAR] [--to YEAR]
                 [--include-shared]                    # Chronological, year-grouped
kinsaga search <QUERY> [-f FMT] [-r|--regex]           # Across all persons; case-insensitive;
                                                       # matches text+location+attachment titles
```

`--include-shared` also shows facts owned by others that list this person
in their `with` field. Search results are date-sorted within each person.

### Facts (identified by UUID)

```
kinsaga add-fact <PERSON> -d <DATE> -c <CATEGORY> -t <TEXT>
    [-w a,b] [--propagate]                             # comma-sep person IDs; copy to each
    [--country C [--place P] [--lat N --lon N]]        # location (lat+lon need country)
    [--attach URL]... [--attach-type MIME] [--attach-title T]
    [--dry-run]
kinsaga edit-fact <UUID> [-d DATE] [-c CAT] [-t TEXT]
    [-w a,b | --clear-with]
    [--country C] [--place P] [--lat N] [--lon N]      # partial: unset fields keep current
    [--clear-location]
    [--add-attach URL... [--attach-type M] [--attach-title T]]
    [--remove-attach URL... | --clear-attachments]
    [--dry-run]
kinsaga remove-fact <UUID> [--dry-run]
```

### Persons

```
kinsaga add-person <ID> --name <NAME> [--dry-run]      # ID: ^[a-z][a-z0-9_-]*$
kinsaga remove-person <ID> [--force] [--dry-run]       # Refuses if referenced in
                                                       # 'with' unless --force (strips refs)
```

### Validate / repair

```
kinsaga validate [--strict]                            # Structure, UUIDs, refs, dates
kinsaga validate --correct [--in-place]                # Fix bad/duplicate UUIDs; corrected
                                                       # JSON → stdout unless --in-place
kinsaga validate --gps [--suggest [--apply [--in-place]]] [--strict]
```

`--correct` without `--in-place` always prints the **full** chronicle to
stdout (even when nothing needed fixing), so `> fixed.json` is complete.

### Merge

```
kinsaga merge <SOURCE_FILE> [--dry-run]
    [--on-conflict skip|overwrite|fail]                # category/person conflicts (default skip)
    [--duplicates skip|add]                            # same (date,category,text) (default skip)
    [--regenerate-uuids]
```

### Schema

```
kinsaga schema                                         # JSON Schema to stdout; no input file
```

## Dates and IDs

| Date form | Meaning |
|---|---|
| `1987` / `1987-03` / `1987-03-15` | year / month / day precision |
| any + `?` suffix (`1987?`) | uncertain |

Calendar-invalid days (`2023-02-31`) parse but `validate` warns. Fact IDs
are UUIDv4 (generated on add). Person/category IDs match
`^[a-z][a-z0-9_-]*$`.

## Shared facts — the one subtle model

`--propagate` creates an **independent copy** per `with`-person (own UUID,
reciprocal `with` lists). Nothing links the copies afterwards:
`edit-fact`/`remove-fact` touch one copy only. `validate` warns when
reciprocal copies on the same date drift in category/location; text
wording and attachments are legitimately per-side (perspective phrasing:
"Married Bob" / "Married Alice"). To change a shared event, edit every
copy (find them via `search`, they share the date).

## Workflow tips & gotchas

- Check IDs before writing: `kinsaga list` for persons; categories are in
  the file's `categories` array (unknown category on `add-fact` errors and
  prints the available ones).
- Prefer `--dry-run` first when acting on a user's real chronicle file.
- **Privacy: `validate --gps` sends stored coordinates and place names to
  the public OpenStreetMap Nominatim service** (rate-limited 1 req/sec, so
  N locations ≈ N seconds). Ask before running it on personal data; a
  self-hosted instance is supported via the library
  (`NominatimClient::with_base_url`).
- `--gps --suggest --apply` applies the *top-ranked* candidate per
  location — review the stderr candidate list (or use `--dry-run`-style
  stdout redirect first) before `--in-place`.
- Attachment URLs must have a scheme (`file://`, `https://`, `s3://`).
  Malformed URLs already in the file are validate-warnings, not load
  failures.
- Merge dedup key is `(date, category, text)` — facts differing only in
  location/attachments count as duplicates and are skipped by default.
