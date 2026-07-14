# Kinsaga — Code Review Findings & Implementation Plan

**Date:** 2026-07-14
**Reviewed:** v0.2.0, commit `712daa2` (clean working tree)
**Scope:** Full codebase — architecture and implementation (all 11 source modules, schema, sample data, docs)
**Method:** Complete source read; `cargo test` (87 unit + 1 doc test, all passing) and `cargo clippy` (clean) executed; findings F1 and F2 **verified by execution** against the built library.

---

## 1. Summary

The overall shape of the project is good: clean module boundaries, a real library/CLI
split for most features, thoughtful merge semantics, and consistent test hygiene.
The review found:

- **2 confirmed correctness bugs** (reachable panic; GPS validation defeated by the fuzzy matcher)
- **1 data-safety gap** (non-atomic save of the single data file)
- **Several CLI contract problems** (exit codes, stdout pollution breaking documented workflows, flag-interaction bugs)
- **1 architecture drift** (GPS logic in `main.rs` contradicting the project's own library-first rule)
- Assorted robustness, duplication, and doc-drift items

Findings are numbered `F1..F32` and referenced by the implementation plan in section 4.

---

## 2. Findings

### 2.1 Critical — confirmed by execution

#### F1. `truncate_text` panics on multi-byte UTF-8 (reachable crash in `validate` and `merge`)

- **Where:** `src/format.rs:55`; private duplicates at `src/validate.rs:253` and `src/merge.rs:362`
- **What:** Truncation slices at a byte index: `&s[..max_len.saturating_sub(3)]`.
- **Proof:** `truncate_text("Wanderung auf die Mettmenalp, Schwändi GL", 38)` panics:
  `end byte index 35 is not a char boundary; it is inside 'ä' (bytes 34..36)`.
- **Impact:** `validate_chronicle` truncates every fact text to 30 bytes for warning
  messages (`validate.rs:89,97,105`); `merge` does the same for warnings. Any
  German/French/Japanese fact text longer than 30 bytes with a multi-byte character at
  the boundary crashes the CLI. The sample data itself contains "Schwändi".
- **Fix:** Cut on `char_indices()` (or floor the index to a char boundary). Keep one
  public implementation in `format.rs`; delete the two private copies.

#### F2. `fuzzy_match` substring containment lets ISO codes match wrong countries

- **Where:** `src/geocode.rs:339` (containment check runs before the ISO-code table)
- **Proof (all return `true`):**
  - `fuzzy_match("CH", "China")`
  - `fuzzy_match("US", "Russia")`
  - `fuzzy_match("IN", "Argentina")`
- **Impact:** `validate --gps` silently passes coordinates located in the wrong country
  whenever the stored country is a 2-letter code that is a substring of the Nominatim
  country name — defeating the purpose of GPS validation.
- **Fix:** Check the ISO table first; only allow the containment shortcut for strings
  above a minimum length (e.g. ≥ 3 chars), or restrict containment to place matching
  and never country codes.

### 2.2 High priority — confirmed by code reading

#### F3. `save()` is not atomic — data-loss risk for the only data file

- **Where:** `src/io.rs:28-34` (`fs::write` truncates then writes)
- **Impact:** A crash, full disk, or Ctrl-C mid-write leaves a corrupt or empty
  chronicle, with no backup. Every mutating command (`add-fact`, `edit-fact`, `merge`,
  `validate --in-place`) rewrites the file.
- **Fix:** Write to a temp file in the same directory, then rename over the target
  (`tempfile::NamedTempFile::persist`; `tempfile` is already a dev-dependency).

#### F4. `validate` always exits 0

- **Where:** `src/main.rs` `cmd_validate` (returns `Ok(())` unconditionally)
- **Impact:** Scripting/CI cannot rely on `kinsaga validate` (`&&` chains always
  proceed even with duplicate-UUID errors).
- **Fix:** Non-zero exit when error-level issues exist; optional `--strict` to also
  fail on warnings.

#### F5. Documented stdout-redirect workflows produce invalid JSON

- **Where:** `src/main.rs` `cmd_validate` — all report output (checkmarks, warnings,
  `--- Corrected JSON ---` / `--- Updated JSON ---` banners) goes to stdout, mixed
  with the JSON payload
- **Impact:** STATUS.md documents `validate --correct > corrected.json` and
  `validate --gps --suggest --apply > updated-chronicle.json`; both produce files that
  do not parse.
- **Fix:** Progress/reports → stderr; machine output (JSON/CSV) → stdout, no banners.
  Update STATUS.md examples.

#### F6. `--correct --in-place` combined with `--gps --suggest --apply --in-place` silently loses UUID corrections

- **Where:** `src/main.rs:880` (corrections applied to a clone and saved) vs
  `src/main.rs:1163` (GPS apply saves the *uncorrected* local `chronicle`, overwriting
  the corrected file)
- **Fix:** Assign the corrected chronicle back to `chronicle` before the GPS section.

#### F7. `correct_uuids` regenerates BOTH facts of a duplicate pair; dead first pass

- **Where:** `src/validate.rs:196-226`
- **What:** Corrections are matched by `(person_id, fact_id)`, so for a duplicate UUID
  the *first, legitimate* occurrence also matches and loses its stable ID (breaking
  external references, e.g. noted-down `edit-fact` UUIDs). Additionally, the first
  pass builds `used_uuids`, which is inserted into but **never read** — the intended
  collision check was never wired up.
- **Fix:** Record corrections by fact position (person index + fact index) or fix only
  the Nth occurrence; either implement the collision check or delete the dead pass.

#### F8. `ChronicleDate` violates the `Ord`/`Eq` consistency contract

- **Where:** `src/date.rs` — derived `PartialEq`/`Eq` compare `raw` + `uncertain`;
  manual `Ord` compares `sort_key()` only
- **Impact:** `"1987"` vs `"1987?"`: `cmp == Equal` but `!=`. Breaks the documented
  invariants of `Ord`; ordered collections (BTreeMap/BTreeSet, sort dedup) may
  misbehave subtly.
- **Fix:** Include tiebreaker fields in `cmp`, or implement `PartialEq` via `sort_key`
  deliberately and document it.

### 2.3 Design & robustness

#### F9. Strict `Url` in the model makes bad data unfixable and rewrites good data

- **Where:** `src/model.rs:111` (`Attachment.url: Url`)
- **Impact:** One malformed attachment URL fails deserialization of the entire file —
  so `validate` (the tool meant to report such problems) cannot even load it. `Url`
  also normalizes on parse (lowercases host, resolves ports), so `save()` can silently
  rewrite user data.
- **Fix (decision needed):** Store `String` and validate URL format in
  `validate_chronicle` (like the MIME check), or keep `Url` with a lenient custom
  deserializer. Breaking model change — schedule deliberately.

#### F10. No duplicate person/category ID validation; merge can create duplicates

- **Where:** `src/validate.rs` (no `DuplicatePersonId`/`DuplicateCategoryId` checks);
  `src/merge.rs:122-125` (`target_person_ids`/`target_category_ids` snapshotted up
  front, not updated as items are added)
- **Impact:** `find_person` silently returns the first match; merging a malformed
  source containing the same person ID twice adds two person entries to the target.
- **Fix:** Add error-level duplicate-ID checks to `validate_chronicle`; update the
  lookup sets during merge insertion.

#### F11. Nominatim client hardening

- **Where:** `src/geocode.rs`
- **Issues:**
  a) No request timeout configured (ureq 3 sets none by default) — a stalled
     connection hangs the CLI indefinitely.
  b) Nominatim returns `{"error": "Unable to geocode"}` with HTTP 200 (e.g. ocean
     coordinates); this surfaces as a confusing `ParseError` instead of a
     no-result case.
  c) Base URL is hard-coded — untestable without live network; blocks self-hosted
     instances.
