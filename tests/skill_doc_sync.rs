//! Doc-sync guard: keeps `claude_skill/SKILL.md` honest against the real CLI.
//!
//! The skill lives in this repo, so it should never drift from the binary it
//! documents (same pattern as mindtask's `skill_doc_sync`). Two checks, both
//! driven off the actual `kinsaga` executable and the shipped skill file:
//!
//!   1. `skill_documents_every_command` — every command and one-level
//!      subcommand that `--help` advertises is mentioned in SKILL.md. Add or
//!      rename a command and this fails until the skill is updated. This is
//!      the real staleness trigger: it catches content drift, which a version
//!      string alone cannot.
//!   2. `skill_version_matches_crate` — the frontmatter `documents-version`
//!      marker matches the crate version, so the human-readable pin can't
//!      silently lie after a release bump.
//!
//! Both this file and `claude_skill/` are excluded from the published
//! package (Cargo.toml `exclude`), so the crates.io test suite is unaffected.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_kinsaga");

fn skill_md() -> String {
    let path = format!("{}/claude_skill/SKILL.md", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

/// Command names listed under the `Commands:` section of
/// `kinsaga <args> --help`, excluding clap's built-in `help`. Returns an
/// empty vec for leaf commands that expose no subcommands.
fn subcommands(args: &[&str]) -> Vec<String> {
    let out = Command::new(BIN)
        .args(args)
        .arg("--help")
        .output()
        .expect("failed to spawn kinsaga");
    let help = String::from_utf8(out.stdout).expect("help output is not UTF-8");

    let mut names = Vec::new();
    let mut in_commands = false;
    for line in help.lines() {
        if line.trim_end() == "Commands:" {
            in_commands = true;
            continue;
        }
        if !in_commands {
            continue;
        }
        // The section ends at the first blank line (before `Options:`, etc.).
        if line.trim().is_empty() {
            break;
        }
        // Command rows are indented exactly two spaces; wrapped description
        // lines (if any) are indented further, so skip those.
        if line.starts_with("  ")
            && !line.starts_with("   ")
            && let Some(name) = line.split_whitespace().next()
            && name != "help"
        {
            names.push(name.to_string());
        }
    }
    names
}

#[test]
fn skill_documents_every_command() {
    let skill = skill_md();
    let mut missing = Vec::new();

    for cmd in subcommands(&[]) {
        // Top-level commands appear as `kinsaga <cmd>` in the reference.
        if !skill.contains(&format!("kinsaga {cmd}")) {
            missing.push(cmd.clone());
        }
        // One level deep, should kinsaga ever grow nested subcommands.
        for sub in subcommands(&[cmd.as_str()]) {
            let phrase = format!("{cmd} {sub}");
            if !skill.contains(&phrase) {
                missing.push(phrase);
            }
        }
    }

    assert!(
        missing.is_empty(),
        "claude_skill/SKILL.md does not document these commands exposed by \
         `kinsaga --help`: {missing:?}\nUpdate the skill's command reference."
    );
}

#[test]
fn skill_version_matches_crate() {
    let skill = skill_md();
    let want = env!("CARGO_PKG_VERSION");

    let documented = skill
        .lines()
        .find_map(|l| l.trim().strip_prefix("documents-version:"))
        .map(|v| v.trim().trim_matches('"').to_string())
        .expect("SKILL.md frontmatter is missing a `documents-version:` field");

    assert_eq!(
        documented, want,
        "SKILL.md frontmatter `documents-version` ({documented}) does not match \
         the crate version ({want}); bump it in claude_skill/SKILL.md on release."
    );
}
