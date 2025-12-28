//! Kinsaga CLI - Family chronicle management tool.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;
use kinsaga::filter::{filter_facts, FactFilter};
use kinsaga::{load, save, search, Attachment, Chronicle, ChronicleDate, Coordinates, Fact, Location, Url};
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

        /// Write corrected JSON back to the input file (requires --correct)
        #[arg(long, requires = "correct")]
        in_place: bool,
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

        /// Country where the event occurred (required if specifying location)
        #[arg(long)]
        country: Option<String>,

        /// City where the event occurred (optional, requires --country)
        #[arg(long, requires = "country")]
        city: Option<String>,

        /// GPS latitude (optional, requires --country and --lon)
        #[arg(long, requires_all = ["country", "lon"], allow_hyphen_values = true)]
        lat: Option<f64>,

        /// GPS longitude (optional, requires --country and --lat)
        #[arg(long, requires_all = ["country", "lat"], allow_hyphen_values = true)]
        lon: Option<f64>,

        /// Attachment URL (can be specified multiple times)
        #[arg(long = "attach", value_name = "URL")]
        attachments: Option<Vec<String>>,

        /// MIME content type for attachments (applies to all --attach URLs)
        #[arg(long = "attach-type", value_name = "MIME")]
        attach_type: Option<String>,

        /// Title/description for attachments (applies to all --attach URLs)
        #[arg(long = "attach-title", value_name = "TITLE")]
        attach_title: Option<String>,

        /// Preview only, don't save to file
        #[arg(long)]
        dry_run: bool,

        /// Also create the fact for each person in --with (with cross-references)
        #[arg(long)]
        propagate: bool,
    },

    /// Merge another chronicle JSON file into the main chronicle
    Merge {
        /// Path to the source chronicle JSON file to merge from
        source: PathBuf,

        /// Preview merge without saving
        #[arg(long)]
        dry_run: bool,

        /// How to handle category/person conflicts
        #[arg(long, value_enum, default_value_t = ConflictStrategy::Skip)]
        on_conflict: ConflictStrategy,

        /// How to handle duplicate facts (same date, category, text)
        #[arg(long, value_enum, default_value_t = DuplicateStrategy::Skip)]
        duplicates: DuplicateStrategy,

        /// Regenerate all UUIDs from source file (avoids collisions)
        #[arg(long)]
        regenerate_uuids: bool,
    },
}

/// Strategy for handling category/person conflicts during merge
#[derive(Clone, Default, ValueEnum)]
enum ConflictStrategy {
    /// Skip conflicting items (keep target values)
    #[default]
    Skip,
    /// Overwrite target with source values
    Overwrite,
    /// Fail on any conflict
    Fail,
}

/// Strategy for handling duplicate facts during merge
#[derive(Clone, Default, ValueEnum)]
enum DuplicateStrategy {
    /// Skip duplicate facts
    #[default]
    Skip,
    /// Add duplicates anyway (with new UUIDs)
    Add,
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
        Commands::Validate { correct, in_place } => cmd_validate(&input, correct, in_place),
        Commands::AddFact {
            person,
            date,
            category,
            text,
            with,
            country,
            city,
            lat,
            lon,
            attachments,
            attach_type,
            attach_title,
            dry_run,
            propagate,
        } => cmd_add_fact(
            &input,
            &person,
            &date,
            &category,
            &text,
            with,
            country,
            city,
            lat,
            lon,
            attachments,
            attach_type,
            attach_title,
            dry_run,
            propagate,
        ),
        Commands::Merge {
            source,
            dry_run,
            on_conflict,
            duplicates,
            regenerate_uuids,
        } => cmd_merge(&input, &source, dry_run, on_conflict, duplicates, regenerate_uuids),
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

                // Show location if present
                if let Some(ref location) = fact.location {
                    println!("    {} {}", "📍".dimmed(), format_location(location).dimmed());
                }