- **Fix:** Configure agent timeouts; deserialize the error envelope into
  `GeocodeError::NoResults` (or a dedicated variant); accept `base_url` in the
  constructor.

#### F12. Invalid regex silently returns "no results"; regex recompiled per fact

- **Where:** `src/filter.rs:109-116` (`Regex::new` inside `matches()`, errors mapped
  to `false`)
- **Impact:** `kinsaga search '[invalid' --regex` prints "No results found" instead of
  an error; compilation is O(facts) instead of O(1).
- **Fix:** Compile once when the filter is built (store `Option<Regex>` or return
  `Result` from a `build()` step); surface compile errors to the user.

#### F13. Inconsistent sort order for unparseable dates

- **Where:** `src/filter.rs:158-168` (unparseable last) vs `src/facts.rs:300-304`
  (`Option` ordering → unparseable first)
- **Fix:** One shared comparator used by both entry points.

#### F14. `edit-fact` silently drops flags; no attach metadata on edit

- **Where:** `src/main.rs:1376-1392` (if/else chain: `--clear-attachments` beats
  `--add-attach` beats `--remove-attach`, losers silently ignored)
- **Fix:** Reject unsupported combinations explicitly (or support them). Add
  `--attach-type`/`--attach-title` counterparts for `--add-attach`.

