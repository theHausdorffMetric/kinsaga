# Kinsaga — Architecture & Code Quality Review (v0.4.0)

**Date:** 2026-08-06
**Reviewed:** v0.4.0, commit `c5c6ccf` (clean working tree)
**Scope:** Full codebase — all 11 library modules, CLI, tests, schema, CI manifest, docs
**Method:** Complete source read; all gates executed (`cargo fmt --check`,
`clippy --all-targets -D warnings`, 130 unit + 22 CLI + 1 doc test, `cargo audit`
— **all green**); findings Q1–Q4 **verified by execution** against the built
library. This is the follow-up to the 2026-07-14 review (`CODE_REVIEW.md`,
F1–F32): the fixes from that cycle were spot-checked and **all hold** — the
UTF-8 truncation fix, word-boundary fuzzy matching, atomic save, exit-code and
stream contracts each carry regression tests that pass.

---

## 1. Summary

Kinsaga at v0.4.0 is in **good shape**. The prior review cycle visibly worked:
the library/CLI split is real (the CLI contains no business logic), saves are
atomic with fsync and permission preservation, merge semantics are thoughtful,
the `Ord` implementation on dates handles the contract edge the docs call out,
the Nominatim client is rate-limited, timeout-guarded, and tested against an
in-process mock HTTP server, and the stdout/stderr + exit-code contract is
locked by integration tests. Nothing found this round rises to the severity of
the 2026-07 F1/F2 class.

What this round found:

- **1 confirmed minor bug** (Q1: merge dedup misses duplicates *within* the
  source file)
- **3 confirmed robustness gaps** (Q2 country-table coverage, Q3
  calendar-invalid dates, Q4 empty person names)
- **1 architectural tension** worth a deliberate decision (A1: propagated
  facts are unlinked copies that silently diverge on edit)
- Assorted small hygiene items (vestigial re-export, unsorted search output,
  CSV `\r` edge, `--strict` not seeing GPS results, MSRV behind house style)

Findings are numbered `A*` (architecture) and `Q*` (quality) to stay distinct
from the prior review's `F*`. The step-by-step plan is in §5.

---

## 2. Architecture assessment

### 2.1 Shape (good)

```
src/lib.rs         exports; 10 domain modules
├── model.rs       serde structs + builders, no logic
├── date.rs        ChronicleDate parse/order; single ordering truth (cmp_date_strings)
├── io.rs          load / atomic save (temp file + fsync + rename, perms preserved)
├── filter.rs      FactFilter (substring | precompiled regex), search
├── validate.rs    issue collection (error/warning levels), UUID correction
├── merge.rs       strategy-driven merge with event log + stats
├── facts.rs       add/edit/remove fact, timeline collection, option structs
├── persons.rs     add/remove person with reference handling
├── format.rs      display/escaping helpers (CSV, MD, truncation, hex color)
└── geocode.rs     NominatimClient + GPS validate/suggest/apply
src/main.rs        CLI: clap definitions + rendering only
```

Dependency flow is clean and acyclic: `model` at the bottom; `facts`/`merge`/
`validate` compose `model` + `date`; `main.rs` composes everything and adds
nothing but rendering. Error types are per-module `thiserror` enums with
structured variants (a TUI or web layer can match on kinds instead of parsing
strings) — exactly right for the declared "library first, multiple UIs later"
goal (STATUS.md roadmap B2 TUI / B3 WASM).

Other things done well, worth keeping as-is:

- **`Fact.date` stays a raw string**, parsed on demand, with one comparison
  function (`cmp_date_strings`) as the single ordering truth; unparseable
  dates sort last deterministically. This keeps user data round-trip-safe.
- **`Attachment.url` stays a raw string** (post-F9): malformed URLs are
  validation warnings, not load failures; saving never rewrites user data.
- **`save()` is genuinely atomic** (`NamedTempFile` in the target dir +
  `sync_all` + `persist`), keeps existing file permissions, and appends a
  trailing newline. The temp-file-leak and missing-directory cases are tested.