                // Show attachments if present
                for attachment in &fact.attachments {
                    let att_display = if let Some(ref title) = attachment.title {
                        format!("{} ({})", title, attachment.url)
                    } else {
                        attachment.url.to_string()
                    };
                    println!("    {} {}", "📎".dimmed(), att_display.dimmed());
                }
            }
            println!();
        }
        OutputFormat::Csv => {
            println!("date,category,text,with,shared_from,location,attachments");
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
                let location_str = fact
                    .location
                    .as_ref()
                    .map(|l| format_location(l))
                    .unwrap_or_default();
                let attachments_str: String = fact
                    .attachments
                    .iter()
                    .map(|a| a.url.to_string())
                    .collect::<Vec<_>>()
                    .join(";");
                println!(
                    "{},{},{},{},{},{},{}",
                    escape_csv(&fact.date),
                    escape_csv(cat_label),
                    escape_csv(&fact.text),
                    escape_csv(&with_str),
                    escape_csv(shared_from),
                    escape_csv(&location_str),
                    escape_csv(&attachments_str)
                );
            }
        }
        OutputFormat::Md => {
            println!("## {} - Timeline\n", person.name);
            println!("| Date | Category | Event | With | Location | Attachments | Shared From |");
            println!("|---|---|---|---|---|---|---|");
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
                let location_str = fact
                    .location
                    .as_ref()
                    .map(|l| format_location(l))
                    .unwrap_or_default();
                let attachments_str: String = fact
                    .attachments
                    .iter()
                    .map(|a| {
                        if let Some(ref title) = a.title {
                            format!("[{}]({})", title, a.url)
                        } else {
                            format!("[link]({})", a.url)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                println!(
                    "| {} | {} | {} | {} | {} | {} | {} |",
                    fact.date, cat_label, fact.text, with_str, location_str, attachments_str, shared_from
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

                // Show location if present
                if let Some(ref location) = result.fact.location {
                    println!("  {} {}", "📍".dimmed(), format_location(location).dimmed());
                }

                // Show attachments if present
                for attachment in &result.fact.attachments {
                    let att_display = if let Some(ref title) = attachment.title {
                        format!("{} ({})", title, attachment.url)
                    } else {
                        attachment.url.to_string()
                    };
                    println!("  {} {}", "📎".dimmed(), att_display.dimmed());
                }
            }
        }
        OutputFormat::Csv => {
            println!("person_id,person_name,date,category,text,location,attachments");
            for result in results {
                let cat_label = chronicle
                    .find_category(&result.fact.category)
                    .map(|c| c.label.as_str())
                    .unwrap_or(&result.fact.category);
                let location_str = result
                    .fact
                    .location
                    .as_ref()
                    .map(|l| format_location(l))
                    .unwrap_or_default();
                let attachments_str: String = result
                    .fact
                    .attachments
                    .iter()
                    .map(|a| a.url.to_string())
                    .collect::<Vec<_>>()
                    .join(";");
                println!(
                    "{},{},{},{},{},{},{}",
                    escape_csv(&result.person.id),
                    escape_csv(&result.person.name),
                    escape_csv(&result.fact.date),
                    escape_csv(cat_label),
                    escape_csv(&result.fact.text),
                    escape_csv(&location_str),
                    escape_csv(&attachments_str)
                );
            }
        }
        OutputFormat::Md => {
            println!("## Search results for '{}'\n", query);
            println!("| Person | Date | Category | Event | Location | Attachments |");
            println!("|---|---|---|---|---|---|");
            for result in results {
                let cat_label = chronicle
                    .find_category(&result.fact.category)
                    .map(|c| c.label.as_str())
                    .unwrap_or(&result.fact.category);
                let location_str = result
                    .fact
                    .location
                    .as_ref()
                    .map(|l| format_location(l))
                    .unwrap_or_default();
                let attachments_str: String = result
                    .fact
                    .attachments
                    .iter()
                    .map(|a| {
                        if let Some(ref title) = a.title {
                            format!("[{}]({})", title, a.url)
                        } else {
                            format!("[link]({})", a.url)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                println!(
                    "| {} | {} | {} | {} | {} | {} |",
                    result.person.name, result.fact.date, cat_label, result.fact.text, location_str, attachments_str
                );
            }
        }
    }

    Ok(())
}

fn cmd_validate(file: &PathBuf, correct: bool, in_place: bool) -> Result<()> {
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

            // Location validation
            if let Some(ref location) = fact.location {
                // Country must not be empty
                if location.country.trim().is_empty() {
                    let msg = format!(
                        "Person '{}', fact '{}': empty country in location",
                        person.id, fact.id
                    );
                    warn!("{}", msg);
                    warnings.push(msg);
                }

                // GPS coordinates validation
                if let Some(ref coords) = location.coordinates {
                    if !coords.is_valid() {
                        let msg = format!(
                            "Person '{}', fact '{}': invalid GPS coordinates (lat: {}, lon: {}). \
                            Valid ranges: lat -90..90, lon -180..180",
                            person.id, fact.id, coords.lat, coords.lon
                        );
                        warn!("{}", msg);
                        warnings.push(msg);
                    }
                }
            }

            // Attachments validation
            for (i, attachment) in fact.attachments.iter().enumerate() {
                // MIME type validation (if present)
                if let Some(ref content_type) = attachment.content_type {
                    if !is_valid_mime_type(content_type) {
                        let msg = format!(
                            "Person '{}', fact '{}': attachment {}: invalid MIME type '{}' \
                            (expected format: type/subtype)",
                            person.id, fact.id, i + 1, content_type
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
        if in_place {
            save(file, &corrected).context("Failed to save corrected chronicle")?;
            println!();
            println!(
                "{}",
                format!("✓ Corrected {} UUID(s) and saved to file", needs_correction.len()).green()
            );
        } else {
            let json = serde_json::to_string_pretty(&corrected)
                .context("Failed to serialize corrected chronicle")?;
            println!();
            println!("{}", "--- Corrected JSON ---".cyan());
            println!("{}", json);
        }
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
    country: Option<String>,
    city: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
    attachments: Option<Vec<String>>,
    attach_type: Option<String>,
    attach_title: Option<String>,
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

    // Build location if country is provided
    let location = if let Some(ref country_name) = country {
        let mut loc = Location::new(country_name);
        if let Some(ref city_name) = city {
            loc = loc.with_city(city_name);
        }
        if let (Some(lat_val), Some(lon_val)) = (lat, lon) {
            let coords = Coordinates::new(lat_val, lon_val);
            if !coords.is_valid() {
                anyhow::bail!(
                    "Invalid GPS coordinates (lat: {}, lon: {}). Valid ranges: lat -90..90, lon -180..180",
                    lat_val, lon_val
                );
            }
            loc = loc.with_coordinates(coords);
        }
        Some(loc)
    } else {
        None
    };

    // Build attachments if provided
    let parsed_attachments: Vec<Attachment> = if let Some(ref urls) = attachments {
        let mut result = Vec::new();
        for url_str in urls {
            let url = Url::parse(url_str).context(format!(
                "Invalid URL '{}'. URLs must include a scheme (e.g., file://, https://, s3://)",
                url_str
            ))?;
            let mut attachment = Attachment::new(url);
            if let Some(ref content_type) = attach_type {
                if !is_valid_mime_type(content_type) {
                    anyhow::bail!(
                        "Invalid MIME type '{}'. Expected format: type/subtype (e.g., image/jpeg)",
                        content_type
                    );
                }
                attachment = attachment.with_content_type(content_type);
            }
            if let Some(ref title) = attach_title {
                attachment = attachment.with_title(title);
            }
            result.push(attachment);
        }
        result
    } else {
        Vec::new()
    };

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
    if let Some(ref loc) = location {
        fact = fact.with_location(loc.clone());
    }
    if !parsed_attachments.is_empty() {
        fact = fact.with_attachments(parsed_attachments.clone());
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

                // Create fact for this person (with same location and attachments)
                let target_fact_id = Uuid::new_v4().to_string();
                let mut target_fact =
                    Fact::new(&target_fact_id, date, category_id, text).with_persons(target_with);
                if let Some(ref loc) = location {
                    target_fact = target_fact.with_location(loc.clone());
                }
                if !parsed_attachments.is_empty() {
                    target_fact = target_fact.with_attachments(parsed_attachments.clone());
                }

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

    // Format location for display
    let location_display = location.as_ref().map(|loc| {
        let mut parts = Vec::new();
        if let Some(ref city_name) = loc.city {
            parts.push(city_name.clone());
        }
        parts.push(loc.country.clone());
        if let Some(ref coords) = loc.coordinates {
            parts.push(format!("({:.4}, {:.4})", coords.lat, coords.lon));
        }
        parts.join(", ")
    });

    // Format attachments for display
    let attachments_display: Vec<String> = parsed_attachments
        .iter()
        .map(|a| {
            let mut s = a.url.to_string();
            if let Some(ref t) = a.title {
                s = format!("{} ({})", t, s);
            }
            s
        })
        .collect();

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
            if let Some(ref loc_str) = location_display {
                println!("    Location: {}", loc_str);
            }
            for att_str in &attachments_display {
                println!("    Attachment: {}", att_str);
            }
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
            if let Some(ref loc_str) = location_display {
                println!("    Location: {}", loc_str);
            }
            for att_str in &attachments_display {
                println!("    Attachment: {}", att_str);
            }
            println!("    UUID: {}", uuid);
        }
    }

    Ok(())
}

fn cmd_merge(
    target_file: &PathBuf,
    source_file: &PathBuf,
    dry_run: bool,
    on_conflict: ConflictStrategy,
    duplicates: DuplicateStrategy,
    regenerate_uuids: bool,
) -> Result<()> {
    let mut target = load(target_file).context("Failed to load target chronicle")?;
    let source = load(source_file).context("Failed to load source chronicle")?;

    // Track merge statistics
    let mut stats = MergeStats::default();

    // Collect existing UUIDs in target (for collision detection)
    let mut existing_uuids: HashSet<String> = HashSet::new();
    for person in &target.persons {
        for fact in &person.facts {
            existing_uuids.insert(fact.id.clone());
        }
    }

    // Build lookup maps for target (owned strings to avoid borrow issues)
    let target_category_ids: HashSet<String> = target.categories.iter().map(|c| c.id.clone()).collect();
    let target_person_ids: HashSet<String> = target.persons.iter().map(|p| p.id.clone()).collect();

    println!(
        "Merging {} into {}...",
        source_file.display(),
        target_file.display()
    );
    println!();

    // === Merge Categories ===
    println!("{}", "Categories:".bold());
    for source_cat in &source.categories {
        if target_category_ids.contains(&source_cat.id) {
            // Conflict: category exists
            let target_cat = target.find_category(&source_cat.id).unwrap();
            let has_diff = target_cat.label != source_cat.label
                || target_cat.color != source_cat.color;

            if has_diff {
                match on_conflict {
                    ConflictStrategy::Skip => {
                        println!("  {} {} (skipped: conflict)", "~".yellow(), source_cat.id);
                        stats.categories_skipped += 1;
                    }
                    ConflictStrategy::Overwrite => {
                        // Find and update the category
                        if let Some(cat) = target.categories.iter_mut().find(|c| c.id == source_cat.id) {
                            cat.label = source_cat.label.clone();
                            cat.color = source_cat.color.clone();
                        }
                        println!("  {} {} (overwritten)", "~".cyan(), source_cat.id);
                        stats.categories_overwritten += 1;
                    }
                    ConflictStrategy::Fail => {
                        anyhow::bail!(
                            "Category conflict: '{}' exists with different values",
                            source_cat.id
                        );
                    }
                }
            } else {
                // Identical, no action needed
                stats.categories_identical += 1;
            }
        } else {
            // New category
            target.categories.push(source_cat.clone());
            println!("  {} {} (new)", "+".green(), source_cat.id);
            stats.categories_added += 1;
        }
    }
    if stats.categories_added == 0 && stats.categories_skipped == 0 && stats.categories_overwritten == 0 {
        println!("  (no changes)");
    }
    println!();

    // === Merge Persons and Facts ===
    println!("{}", "Persons:".bold());
    for source_person in &source.persons {
        if target_person_ids.contains(&source_person.id) {
            // Person exists - merge facts
            // First, gather info we need without holding borrows
            let (target_name, existing_facts): (String, HashSet<(String, String, String)>) = {
                let target_person = target.find_person(&source_person.id).unwrap();
                let facts: HashSet<(String, String, String)> = target_person
                    .facts
                    .iter()
                    .map(|f| (f.date.clone(), f.category.clone(), f.text.clone()))
                    .collect();
                (target_person.name.clone(), facts)
            };

            // Check for name conflict
            if target_name != source_person.name {
                match on_conflict {
                    ConflictStrategy::Skip => {
                        // Keep target name, but still merge facts
                    }
                    ConflictStrategy::Overwrite => {
                        if let Some(p) = target.find_person_mut(&source_person.id) {
                            p.name = source_person.name.clone();
                        }
                    }
                    ConflictStrategy::Fail => {
                        anyhow::bail!(
                            "Person conflict: '{}' has different name ('{}' vs '{}')",
                            source_person.id,
                            target_name,
                            source_person.name
                        );
                    }
                }
            }

            let mut facts_added = 0;
            let mut facts_skipped = 0;

            // Collect facts to add first
            let mut facts_to_add: Vec<Fact> = Vec::new();

            for source_fact in &source_person.facts {
                let fact_key = (
                    source_fact.date.clone(),
                    source_fact.category.clone(),
                    source_fact.text.clone(),
                );

                let is_duplicate = existing_facts.contains(&fact_key);

                if is_duplicate {
                    match duplicates {
                        DuplicateStrategy::Skip => {
                            facts_skipped += 1;
                            stats.facts_skipped += 1;
                            continue;
                        }
                        DuplicateStrategy::Add => {
                            // Will add below with new UUID
                        }
                    }
                }

                // Determine UUID
                let new_uuid = if regenerate_uuids || existing_uuids.contains(&source_fact.id) {
                    let uuid = Uuid::new_v4().to_string();
                    existing_uuids.insert(uuid.clone());
                    uuid
                } else {
                    existing_uuids.insert(source_fact.id.clone());
                    source_fact.id.clone()
                };

                // Create fact with potentially new UUID
                let mut new_fact = Fact::new(&new_uuid, &source_fact.date, &source_fact.category, &source_fact.text);
                if let Some(ref with) = source_fact.with {
                    new_fact = new_fact.with_persons(with.clone());
                }
                if let Some(ref location) = source_fact.location {
                    new_fact = new_fact.with_location(location.clone());
                }
                if !source_fact.attachments.is_empty() {
                    new_fact = new_fact.with_attachments(source_fact.attachments.clone());
                }

                facts_to_add.push(new_fact);
                facts_added += 1;
                stats.facts_added += 1;
            }

            // Now add all facts to target person
            if let Some(p) = target.find_person_mut(&source_person.id) {
                p.facts.extend(facts_to_add);
            }

            if facts_added > 0 || facts_skipped > 0 {
                let mut parts = Vec::new();
                if facts_added > 0 {
                    parts.push(format!("{} facts added", facts_added));
                }
                if facts_skipped > 0 {
                    parts.push(format!("{} duplicates skipped", facts_skipped));
                }
                println!(
                    "  {} {} (merged: {})",
                    "~".cyan(),
                    source_person.id,
                    parts.join(", ")
                );
                stats.persons_merged += 1;
            }
        } else {
            // New person - add entirely
            let mut new_person = source_person.clone();

            // Regenerate UUIDs if needed
            if regenerate_uuids {
                for fact in &mut new_person.facts {
                    let new_uuid = Uuid::new_v4().to_string();
                    existing_uuids.insert(new_uuid.clone());
                    fact.id = new_uuid;
                }
            } else {
                // Check for UUID collisions and fix them
                for fact in &mut new_person.facts {
                    if existing_uuids.contains(&fact.id) {
                        let new_uuid = Uuid::new_v4().to_string();
                        existing_uuids.insert(new_uuid.clone());
                        fact.id = new_uuid;
                    } else {
                        existing_uuids.insert(fact.id.clone());
                    }
                }
            }

            let fact_count = new_person.facts.len();
            target.persons.push(new_person);
            println!(
                "  {} {} (new, {} facts)",
                "+".green(),
                source_person.id,
                fact_count
            );
            stats.persons_added += 1;
            stats.facts_added += fact_count;
        }
    }
    if stats.persons_added == 0 && stats.persons_merged == 0 {
        println!("  (no changes)");
    }
    println!();

    // === Validate with references ===
    let mut with_warnings = Vec::new();
    let final_person_ids: HashSet<&str> = target.persons.iter().map(|p| p.id.as_str()).collect();
    for person in &target.persons {
        for fact in &person.facts {
            if let Some(ref with) = fact.with {
                for with_id in with {
                    if !final_person_ids.contains(with_id.as_str()) {
                        with_warnings.push(format!(
                            "Person '{}', fact '{}': references unknown person '{}'",
                            person.id,
                            fact.text.chars().take(30).collect::<String>(),
                            with_id
                        ));
                    }
                }
            }
        }
    }

    if !with_warnings.is_empty() {
        println!("{}", "Warnings:".yellow());
        for warning in &with_warnings {
            println!("  {} {}", "!".yellow(), warning);
        }
        println!();
    }

    // === Summary ===
    println!("{}", "Summary:".bold());
    println!(
        "  Categories: {} added, {} skipped, {} overwritten",
        stats.categories_added, stats.categories_skipped, stats.categories_overwritten
    );
    println!(
        "  Persons: {} added, {} merged",
        stats.persons_added, stats.persons_merged
    );
    println!(
        "  Facts: {} added, {} skipped (duplicates)",
        stats.facts_added, stats.facts_skipped
    );
    println!();

    // === Save ===
    if dry_run {
        println!("{}", "Dry run - no changes saved".yellow());
    } else {
        save(target_file, &target).context("Failed to save merged chronicle")?;
        println!("{}", format!("✓ Saved to {}", target_file.display()).green());
    }

    Ok(())
}

/// Statistics for merge operation
#[derive(Default)]
struct MergeStats {
    categories_added: usize,
    categories_skipped: usize,
    categories_overwritten: usize,
    categories_identical: usize,
    persons_added: usize,
    persons_merged: usize,
    facts_added: usize,
    facts_skipped: usize,
}

fn format_date_display(date_str: &str) -> String {
    // Just return the date as-is for now, padded
    date_str.to_string()
}

/// Format a location for display.
fn format_location(location: &Location) -> String {
    let mut parts = Vec::new();
    if let Some(ref city) = location.city {
        parts.push(city.clone());
    }
    parts.push(location.country.clone());
    if let Some(ref coords) = location.coordinates {
        parts.push(format!("({:.4}, {:.4})", coords.lat, coords.lon));
    }
    parts.join(", ")
}

/// Check if a string is a valid MIME type (basic format: type/subtype).
fn is_valid_mime_type(s: &str) -> bool {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() != 2 {
        return false;
    }
    let type_part = parts[0];
    let subtype_part = parts[1];

    // Both parts must be non-empty and contain only valid characters
    // Valid MIME characters: alphanumeric, hyphen, plus, dot
    let is_valid_part = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '+' || c == '.')
    };

    is_valid_part(type_part) && is_valid_part(subtype_part)
}