#### F15. `add_fact` `with` edge cases

- **Where:** `src/facts.rs`
- **What:** `with` may contain the target person themselves or duplicate IDs
  (`-w bob,bob`); with `--propagate` this creates self-referencing or doubled facts.
- **Fix:** Validate: reject self-reference, dedupe the list.

#### F16. Merge reconstructs facts field-by-field

- **Where:** `src/merge.rs:255-269`
- **Impact:** Any future `Fact` field is silently dropped during merges.
- **Fix:** `let mut f = source_fact.clone(); f.id = new_uuid;`

### 2.4 Architecture

#### F17. GPS logic lives in `main.rs`, contradicting the library-first design

- **Where:** `src/main.rs` `cmd_validate` (~280 lines of GPS validate/suggest/apply
  orchestration); `src/geocode.rs:216` (`GpsValidationResult` + `is_mismatch()`
  exported but **never constructed** — main.rs re-implements the logic inline at
  `main.rs:940-952`)
- **Impact:** STATUS.md states "all business logic in library modules; CLI only
  handles I/O and formatting". The planned TUI (B2) and web (B3) frontends would have
  to reimplement GPS validation.
- **Fix:** Move `validate_gps(...) -> Vec<GpsValidationResult>` and
  `suggest_coordinates(...)` into the library; `cmd_validate` keeps only printing.
  This also decomposes the ~350-line `cmd_validate`.
- **Forward note (Phase B3/WASM):** `ureq` + `thread::sleep` do not compile to
  wasm32; the geocode module will eventually need a `cfg` gate or an HTTP-client
  trait.

#### F18. Three error-handling regimes

- **Where:** `thiserror` enums (`DateError`, `IoError`, `GeocodeError`); stringly
  structs (`MergeError`, `AddFactError` — the latter also reused by `edit_fact`);
  `anyhow` in the CLI
- **Impact:** Stringly errors cannot be matched programmatically (a TUI can't
  distinguish "unknown category" from "unknown person" without parsing English).
  `AddFactError` is not re-exported from `lib.rs` although functions returning it are.
- **Fix:** Converge on `thiserror` enums with structured variants (e.g. a `FactError`
  shared by add/edit); re-export all public error types.

#### F19. Duplication worth collapsing

- `truncate_text` ×3 (see F1)
- Date-sort comparator ×2 (see F13)
- Four output formats hand-rolled per command in `main.rs` (~400 similar lines) — a
  small `render(headers, rows, format)` helper would collapse most of it
- Dry-run vs. save print blocks in `cmd_add_fact`/`cmd_edit_fact` are verbatim copies

#### F20. Two sources of truth for the schema (already drifting)

- **Where:** `schema.json` vs Rust structs/validators
- **Drift observed:** schema requires 2-digit months (`1987-3` accepted by `date.rs`);
  UUID **v4** pattern (`Uuid::parse_str` accepts any version); `^[a-z][a-z0-9_-]*$`
  ID patterns unenforced in Rust.
- **Fix (decision needed):** Generate the schema from the structs (`schemars`), or add
  the missing checks to `validate_chronicle` so `kinsaga validate` enforces what
  `kinsaga schema` promises.

### 2.5 Smaller items

#### F21. Dead dependencies: `log` + `env_logger`
No `log::` macro anywhere; `env_logger::init()` runs but nothing logs; STATUS.md
documents `RUST_LOG=warn`. Add logging or drop both deps.

#### F22. Timeline colors are fake
`src/main.rs:539-551` builds the category→hex map, then ignores the hex and hardcodes
the five sample category IDs; custom categories render white. Use
`Colorize::truecolor(r, g, b)` from the parsed hex.

#### F23. Markdown output doesn't escape `|` in cell content
Breaks the table. (CSV is fine; classic `=`-formula injection is possible but low
severity for a personal tool.)

#### F24. No person management commands
No `add-person` / `remove-fact`; new chronicles or family members require hand-editing
JSON. Biggest missing CLI feature for daily use.