- **CLI contract**: stdout is data-only, everything else on stderr; empty
  result sets still emit valid CSV headers / `[]` JSON. Locked by tests.
- **Merge** keeps its lookup sets updated while merging (malformed sources
  with duplicate person entries can't duplicate targets — tested), clones
  facts wholesale so future `Fact` fields survive, and returns a typed event
  log instead of printing.

### 2.2 A1 — Propagated facts are unlinked copies (needs a decision)

`add_fact --propagate` creates an **independent copy** of the fact per `with`
person, each with its own UUID and reciprocal `with` lists (`facts.rs:192`).
This denormalization is a *documented* design decision (STATUS.md: "Shared
facts | Duplicate per person"), and timeline display dedupes shared facts by
the `(date, category, text)` triple (`facts.rs:264`).

The unhandled consequence: **nothing links the copies after creation.**
`edit-fact` edits one copy only; `remove-fact` removes one copy only. The
moment one copy is edited, the triple-based dedup stops matching and the
same real-world event appears twice in `timeline --include-shared`; there is
no way to even notice this has happened short of manual inspection.

Options, cheapest first:

1. **Detect drift in `validate`** — a warning when two facts on different
   persons reference each other via `with` and share a date but differ in
   text/category (i.e. they look like diverged propagation siblings). No
   schema change; catches the silent case. *Recommended as the v0.4.x step.*
2. `edit-fact --propagate` / `remove-fact --propagate` — best-effort: find the
   sibling copies via the reciprocal `with` + date and apply the same change.
3. A `group` id on `Fact` (schema addition) making the link explicit — the
   real fix, but a schema/version bump; fold it into whatever schema evolution
   B2/B3 forces anyway.

Doing (1) now and deferring (3) to the next schema rev is the proportionate
path.

### 2.3 A2 — Scale posture (fine, note only)

Every operation loads the whole file, clones the chronicle on save
(`io.rs:38`), and searches linearly; `apply_suggestions` is
O(suggestions × facts). For a family chronicle (the sample: 2 persons,
14 facts; realistic: thousands of facts) none of this matters. No action —
just don't add caching/indexing complexity speculatively.

### 2.4 A3 — No schema-version gate on load

`schema.json` pins `"version": {"const": "1.0"}`, but neither `load()` nor
`validate_chronicle()` looks at the field — a future `"2.0"` file (or
`"version": "banana"`) loads silently and gets validated against 1.0 rules.
One warning in `validate_chronicle` (`version != "1.0"` → warning) closes the
gap cheaply and gives future-you a migration hook.

### 2.5 A4 — Hand-rolled country table

`geocode.rs:521` carries a 41-entry ISO-code/name table. The word-boundary
matching on top of it is correct (F2's fix, verified), but the table's
*coverage* is the weak point — see Q2. A data-crate (e.g. `rust_iso3166`)
covers all 249 codes for the code↔English-name axis; the native-language
names ("Schweiz", "日本") that Nominatim actually returns would still need a
curated overlay, so the right shape is: **crate for codes + slim table for
native aliases**, or an explicit doc note that GPS country matching only
understands the listed countries.

### 2.6 A5 — `main.rs` size

1743 lines, of which ~900 are per-command × per-format rendering. It is
*disciplined* (no business logic), just monolithic. When B2 (TUI) starts, pull
the rendering into `src/render.rs` (or a `cli/` module dir) so `main.rs` is
argument definitions + dispatch. Not urgent; do it as the first commit of the
next feature phase, not as churn now.

### 2.7 A6 — Vestigial public re-export

`lib.rs:76`: `pub use url::Url;` — no public API takes or returns `Url` since
F9 made attachment URLs raw strings. The re-export publicly couples the crate
to the `url` major version for zero benefit. Remove on the next minor bump
(technically API surface, so 0.5.0).

---

## 3. Quality findings

### Q1 — Merge dedup misses duplicates within the source *(confirmed bug, minor)*

- **Where:** `merge.rs:190` (`existing_facts` snapshot), `merge.rs:243` loop
- **What:** `existing_facts` is built once from the *target* person and never
  extended while the source person's facts are processed. Two identical facts
  (same date/category/text) inside one source person both pass the
  `DuplicateStrategy::Skip` check and both land in the target.
- **Proof (executed):** source with the same event twice, empty target →
  target ends with **2** facts under `Skip` (expected 1).
- **Impact:** low — needs a malformed/hand-edited source; but `Skip` is the
  default and its documented promise ("skip duplicate facts") is broken for
  this case, and a *second* merge of the same source won't clean it up (both
  copies now match the target set... actually both stay, since the *source*
  still adds nothing new — the duplication is permanent).
- **Fix:** insert `fact_key` into `existing_facts` when a fact is queued for
  adding (one line), so later source facts see earlier ones. Add the
  regression test.

### Q2 — Country matching fails for most of the world *(confirmed gap)*

- **Where:** `geocode.rs:521` (`COUNTRY_CODES`)
- **Proof (executed):** `fuzzy_match("UA", "Ukraine")` → `false`;
  `fuzzy_match("Ukraine", "Україна")` → `false`.
- **Impact:** `validate --gps` reports a **country mismatch** for perfectly
  correct coordinates whenever the country isn't one of the 41 listed (no
  African country except ZA/EG, no Ukraine, no Baltics, no Balkans, no
  Central/South America beyond BR/MX/AR...). The failure direction is noisy
  (false mismatch warnings), not silent, but it erodes trust in `--gps`.
- **Fix:** per A4 — ISO-code axis from a data crate + curated native-name
  overlay; or document the coverage. The exact-match and containment paths
  already handle same-language comparisons, so the table only matters when
  the stored value and Nominatim's answer are in different languages/codes.

### Q3 — Calendar-invalid dates accepted *(confirmed gap)*

- **Where:** `date.rs:192` (`parse_day` checks 1–31 only)
- **Proof (executed):** `ChronicleDate::parse("2023-02-31")` → `Ok`.
- **Impact:** `2023-02-31` and `2023-04-31` are storable, pass `validate`,
  and sort as given. `schema.json`'s regex has the same blind spot, so the
  two are at least *consistent* — but nothing documents the leniency.
- **Fix:** validate day-vs-month(+leap) in `parse_day`'s caller — `jiff` is
  already a dependency and `jiff::civil::Date::new(y, m, d)` does it in one
  call. Decide whether a bad day is a parse **error** (breaking for existing
  files: make it a *validation warning* instead) — recommendation: keep parse
  lenient, add an `ImplausibleDate` warning in `validate_chronicle`.

### Q4 — Empty person name passes everything silently *(confirmed gap)*

- **Proof (executed):** `Person::new("x", "")` + `validate_chronicle` → 0
  warnings; `add-person x --name ""` would save it.
- **Fix:** warning in `validate_chronicle` (`EmptyName`), and either reject or
  warn in `add_person`. (Empty `Fact.text` has the same shape if one wants to
  be thorough.)

### Q5 — `escape_csv` ignores bare `\r`

`format.rs:69` quotes on `,`, `"`, `\n` — a field containing a lone carriage
return is emitted unquoted. Rare (requires `\r` inside fact text), but the fix
is adding one character to the condition.

### Q6 — GPS results are invisible to the exit code

`validate --gps` can print N mismatches and still exit 0; `--strict` only
sees `validate_chronicle` warnings (`main.rs:1266`). Either is defensible —
network-dependent checks failing CI is its own annoyance — but it should be a
*choice*: document that GPS outcomes are informational, or fold mismatch
count into the `--strict` failure path. Recommendation: document (network
checks shouldn't gate scripts by default), and add the count to the bail
message only when `--strict --gps` was explicitly combined.

### Q7 — Library-level `edit_fact` accepts unvalidated attachment URLs

The CLI always routes new attachments through `build_attachments` (which
`Url::parse`-checks), but a library consumer can pass
`AttachmentUpdate::Add(vec![Attachment::new("not a url")])` and `edit_fact`
only checks MIME (`facts.rs:557`). Same posture as `add_fact` (attachments
pre-built by the caller), so it's *consistent* — but since `Attachment::new`'s
docs delegate checking to "`build_attachments` or `validate_chronicle`", one
sentence in `edit_fact`/`add_fact` docs stating the caller owns URL validity
would close the ambiguity. (Validating inside `edit_fact` would double-check
the CLI path; harmless, also fine.)

### Q8 — MSRV behind house style

`rust-version = "1.88"` (edition 2024) vs current stable 1.97. House
convention for this machine's projects is: target latest stable and pin
`rust-version` to it, using new features freely. For a *published* crate a
conservative MSRV is a defensible product choice — but nothing in the tree
needs 1.88-compat specifically, and the sibling projects all track latest.
Bump to 1.97 unless there's a known consumer pinned lower (MSRV bump =
semver-minor: fold into 0.5.0 with A6).

### Q9 — `remove_person` counts and strips from different scopes

The refusal count excludes the removed person's own facts
(`persons.rs:65`: `filter(|p| p.id != id)`), the strip loop includes them
(`persons.rs:84`: all persons). With well-formed data (self-references
rejected since F15) the two agree; on a hand-edited file with a
self-reference, `references_stripped` can exceed the count the refusal quoted.
Harmless (the person is deleted either way) — align the scopes or leave a
one-line comment stating the asymmetry is intentional.

### Q10 — `search` output is unsorted

`filter.rs:152` returns person-order × document-order; `timeline` sorts
chronologically, `search` doesn't. Grouping by person is arguably the right
reading order for text output, but for `--format csv/json` consumers a
deterministic documented order matters. Either sort by
`(person, cmp_date_strings)` or document the current order. One line either
way.

### Test coverage notes

The suite is strong where it counts (mock-server HTTP tests, `Ord` contract,
UTF-8 truncation regression, stream/exit-code contract). Gaps worth filling:

- **No CLI test for `timeline`** in `tests/cli.rs` (all other query commands
  have one) — the year-grouping and shared-fact rendering paths run untested
  at the binary level.
- No test pinning Q1's dedup behavior (add with the fix).
- `validate --gps` paths are untested end-to-end (understandable: network;
  the library layer is covered by the mock server — acceptable as-is).

### Dependency review

All 13 runtime deps are justified and current; `cargo audit` is clean
(149 crates). `tempfile` as a *runtime* dep is correct (atomic save).
`jiff` is used only for the save timestamp today — Q3 would give it a second
job. `dotenvy` (maintained fork) over `dotenv`: right choice. One removal
candidate: `urlencoding` (single call site, `geocode.rs:169`) could be
replaced by `url::form_urlencoded` already in-tree — marginal, only worth it
opportunistically.

---

## 4. Severity overview

| # | Severity | One-liner |
|---|----------|-----------|
| Q1 | Bug (minor), confirmed | merge `Skip` misses source-internal duplicate facts |
| Q2 | Gap, confirmed | country table covers 41 countries; rest → false GPS mismatches |
| A1 | Architecture decision | propagated facts silently diverge on edit/remove |
| Q3 | Gap, confirmed | `2023-02-31` parses and validates clean |
| Q4 | Gap, confirmed | empty person name accepted everywhere |
| A3 | Gap | unknown schema versions load without warning |
| Q6 | Decision | GPS mismatches never affect exit code |
| Q5, Q7, Q9, Q10 | Polish | CSV `\r`, doc contract, scope asymmetry, search order |
| A5, A6, Q8 | Hygiene | main.rs split (later), drop `Url` re-export, MSRV 1.97 |

---

## 5. Step-by-step implementation plan

Ordered so every step leaves the tree green (`cargo fmt && cargo clippy
--all-targets -- -D warnings && cargo test`); commit per step. Steps 1–3 are
a patch release (0.4.1); step 4 is the 0.5.0 minor; step 5 is optional
follow-through. Estimated total: a focused day.

### Step 1 — Correctness quick wins (Q1, Q5, Q9)

1. `merge.rs`: make `existing_facts` mutable; on the add path insert
   `fact_key` before pushing to `facts_to_add`. Regression test:
   source person with the same `(date, category, text)` twice, `Skip` →
   exactly one lands; `Add` → both land.
2. `format.rs::escape_csv`: add `|| s.contains('\r')` to the quote condition;
   extend the escape test.
3. `persons.rs`: align the strip loop to the same scope as the count (skip
   the removed person's own facts — they're deleted anyway) + comment.

### Step 2 — Validation gaps (Q3, Q4, A3)

1. `validate.rs`: new `IssueType` variants `ImplausibleDate`, `EmptyName`,
   `UnknownVersion` (all warnings).
   - `ImplausibleDate`: on facts whose date parses but where
     `jiff::civil::Date::new(y, m, d)` errors (only for complete dates).
     Parse stays lenient — existing files keep loading.
   - `EmptyName`: `person.name.trim().is_empty()`; same check for
     `Category.label` while there.
   - `UnknownVersion`: `chronicle.version != "1.0"`.
2. `persons.rs::add_person`: reject empty/whitespace `name` with a new
   `PersonError::EmptyName` (CLI: `add-person x --name ""` should fail, not
   warn — nothing depends on creating nameless persons).
3. Tests for each; update the STATUS.md validation-checks table.

### Step 3 — Shared-fact drift detection (A1 option 1)

1. `validate.rs`: for every fact pair `(a on P, b on Q)` where
   `a.with` ∋ Q and `b.with` ∋ P and `a.date == b.date` but
   `(a.category, a.text) != (b.category, b.text)` → warning
   `DivergedSharedFact` naming both UUIDs. (Index by `with`-pairs first;
   don't go O(n²) over all facts.)
2. Unit test: propagate via `add_fact`, edit one copy via `edit_fact`,
   `validate_chronicle` flags exactly the pair; untouched propagations stay
   clean.
3. Document in STATUS.md under Design Decisions that copies + drift-detection
   is the current model, and that a `group` id is the eventual schema-level
   fix (A1 option 3) when the next schema bump happens.

### Step 4 — 0.5.0: API hygiene + GPS coverage (A6, Q8, Q2, Q6, Q10)

1. Remove `pub use url::Url` from `lib.rs`; bump `rust-version` to `1.97`
   and version to `0.5.0`.
2. Q2: add `rust_iso3166` (or equivalent, license-check first) for the
   code↔name axis; keep the existing table as the native-alias overlay only;
   keep every existing test green and add UA/HR/EE-style cases that fail
   today.
3. Q6: document GPS-informational semantics in STATUS.md + `--help` text for
   `--gps`; when `--strict` and `--gps` are both given, include mismatch
   count in the failure path.
4. Q10: sort `search()` results with `cmp_date_strings` within person groups
   (or document; pick one).
5. Q7: one doc sentence on `add_fact`/`edit_fact` attachment URL ownership.
6. Add the missing `timeline` CLI integration test (text + one machine
   format).

### Step 5 — Optional / next-phase

- A5: extract `src/render.rs` as the opening commit of the B2 (TUI) work.
- `.build.yml`: add a `cargo audit` task (advisory DB fetch needs network on
  the builder; keep it a soft-fail task if that's flaky).
- Drop `urlencoding` for `url::form_urlencoded` opportunistically.
- Consider `Fact.group` (A1 option 3) together with whatever schema changes
  B2/B3 need, behind a `"version": "1.1"` gate — A3's warning is the
  migration hook.

---

## 6. Verification checklist (after each step)

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                      # 130 unit + 22 CLI + 1 doc, plus new tests
cargo audit
cargo run -- -i examples/sample-chronicle.json validate --strict
cargo run -- -i examples/sample-chronicle.json timeline alice --include-shared
cargo run -- -i examples/sample-chronicle.json search Tokyo -f json | jq .
```

Manual spot-check for step 3: propagate a fact between two sample persons,
edit one copy, confirm `validate` flags it and `timeline --include-shared`
shows the duplication the warning is about.
