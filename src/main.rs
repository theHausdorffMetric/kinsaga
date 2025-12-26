//! Kinsaga CLI - Family chronicle management tool.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;
use kinsaga::filter::{filter_facts, FactFilter};
use kinsaga::{load, save, search, Chronicle, ChronicleDate, Fact};
use log::{error, warn};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use uuid::Uuid;

/// Output format for commands
#[derive(Clone, Default, ValueEnum)]
enum OutputFormat {
    /// Plain text output (default)
    #[default]
    Text,
    /// Comma-separated values
    Csv,
    /// Markdown table
    Md,
}

#[derive(Parser)]
#[command(name = "kinsaga")]
#[command(about = "A family chronicle management tool", long_about = None)]
#[command(version)]
struct Cli {
    /// Path to the chronicle JSON file (also reads from KINSAGA_INPUT env var or .env)
    #[arg(short, long, global = true, env = "KINSAGA_INPUT")]
    input: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all persons in the chronicle
    List {
        /// Output format
        #[arg(short, long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },

    /// Show timeline for a person
    Timeline {
        /// Person ID to show
        person: String,

        /// Output format
        #[arg(short, long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,

        /// Filter by category
        #[arg(short, long)]
        category: Option<String>,

        /// Filter from year (inclusive)
        #[arg(long)]
        from: Option<u16>,

        /// Filter to year (inclusive)
        #[arg(long)]
        to: Option<u16>,

        /// Include facts from other persons where this person is in their 'with' field
        #[arg(long)]
        include_shared: bool,
    },

    /// Search for text across all persons
    Search {
        /// Text to search for
        query: String,

        /// Output format
        #[arg(short, long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },

    /// Validate a chronicle JSON file
    Validate {
        /// Generate valid UUIDs for empty/invalid ones and output corrected JSON to stdout
        #[arg(long)]
        correct: bool,
    },

    /// Add a new fact to a person's timeline
    AddFact {
        /// Person ID to add fact to
        person: String,

        /// Date (ISO 8601: YYYY, YYYY-MM, or YYYY-MM-DD, optional ? suffix for uncertainty)
        #[arg(short, long)]
        date: String,

        /// Category ID
        #[arg(short, long)]
        category: String,

        /// Event description
        #[arg(short, long)]
        text: String,

        /// Other person IDs involved (comma-separated)
        #[arg(short, long, value_delimiter = ',')]
        with: Option<Vec<String>>,

        /// Preview only, don't save to file
        #[arg(long)]
        dry_run: bool,

        /// Also create the fact for each person in --with (with cross-references)
        #[arg(long)]
        propagate: bool,
    },
}

fn main() -> Result<()> {
    // Initialize logging (set RUST_LOG=warn or RUST_LOG=info to see logs)
    env_logger::init();

    // Load .env file if present (before parsing CLI so env vars are available)
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    // Get file path from CLI arg, env var, or .env (clap handles the precedence)
    let input = cli.input.context(
        "No chronicle file specified. Use --input/-i, set KINSAGA_INPUT env var, or add KINSAGA_INPUT to .env",
    )?;

    match cli.command {
        Commands::List { format } => cmd_list(&input, &format),
        Commands::Timeline {
            person,
            format,
            category,
            from,
            to,
            include_shared,
        } => cmd_timeline(&input, &person, &format, category, from, to, include_shared),
        Commands::Search { query, format } => cmd_search(&input, &query, &format),
        Commands::Validate { correct } => cmd_validate(&input, correct),
        Commands::AddFact {
            person,
            date,
            category,
            text,
            with,
            dry_run,
            propagate,
        } => cmd_add_fact(&input, &person, &date, &category, &text, with, dry_run, propagate),
    }
}

fn cmd_list(file: &PathBuf, format: &OutputFormat) -> Result<()> {
    let chronicle = load(file).context("Failed to load chronicle")?;

    match format {
        OutputFormat::Text => {
            if let Some(title) = &chronicle.title {
                println!("{}", title.bold());
                println!();
            }
            for person in &chronicle.persons {
                let fact_count = person.facts.len();
                println!(
                    "{:<12} {:<20} ({} {})",
                    person.id,
                    person.name,
                    fact_count,
                    if fact_count == 1 { "fact" } else { "facts" }
                );
            }
        }
        OutputFormat::Csv => {
            println!("id,name,facts");
            for person in &chronicle.persons {
                println!(
                    "{},{},{}",
                    escape_csv(&person.id),
                    escape_csv(&person.name),
                    person.facts.len()
                );
            }
        }
        OutputFormat::Md => {
            println!("| ID | Name | Facts |");
            println!("|---|---|---:|");
            for person in &chronicle.persons {
                println!(
                    "| {} | {} | {} |",
                    person.id, person.name, person.facts.len()
                );
            }
        }
    }

    Ok(())
}

/// Escape a string for CSV output (wrap in quotes if contains comma, quote, or newline)
fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// A fact with its source information (own or shared from another person)
struct FactWithSource<'a> {
    fact: &'a Fact,
    /// None if own fact, Some(person_name) if shared from another person
    shared_from: Option<&'a str>,
}

fn cmd_timeline(
    file: &PathBuf,
    person_id: &str,
    format: &OutputFormat,
    category: Option<String>,
    from: Option<u16>,
    to: Option<u16>,
    include_shared: bool,
) -> Result<()> {
    let chronicle = load(file).context("Failed to load chronicle")?;

    let person = chronicle
        .find_person(person_id)
        .context(format!("Person '{}' not found", person_id))?;

    // Build category color map
    let category_colors: HashMap<&str, &str> = chronicle
        .categories
        .iter()
        .filter_map(|c| c.color.as_ref().map(|color| (c.id.as_str(), color.as_str())))
        .collect();

    // Build filter
    let mut filter = FactFilter::new();
    if let Some(cat) = category {
        filter = filter.with_category(cat);
    }
    if let Some(year) = from {
        filter = filter.from_year(year);
    }
    if let Some(year) = to {
        filter = filter.to_year(year);
    }

    // Collect own facts
    let own_facts: Vec<FactWithSource> = filter_facts(person, &filter)
        .into_iter()
        .map(|fact| FactWithSource {
            fact,
            shared_from: None,
        })
        .collect();

    // Build set of (date, category, text) for owned facts (for deduplication)
    let owned_keys: HashSet<(&str, &str, &str)> = own_facts
        .iter()
        .map(|f| (f.fact.date.as_str(), f.fact.category.as_str(), f.fact.text.as_str()))
        .collect();

    // Collect shared facts (from other persons where this person is in 'with')
    let mut all_facts = own_facts;

    if include_shared {
        for other_person in &chronicle.persons {
            if other_person.id == person_id {
                continue;
            }
            for fact in &other_person.facts {
                // Check if this person is in the 'with' field
                let is_shared = fact
                    .with
                    .as_ref()
                    .is_some_and(|w| w.iter().any(|id| id == person_id));

                if is_shared {
                    // Skip if this fact duplicates an owned fact (same date, category, text)
                    let key = (fact.date.as_str(), fact.category.as_str(), fact.text.as_str());
                    if owned_keys.contains(&key) {
                        continue;
                    }

                    // Apply same filters
                    let shared_date = ChronicleDate::parse(&fact.date).ok();
                    let shared_year = shared_date.as_ref().and_then(|d| d.year);

                    // Check category filter
                    if let Some(ref cat) = filter.category {
                        if &fact.category != cat {
                            continue;
                        }
                    }

                    // Check year filters
                    if let Some(from_year) = filter.from_year {
                        if shared_year.is_none() || shared_year.unwrap() < from_year {
                            continue;
                        }
                    }
                    if let Some(to_year) = filter.to_year {
                        if shared_year.is_none() || shared_year.unwrap() > to_year {
                            continue;
                        }
                    }

                    // Check text filter
                    if let Some(ref text) = filter.text {
                        if !fact.text.to_lowercase().contains(&text.to_lowercase()) {
                            continue;
                        }
                    }

                    all_facts.push(FactWithSource {
                        fact,
                        shared_from: Some(&other_person.name),
                    });
                }
            }
        }
    }

    // Sort all facts by date
    all_facts.sort_by(|a, b| {
        let date_a = ChronicleDate::parse(&a.fact.date).ok();
        let date_b = ChronicleDate::parse(&b.fact.date).ok();
        date_a.cmp(&date_b)
    });

    match format {
        OutputFormat::Text => {
            // Print header
            println!("{} - Timeline", person.name.bold());
            println!("{}", "=".repeat(person.name.len() + 11));
            if include_shared {
                println!("{}", "(including shared facts)".dimmed());
            }
            println!();

            // Group by year
            let mut current_year: Option<u16> = None;

            for fact_with_source in &all_facts {
                let fact = fact_with_source.fact;
                let date = ChronicleDate::parse(&fact.date).ok();
                let year = date.as_ref().and_then(|d| d.year);

                // Print year header if changed
                if year != current_year {
                    if current_year.is_some() {
                        println!();
                    }
                    if let Some(y) = year {
                        println!("{}", y.to_string().bold());
                    } else {
                        println!("{}", "Unknown".bold());
                    }
                    current_year = year;
                }

                // Format date display
                let date_display = format_date_display(&fact.date);

                // Format category
                let cat_label = chronicle
                    .find_category(&fact.category)
                    .map(|c| c.label.as_str())
                    .unwrap_or(&fact.category);

                let cat_display = format!("[{}]", cat_label);
                let cat_colored = if let Some(_color) = category_colors.get(fact.category.as_str())
                {
                    // Could parse hex color, but for simplicity use predefined colors
                    match fact.category.as_str() {
                        "education" => cat_display.blue(),
                        "family" => cat_display.red(),
                        "travel" => cat_display.green(),
                        "hobby" => cat_display.yellow(),
                        "circumstance" => cat_display.purple(),
                        _ => cat_display.white(),
                    }
                } else {
                    cat_display.white()
                };

                // Format shared indicator
                let shared_indicator = match fact_with_source.shared_from {
                    Some(name) => format!(" {}", format!("(via {})", name).dimmed()),
                    None => String::new(),
                };

                println!(
                    "  {} {:<14} {:<20} {}{}",
                    if fact_with_source.shared_from.is_some() {
                        "○".dimmed()
                    } else {
                        "●".cyan()
                    },
                    cat_colored,
                    date_display,
                    fact.text,
                    shared_indicator
                );
            }
            println!();
        }
        OutputFormat::Csv => {
            println!("date,category,text,with,shared_from");
            for fact_with_source in &all_facts {
                let fact = fact_with_source.fact;
                let cat_label = chronicle
                    .find_category(&fact.category)
                    .map(|c| c.label.as_str())
                    .unwrap_or(&fact.category);
                let with_str = fact
                    .with
                    .as_ref()
                    .map(|w| w.join(";"))
                    .unwrap_or_default();
                let shared_from = fact_with_source.shared_from.unwrap_or("");
                println!(
                    "{},{},{},{},{}",
                    escape_csv(&fact.date),
                    escape_csv(cat_label),
                    escape_csv(&fact.text),
                    escape_csv(&with_str),
                    escape_csv(shared_from)
                );
            }
        }
        OutputFormat::Md => {
            println!("## {} - Timeline\n", person.name);
            println!("| Date | Category | Event | With | Shared From |");
            println!("|---|---|---|---|---|");
            for fact_with_source in &all_facts {
                let fact = fact_with_source.fact;
                let cat_label = chronicle
                    .find_category(&fact.category)
                    .map(|c| c.label.as_str())
                    .unwrap_or(&fact.category);
                let with_str = fact
                    .with
                    .as_ref()
                    .map(|w| w.join(", "))
                    .unwrap_or_default();
                let shared_from = fact_with_source.shared_from.unwrap_or("");
                println!(
                    "| {} | {} | {} | {} | {} |",
                    fact.date, cat_label, fact.text, with_str, shared_from
                );
            }
        }
    }

    Ok(())
}

fn cmd_search(file: &PathBuf, query: &str, format: &OutputFormat) -> Result<()> {
    let chronicle = load(file).context("Failed to load chronicle")?;

    let filter = FactFilter::new().with_text(query);
    let results = search(&chronicle, &filter);

    if results.is_empty() {
        println!("No results found for '{}'", query);
        return Ok(());
    }

    match format {
        OutputFormat::Text => {
            println!(
                "Found {} {} for '{}':",
                results.len(),
                if results.len() == 1 { "result" } else { "results" },
                query
            );
            println!();

            for result in results {
                let date_display = format_date_display(&result.fact.date);
                let cat_label = chronicle
                    .find_category(&result.fact.category)
                    .map(|c| c.label.as_str())
                    .unwrap_or(&result.fact.category);

                println!(
                    "{}: {:<12} [{}] {}",
                    result.person.name.bold(),
                    date_display,
                    cat_label,
                    result.fact.text
                );
            }
        }
        OutputFormat::Csv => {
            println!("person_id,person_name,date,category,text");
            for result in results {
                let cat_label = chronicle
                    .find_category(&result.fact.category)
                    .map(|c| c.label.as_str())
                    .unwrap_or(&result.fact.category);
                println!(
                    "{},{},{},{},{}",
                    escape_csv(&result.person.id),
                    escape_csv(&result.person.name),
                    escape_csv(&result.fact.date),
                    escape_csv(cat_label),
                    escape_csv(&result.fact.text)
                );
            }
        }
        OutputFormat::Md => {
            println!("## Search results for '{}'\n", query);
            println!("| Person | Date | Category | Event |");
            println!("|---|---|---|---|");
            for result in results {
                let cat_label = chronicle
                    .find_category(&result.fact.category)
                    .map(|c| c.label.as_str())
                    .unwrap_or(&result.fact.category);
                println!(
                    "| {} | {} | {} | {} |",
                    result.person.name, result.fact.date, cat_label, result.fact.text
                );
            }
        }
    }

    Ok(())
}

fn cmd_validate(file: &PathBuf, correct: bool) -> Result<()> {
    let chronicle = load(file).context("Failed to load chronicle")?;

    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    let mut seen_uuids: HashSet<String> = HashSet::new();
    let mut needs_correction: Vec<(String, String)> = Vec::new(); // (person_id, fact_id)

    // Count facts
    let total_facts: usize = chronicle.persons.iter().map(|p| p.facts.len()).sum();

    // Check for missing category references
    let category_ids: Vec<&str> = chronicle.categories.iter().map(|c| c.id.as_str()).collect();

    for person in &chronicle.persons {
        for fact in &person.facts {
            // UUID validation
            if fact.id.is_empty() {
                let msg = format!("Person '{}': empty UUID for fact '{}'", person.id, fact.text);
                warn!("{}", msg);
                warnings.push(msg);
                needs_correction.push((person.id.clone(), fact.id.clone()));
            } else if Uuid::parse_str(&fact.id).is_err() {
                let msg = format!(
                    "Person '{}': invalid UUID format '{}' for fact '{}'",
                    person.id, fact.id, fact.text
                );
                warn!("{}", msg);
                warnings.push(msg);
                needs_correction.push((person.id.clone(), fact.id.clone()));
            } else if seen_uuids.contains(&fact.id) {
                let msg = format!(
                    "Person '{}': duplicate UUID '{}' for fact '{}'",
                    person.id, fact.id, fact.text
                );
                error!("{}", msg);
                errors.push(msg);
                needs_correction.push((person.id.clone(), fact.id.clone()));
            } else {
                seen_uuids.insert(fact.id.clone());
            }

            // Category validation
            if !category_ids.contains(&fact.category.as_str()) {
                let msg = format!(
                    "Person '{}', fact '{}': unknown category '{}'",
                    person.id, fact.id, fact.category
                );
                warn!("{}", msg);
                warnings.push(msg);
            }

            // Date validation
            if let Err(e) = ChronicleDate::parse(&fact.date) {
                let msg = format!(
                    "Person '{}', fact '{}': invalid date '{}' - {}",
                    person.id, fact.id, fact.date, e
                );
                warn!("{}", msg);
                warnings.push(msg);
            }

            // 'with' references validation
            if let Some(ref with) = fact.with {
                for person_ref in with {
                    if chronicle.find_person(person_ref).is_none() {
                        let msg = format!(
                            "Person '{}', fact '{}': unknown person reference '{}'",
                            person.id, fact.id, person_ref
                        );
                        warn!("{}", msg);
                        warnings.push(msg);
                    }
                }
            }
        }
    }

    // Print results
    println!("{}", "✓ Valid UTF-8".green());
    println!("{}", "✓ Valid JSON structure".green());
    println!(
        "{} {} persons, {} facts",
        "✓".green(),
        chronicle.persons.len(),
        total_facts
    );

    if errors.is_empty() && warnings.is_empty() {
        println!("{}", "✓ All references and UUIDs valid".green());
    } else {
        if !errors.is_empty() {
            println!();
            println!("{}", "Errors:".red());
            for err in &errors {
                println!("  {} {}", "✗".red(), err);
            }
        }
        if !warnings.is_empty() {
            println!();
            println!("{}", "Warnings:".yellow());
            for warning in &warnings {
                println!("  {} {}", "!".yellow(), warning);
            }
        }
    }

    // Handle --correct flag
    if correct && !needs_correction.is_empty() {
        let corrected = correct_uuids(chronicle, &needs_correction);
        let json = serde_json::to_string_pretty(&corrected)
            .context("Failed to serialize corrected chronicle")?;
        println!();
        println!("{}", "--- Corrected JSON ---".cyan());
        println!("{}", json);
    } else if correct && needs_correction.is_empty() {
        println!();
        println!("{}", "No UUID corrections needed.".green());
    }

    Ok(())
}

/// Generate new UUIDs for facts with empty, invalid, or duplicate UUIDs.
fn correct_uuids(mut chronicle: Chronicle, corrections: &[(String, String)]) -> Chronicle {
    let mut used_uuids: HashSet<String> = HashSet::new();

    // First pass: collect all valid, unique UUIDs
    for person in &chronicle.persons {
        for fact in &person.facts {
            let needs_fix = corrections
                .iter()
                .any(|(pid, fid)| pid == &person.id && fid == &fact.id);
            if !needs_fix && Uuid::parse_str(&fact.id).is_ok() {
                used_uuids.insert(fact.id.clone());
            }
        }
    }

    // Second pass: fix invalid UUIDs
    for person in &mut chronicle.persons {
        for fact in &mut person.facts {
            let needs_fix = corrections
                .iter()
                .any(|(pid, fid)| pid == &person.id && fid == &fact.id);
            if needs_fix {
                let new_uuid = Uuid::new_v4().to_string();
                used_uuids.insert(new_uuid.clone());
                fact.id = new_uuid;
            }
        }
    }

    chronicle
}

fn cmd_add_fact(
    file: &PathBuf,
    person_id: &str,
    date: &str,
    category_id: &str,
    text: &str,
    with: Option<Vec<String>>,
    dry_run: bool,
    propagate: bool,
) -> Result<()> {
    let mut chronicle = load(file).context("Failed to load chronicle")?;

    // Validate person exists
    if chronicle.find_person(person_id).is_none() {
        anyhow::bail!(
            "Person '{}' not found. Available persons: {}",
            person_id,
            chronicle
                .persons
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // Validate date format
    ChronicleDate::parse(date).context(format!("Invalid date format '{}'", date))?;

    // Validate category exists
    let category = chronicle.find_category(category_id).context(format!(
        "Category '{}' not found. Available categories: {}",
        category_id,
        chronicle
            .categories
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ))?;
    let category_label = category.label.clone();

    // Validate 'with' references
    if let Some(ref with_ids) = with {
        for with_id in with_ids {
            if chronicle.find_person(with_id).is_none() {
                anyhow::bail!(
                    "Person '{}' in --with not found. Available persons: {}",
                    with_id,
                    chronicle
                        .persons
                        .iter()
                        .map(|p| p.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
    }

    // Warn if --propagate is used without --with
    if propagate && with.is_none() {
        println!(
            "{}",
            "Warning: --propagate has no effect without --with".yellow()
        );
    }

    // Collect all facts to add (for display purposes)
    let mut added_facts: Vec<(String, String, String)> = Vec::new(); // (person_name, person_id, fact_id)

    // Generate UUID and create fact for the main person
    let fact_id = Uuid::new_v4().to_string();
    let mut fact = Fact::new(&fact_id, date, category_id, text);
    if let Some(ref with_ids) = with {
        fact = fact.with_persons(with_ids.clone());
    }

    // Get person name for output
    let person_name = chronicle.find_person(person_id).unwrap().name.clone();
    added_facts.push((person_name.clone(), person_id.to_string(), fact_id.clone()));

    // Add fact to main person
    let person = chronicle
        .find_person_mut(person_id)
        .expect("Person already validated");
    person.facts.push(fact);

    // If propagate is enabled and there are 'with' persons, create facts for them too
    if propagate {
        if let Some(ref with_ids) = with {
            for target_id in with_ids {
                // Build the 'with' list for this person: original person + other with persons
                let mut target_with: Vec<String> = vec![person_id.to_string()];
                for other_id in with_ids {
                    if other_id != target_id {
                        target_with.push(other_id.clone());
                    }
                }

                // Create fact for this person
                let target_fact_id = Uuid::new_v4().to_string();
                let target_fact =
                    Fact::new(&target_fact_id, date, category_id, text).with_persons(target_with);

                let target_name = chronicle.find_person(target_id).unwrap().name.clone();
                added_facts.push((target_name, target_id.clone(), target_fact_id));

                // Add fact to target person
                let target_person = chronicle
                    .find_person_mut(target_id)
                    .expect("Person already validated");
                target_person.facts.push(target_fact);
            }
        }
    }

    if dry_run {
        println!("{}", "Dry run - not saving changes".yellow());
        println!();
        println!("Would add {} fact(s):", added_facts.len());
        for (name, _id, uuid) in &added_facts {
            println!();
            println!("  {}:", name.bold());
            println!(
                "    {} {} [{}] {}",
                "●".cyan(),
                date,
                category_label,
                text
            );
            println!("    UUID: {}", uuid);
        }
    } else {
        save(file, &chronicle).context("Failed to save chronicle")?;
        println!(
            "{}",
            format!("{} fact(s) added successfully", added_facts.len()).green()
        );
        for (name, _id, uuid) in &added_facts {
            println!();
            println!("  {}:", name.bold());
            println!(
                "    {} {} [{}] {}",
                "●".cyan(),
                date,
                category_label,
                text
            );
            println!("    UUID: {}", uuid);
        }
    }

    Ok(())
}

fn format_date_display(date_str: &str) -> String {
    // Just return the date as-is for now, padded
    date_str.to_string()
}