#### F25. No integration tests, no CI
All 87 tests are unit tests; nothing exercises the binary (would have caught F4/F5).
No `.build.yml` for builds.sr.ht.

#### F26. Cargo.toml hygiene
No `rust-version` (let-chains require 1.88+); `chrono` could use
`default-features = false, features = ["clock"]`; STATUS.md says `ureq = "3.0"` vs
actual 3.1.

#### F27. `lib.rs` doc example uses `"uuid-1"` as a fact ID
Teaches exactly what `validate` flags as invalid.

#### F28. Privacy note for `--gps`
Document that GPS validation sends family location data to the public
nominatim.openstreetmap.org service.

#### F29. `from_json`/`to_json` not re-exported at crate root
Documented in STATUS.md as API highlights; accessible only as `kinsaga::io::from_json`.
Minor inconsistency with `load`/`save`.

#### F30. Person-name conflict during merge produces no event
Categories get a `Skipped` event on conflict; person-name conflicts under `Skip` are
silent. Inconsistent observability.

#### F31. Year-range filter passes facts with unparseable dates
`FactFilter::matches` only applies `from`/`to` when the date parses; invalid dates
always pass year filters. Decide and document the intended semantics.

#### F32. `validate` prints trivially-true checks
"✓ Valid UTF-8 / ✓ Valid JSON structure" are implied by a successful load. Cosmetic.

### 2.6 What's good (keep doing this)

- Module boundaries (model/date/io/filter/validate/merge/facts/format) are the right cut
- Builder-style constructors on the model
- Merge design with events + stats + strategies
- Nominatim rate limiter respects the ToS (1.1 s interval)
- Date-uncertainty design (`?` suffix, precision-aware sort keys) is simple and correct
- Test hygiene: fictional data only, per the privacy note; clippy clean

---

## 3. Severity overview

| Severity | Findings |
|---|---|
| Critical (confirmed) | F1, F2 |
| High | F3, F4, F5, F6, F7, F8 |
| Medium | F9, F10, F11, F12, F13, F14, F15, F16, F17, F18, F20 |
| Low / polish | F19, F21–F32 |

---

## 4. Step-by-step implementation plan

Each step lists scope, files, and acceptance criteria. Steps within a phase are
ordered; phases are sequential releases. Run `cargo test && cargo clippy --all-targets`
after every step.

### Phase 1 — Correctness hotfixes → release v0.2.1 ✅ DONE (2026-07-14)

Small, isolated, high-impact. No API changes.
Implemented: F1 (`5a03a18`), F2 (`681ef08`), F7 (`6d773fa`), F8 (`bd892b6`).
Note on 1.2: implemented with word-boundary matching (`contains_word`) instead of
a minimum-length threshold — strictly stronger; also fixes the same flaw inside
`matches_country_code`/`same_country` (e.g. "Ukraine" vs "uk").

**Step 1.1 — Fix `truncate_text` (F1, part of F19)**
- Rewrite `format::truncate_text` to cut on `char_indices()` boundaries.
- Delete the private copies in `validate.rs` and `merge.rs`; import from `format`.
- Handle `max_len < 3` sanely (result never longer than `max_len`).
- Tests: umlaut at boundary ("Schwändi"), CJK ("日本…"), emoji, `max_len` 0/1/3, ASCII regression.

**Step 1.2 — Fix `fuzzy_match` (F2)**
- Reorder: exact → ISO-code table → `same_country` → containment.
- Containment only when both normalized strings are ≥ 3 chars.
- Tests (must be `false`): `("CH","China")`, `("US","Russia")`, `("IN","Argentina")`.
- Tests (must remain `true`): `("CH","Schweiz")`, `("Paris","Paris, France")`, `("日本","JP")`.

**Step 1.3 — Fix `correct_uuids` (F7)**
- Change `needs_correction` to positional identity (`(person_idx, fact_idx)`), or fix
  only occurrences after the first for duplicates.
- Delete the dead `used_uuids` first pass (UUIDv4 collision risk is negligible) or
  wire it up properly — pick one, don't keep dead code.
- Tests: duplicate pair → first fact keeps its UUID, second gets a new one; two facts
  with empty IDs both fixed; cross-person duplicate fixed only at flagged occurrence.

**Step 1.4 — Fix `ChronicleDate` Ord/Eq (F8)**
- Extend `Ord::cmp` with tiebreakers (`uncertain`, then `raw`) so `Equal ⇔ ==`.
- Test: `"1987"` vs `"1987?"` — `cmp != Equal`, ordering of equal sort-keys stable.

**Step 1.5 — Release**
- Bump to 0.2.1, changelog entry, tag.

### Phase 2 — Data safety & CLI contract → release v0.3.0

User-visible behavior changes (exit codes, output streams) ⇒ minor version bump.

**Step 2.1 — Atomic save (F3)**
- Promote `tempfile` to `[dependencies]`.
- `io::save`: write to `NamedTempFile` in the target's parent dir, `persist()` over
  the target. Preserve a trailing newline.
- Tests: save-over-existing keeps content on simulated failure path; roundtrip intact.

**Step 2.2 — stderr/stdout separation (F5, F32)**
- All progress, checkmarks, warnings, summaries in every command → `eprintln!`.
- Machine output only on stdout: corrected JSON (`--correct`), updated JSON
  (`--apply`), `--format json|csv|md` payloads, `schema`. Remove `--- ... ---` banners.
- Drop the trivially-true "Valid UTF-8 / Valid JSON" lines (F32).
- Acceptance: `kinsaga validate --correct > out.json` and
  `validate --gps --suggest --apply > out.json` produce parseable JSON.

**Step 2.3 — Exit codes (F4)**
- `validate`: exit 1 when `result.errors` non-empty (or corrections were needed but
  not applied — decide and document); add `--strict` for warnings.
- Acceptance: `kinsaga validate` on a chronicle with duplicate UUIDs exits non-zero.

**Step 2.4 — Fix `--correct`/`--gps` interplay (F6)**
- After correction, assign the corrected chronicle back to `chronicle` so subsequent
  GPS apply/save operates on corrected data; single final save.
- Test/acceptance: `--correct --gps --suggest --apply --in-place` leaves corrected
  UUIDs *and* applied coordinates in the file.

**Step 2.5 — Integration test harness (F25, first half)**
- Add `assert_cmd` + `predicates` as dev-deps; create `tests/cli.rs`.
- Cover: validate exit codes, stdout purity under redirect, add-fact roundtrip on a
  temp copy of the sample, search regex error (after Step 3.4).

**Step 2.6 — Update docs & release**
- STATUS.md/README: new exit-code semantics, redirect examples now valid.
- Bump to 0.3.0, tag.

### Phase 3 — Architecture consolidation (library-first restoration)

**Step 3.1 — Extract GPS logic into the library (F17)**
- New library functions (in `geocode.rs` or a `gps` module):
  - `validate_gps(&Chronicle, &mut NominatimClient) -> Vec<Result<GpsValidationResult, GeocodeError>>`
    (actually constructs `GpsValidationResult`; delete the inline reimplementation)
  - `suggest_coordinates(&Chronicle, &mut NominatimClient, limit) -> Vec<Suggestion>`
  - `apply_suggestions(&mut Chronicle, &[Suggestion])`
- Consider a progress callback (`FnMut(Progress)`) so the CLI can print per-item lines
  while iterating.
- `cmd_validate` shrinks to: run library calls, print, save. Target < 120 lines.
- Unit tests for match logic using hand-built `GeocodedPlace` values (no network).

**Step 3.2 — Nominatim client hardening (F11)**
- Constructor accepts optional `base_url` (default the public instance).
- Configure `ureq` agent with connect + global timeouts (e.g. 10 s / 30 s).
- Deserialize the `{"error": ...}` envelope → `GeocodeError::NoResults` (reverse) or a
  new `Nominatim(String)` variant.
- Tests: response-parsing tests against canned JSON (incl. error envelope); optional
  mock-server test via injected base_url.

**Step 3.3 — Unify errors (F18, F29)**
- Replace `AddFactError`/`MergeError` with `thiserror` enums with structured variants
  (`FactError::{PersonNotFound{id, available}, CategoryNotFound{..}, InvalidDate{..}, ...}`,
  `MergeError::CategoryConflict{..}` etc.).
- Re-export all public error types + `io::{from_json, to_json}` from `lib.rs`.
- CLI messages must remain equally helpful (Display impls carry the same detail).

**Step 3.4 — Filter/regex correctness (F12, F13, F31)**
- `FactFilter`: compile regex once (builder returns `Result<FactFilter, regex::Error>`
  or stores `Result<Regex, ...>` surfaced on first use); CLI reports invalid patterns
  as errors.
- One shared date comparator (`date::cmp_date_strings` or similar) used by
  `sort_facts_by_date` and `collect_timeline_facts`; document unparseable-date
  placement (recommend: last).
- Decide + document year-filter semantics for unparseable dates (recommend: excluded
  when a year filter is active); test it.

**Step 3.5 — Merge robustness (F10, F16, F30)**
- Clone-and-set-id instead of field-by-field fact reconstruction.
- Update `target_person_ids`/`target_category_ids` as items are inserted.
- Emit a `Skipped` event for person-name conflicts under `Skip`.
- `validate_chronicle`: add error-level `DuplicatePersonId` / `DuplicateCategoryId`.
- Tests: malformed source with duplicated person id; person-name conflict event.

### Phase 4 — CLI polish & model decisions

**Step 4.1 — `edit-fact` flag semantics (F14)**
- Error on conflicting attachment flags (`--clear-attachments` + `--add-attach`, etc.)
  or define documented compose order; support `--attach-type`/`--attach-title` with
  `--add-attach`.

**Step 4.2 — `add_fact` `with` validation (F15)**
- Reject `with` containing the target person; dedupe the list; tests for `--propagate`.

**Step 4.3 — Decision: attachment URL representation (F9)**
- Option A (recommended): `Attachment.url: String` + URL-format check in
  `validate_chronicle` (warning) — malformed files stay loadable/fixable, no silent
  normalization on save. Breaking for library consumers ⇒ pair with 0.4.0.
- Option B: keep `Url`, add lenient custom deserializer capturing the raw string.
- Either way: test that a chronicle with one bad URL loads and validates.

**Step 4.4 — Decision: schema source of truth (F20)**
- Option A: generate `schema.json` from structs via `schemars` (build-time or a
  `kinsaga schema` that renders it); delete the hand-written file.
- Option B: keep hand-written schema; add the missing checks (ID patterns, UUID v4,
  2-digit date parts) to `validate_chronicle` and a doc-test that cross-checks the
  sample file against both.

**Step 4.5 — Output improvements (F19, F22, F23)**
- `render(headers, rows, format)` helper collapsing csv/md/json table emission in
  `main.rs`; escape `|` (and newlines) in markdown cells.
- Parse category hex colors → `Colorize::truecolor`; remove the hardcoded id→color
  match. Deduplicate the dry-run/save print blocks.

**Step 4.6 — Person management commands (F24)**
- `add-person <id> --name <name>`, `remove-fact <uuid>`, optionally
  `remove-person <id> [--force]` with `with`-reference warnings. Library functions
  first, thin CLI on top. (Feature work — can ship independently.)

### Phase 5 — Process, deps, docs

**Step 5.1 — CI (F25, second half)**
- `.build.yml` for builds.sr.ht: stable toolchain, `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test`.

**Step 5.2 — Cargo.toml hygiene (F21, F26)**
- Add `rust-version = "1.88"`.
- `chrono`: `default-features = false, features = ["clock"]`.
- Drop `log` + `env_logger` (or actually add `log::warn!`/`debug!` calls in library
  paths — pick one; if dropped, remove the RUST_LOG section from STATUS.md).

**Step 5.3 — Doc sync (F26, F27, F28)**
- STATUS.md: ureq version, exit codes, stderr/stdout contract, remove/replace RUST_LOG.
- README: privacy note for `--gps` (data sent to public Nominatim).
- `lib.rs` doc example: use a valid UUID (`Uuid::new_v4()` or a literal v4).

### Suggested sequencing at a glance

| Phase | Content | Release |
|---|---|---|
| 1 | F1, F2, F7, F8 hotfixes | v0.2.1 |
| 2 | F3–F6 data safety + CLI contract, first integration tests | v0.3.0 |
| 3 | F10–F13, F16–F18, F29–F31 architecture consolidation | v0.3.x |
| 4 | F9, F14, F15, F19, F20, F22–F24 polish + model decisions | v0.4.0 |
| 5 | F21, F25–F28 process/deps/docs | rolling |

---

## 5. Verification checklist (run after each phase)

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test

# CLI contract (after Phase 2):
kinsaga -i examples/sample-chronicle.json validate --correct > /tmp/out.json \
  && python3 -m json.tool /tmp/out.json > /dev/null && echo "stdout clean"
kinsaga -i /tmp/dup-uuid.json validate; echo "exit=$?"   # expect non-zero

# Panic regression (after Phase 1):
kinsaga -i /tmp/umlaut-long-text.json validate            # must not panic
```
