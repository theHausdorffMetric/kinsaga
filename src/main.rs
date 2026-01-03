//! Kinsaga CLI - Family chronicle management tool.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;
use kinsaga::{
    add_fact, build_attachments, build_location, collect_timeline_facts, edit_fact,
    correct_uuids, escape_csv, format_attachment, format_date_display, format_location,
    load, merge_chronicles, save, search, validate_chronicle,
    AddFactOptions, AttachmentUpdate, ChronicleDate, EditFactOptions, Fact, FactFilter,
    IssueType, LocationUpdate, MergeOptions, WithUpdate,
    ConflictStrategy as LibConflictStrategy, DuplicateStrategy as LibDuplicateStrategy,
};
use std::collections::HashMap;
use std::path::PathBuf;

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
    /// JSON output
    Json,
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
#[allow(clippy::large_enum_variant)]
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
        /// Text to search for (or regex pattern with --regex)
        query: String,

        /// Output format
        #[arg(short, long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,

        /// Treat query as a regex pattern
        #[arg(short = 'r', long)]
        regex: bool,
    },

    /// Validate a chronicle JSON file
    Validate {
        /// Generate valid UUIDs for empty/invalid ones and output corrected JSON to stdout
        #[arg(long)]
        correct: bool,

        /// Write changes back to the input file (requires --correct or --apply)
        #[arg(long)]
        in_place: bool,

        /// Validate GPS coordinates against Nominatim (reverse geocoding)
        #[arg(long)]
        gps: bool,

        /// Suggest GPS coordinates for locations without them (requires --gps)
        #[arg(long, requires = "gps")]
        suggest: bool,

        /// Apply suggested GPS coordinates and output JSON to stdout (requires --suggest)
        #[arg(long, requires = "suggest")]
        apply: bool,
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

        /// Place name: city, address, landmark, etc. (optional, requires --country)
        #[arg(long, requires = "country")]
        place: Option<String>,

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

    /// Edit an existing fact by UUID
    EditFact {
        /// UUID of the fact to edit
        uuid: String,

        /// New date (ISO 8601 format)
        #[arg(short, long)]
        date: Option<String>,

        /// New category ID
        #[arg(short, long)]
        category: Option<String>,

        /// New description text
        #[arg(short, long)]
        text: Option<String>,

        /// Replace 'with' list (comma-separated person IDs)
        #[arg(short, long, value_delimiter = ',')]
        with: Option<Vec<String>>,

        /// Clear all 'with' references
        #[arg(long)]
        clear_with: bool,

        /// Set/update country
        #[arg(long)]
        country: Option<String>,

        /// Set/update place name
        #[arg(long)]
        place: Option<String>,

        /// Set/update GPS latitude
        #[arg(long, allow_hyphen_values = true)]
        lat: Option<f64>,

        /// Set/update GPS longitude
        #[arg(long, allow_hyphen_values = true)]
        lon: Option<f64>,

        /// Clear location entirely
        #[arg(long)]
        clear_location: bool,

        /// Add attachment URL
        #[arg(long = "add-attach", value_name = "URL")]
        add_attachments: Option<Vec<String>>,

        /// Remove attachment by URL
        #[arg(long = "remove-attach", value_name = "URL")]
        remove_attachments: Option<Vec<String>>,

        /// Clear all attachments
        #[arg(long)]
        clear_attachments: bool,

        /// Preview without saving
        #[arg(long)]
        dry_run: bool,
    },

    /// Print the JSON Schema for chronicle files
    Schema,
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

    // Handle commands that don't require an input file
    if let Commands::Schema = cli.command {
        return cmd_schema();
    }

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
        Commands::Search { query, format, regex } => cmd_search(&input, &query, &format, regex),
        Commands::Validate { correct, in_place, gps, suggest, apply } => cmd_validate(&input, correct, in_place, gps, suggest, apply),
        Commands::AddFact {
            person,
            date,
            category,
            text,
            with,
            country,
            place,
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
            place,
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
        Commands::EditFact {
            uuid,
            date,
            category,
            text,
            with,
            clear_with,
            country,
            place,
            lat,
            lon,
            clear_location,
            add_attachments,
            remove_attachments,
            clear_attachments,
            dry_run,
        } => cmd_edit_fact(
            &input,
            &uuid,
            date,
            category,
            text,
            with,
            clear_with,
            country,
            place,
            lat,
            lon,
            clear_location,
            add_attachments,
            remove_attachments,
            clear_attachments,
            dry_run,
        ),
        Commands::Schema => unreachable!(), // Handled above
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
        OutputFormat::Json => {
            let json_list: Vec<PersonSummaryJson> = chronicle
                .persons
                .iter()
                .map(|p| PersonSummaryJson {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    fact_count: p.facts.len(),
                })
                .collect();
            let json = serde_json::to_string_pretty(&json_list)
                .context("Failed to serialize person list")?;
            println!("{}", json);
        }
    }

    Ok(())
}

/// JSON-serializable person summary for list command
#[derive(serde::Serialize)]
struct PersonSummaryJson {
    id: String,
    name: String,
    fact_count: usize,
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

    // Use library function to collect all facts
    let all_facts = collect_timeline_facts(&chronicle, person_id, &filter, include_shared);

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

            for timeline_fact in &all_facts {
                let fact = timeline_fact.fact;
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
                let shared_indicator = match timeline_fact.shared_from {
                    Some(name) => format!(" {}", format!("(via {})", name).dimmed()),
                    None => String::new(),
                };

                println!(
                    "  {} {:<14} {:<20} {}{}",
                    if timeline_fact.shared_from.is_some() {
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
                    println!("    {} {}", "📎".dimmed(), format_attachment(attachment).dimmed());
                }
            }
            println!();
        }
        OutputFormat::Csv => {
            println!("date,category,text,with,shared_from,location,attachments");
            for timeline_fact in &all_facts {
                let fact = timeline_fact.fact;
                let cat_label = chronicle
                    .find_category(&fact.category)
                    .map(|c| c.label.as_str())
                    .unwrap_or(&fact.category);
                let with_str = fact
                    .with
                    .as_ref()
                    .map(|w| w.join(";"))
                    .unwrap_or_default();
                let shared_from = timeline_fact.shared_from.unwrap_or("");
                let location_str = fact
                    .location
                    .as_ref()
                    .map(format_location)
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
            for timeline_fact in &all_facts {
                let fact = timeline_fact.fact;
                let cat_label = chronicle
                    .find_category(&fact.category)
                    .map(|c| c.label.as_str())
                    .unwrap_or(&fact.category);
                let with_str = fact
                    .with
                    .as_ref()
                    .map(|w| w.join(", "))
                    .unwrap_or_default();
                let shared_from = timeline_fact.shared_from.unwrap_or("");
                let location_str = fact
                    .location
                    .as_ref()
                    .map(format_location)
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
        OutputFormat::Json => {
            let json_facts: Vec<TimelineFactJson> = all_facts
                .iter()
                .map(|f| TimelineFactJson {
                    fact: f.fact.clone(),
                    shared_from: f.shared_from.map(|s| s.to_string()),
                })
                .collect();
            let json = serde_json::to_string_pretty(&json_facts)
                .context("Failed to serialize timeline")?;
            println!("{}", json);
        }
    }

    Ok(())
}

/// JSON-serializable timeline fact
#[derive(serde::Serialize)]
struct TimelineFactJson {
    #[serde(flatten)]
    fact: Fact,
    #[serde(skip_serializing_if = "Option::is_none")]
    shared_from: Option<String>,
}

fn cmd_search(file: &PathBuf, query: &str, format: &OutputFormat, use_regex: bool) -> Result<()> {
    let chronicle = load(file).context("Failed to load chronicle")?;

    let filter = FactFilter::new().with_text(query).with_regex(use_regex);
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
                    .map(format_location)
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
                    .map(format_location)
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
        OutputFormat::Json => {
            let json_results: Vec<SearchResultJson> = results
                .iter()
                .map(|r| SearchResultJson {
                    person_id: r.person.id.clone(),
                    person_name: r.person.name.clone(),
                    fact: r.fact.clone(),
                })
                .collect();
            let json = serde_json::to_string_pretty(&json_results)
                .context("Failed to serialize search results")?;
            println!("{}", json);
        }
    }

    Ok(())
}

/// JSON-serializable search result
#[derive(serde::Serialize)]
struct SearchResultJson {
    person_id: String,
    person_name: String,
    fact: Fact,
}

fn cmd_validate(file: &PathBuf, correct: bool, in_place: bool, gps: bool, suggest: bool, apply: bool) -> Result<()> {
    use kinsaga::{fuzzy_match, Coordinates, NominatimClient};

    // Validate --in-place requires --correct or --apply
    if in_place && !correct && !apply {
        anyhow::bail!("--in-place requires --correct or --apply");
    }

    let mut chronicle = load(file).context("Failed to load chronicle")?;

    // Use library validation function
    let result = validate_chronicle(&chronicle);

    // Print results
    println!("{}", "✓ Valid UTF-8".green());
    println!("{}", "✓ Valid JSON structure".green());
    println!(
        "{} {} persons, {} facts",
        "✓".green(),
        result.person_count,
        result.fact_count
    );

    if result.is_valid() {
        println!("{}", "✓ All references and UUIDs valid".green());
    } else {
        if !result.errors.is_empty() {
            println!();
            println!("{}", "Errors:".red());
            for issue in &result.errors {
                println!("  {} {}", "✗".red(), issue.message);
            }
        }
        if !result.warnings.is_empty() {
            println!();
            println!("{}", "Warnings:".yellow());
            for issue in &result.warnings {
                // Show issue type indicator
                let indicator = match issue.issue_type {
                    IssueType::DuplicateUuid => "✗".red(),
                    _ => "!".yellow(),
                };
                println!("  {} {}", indicator, issue.message);
            }
        }
    }

    // Handle --correct flag
    if correct && !result.needs_correction.is_empty() {
        let corrected = correct_uuids(chronicle.clone(), &result.needs_correction);
        if in_place {
            save(file, &corrected).context("Failed to save corrected chronicle")?;
            println!();
            println!(
                "{}",
                format!("✓ Corrected {} UUID(s) and saved to file", result.needs_correction.len()).green()
            );
        } else {
            let json = serde_json::to_string_pretty(&corrected)
                .context("Failed to serialize corrected chronicle")?;
            println!();
            println!("{}", "--- Corrected JSON ---".cyan());
            println!("{}", json);
        }
    } else if correct && result.needs_correction.is_empty() {
        println!();
        println!("{}", "No UUID corrections needed.".green());
    }

    // Handle --gps flag
    if gps {
        println!();
        println!("{}", "Validating GPS coordinates...".cyan());

        let mut client = NominatimClient::new("kinsaga/0.1.0 (https://git.sr.ht/~danprobst/kinsaga)");

        // Collect facts with coordinates
        let facts_with_coords: Vec<_> = chronicle
            .persons
            .iter()
            .flat_map(|p| {
                p.facts.iter().filter_map(|f| {
                    f.location.as_ref().and_then(|loc| {
                        loc.coordinates.as_ref().map(|coords| {
                            (p.name.as_str(), f.id.as_str(), loc, coords)
                        })
                    })
                })
            })
            .collect();

        if facts_with_coords.is_empty() {
            println!("  No facts with GPS coordinates found.");
        } else {
            println!(
                "  Checking {} location(s) with coordinates (1 req/sec rate limit)...",
                facts_with_coords.len()
            );
            println!();

            let mut checked = 0;
            let mut mismatches = 0;
            let mut errors = 0;

            for (person_name, fact_id, location, coords) in facts_with_coords {
                checked += 1;

                match client.reverse_geocode(coords.lat, coords.lon) {
                    Ok(place) => {
                        let country_matches = place
                            .country
                            .as_ref()
                            .is_some_and(|c| fuzzy_match(c, &location.country));

                        let place_matches = location.place.as_ref().map(|stored_place| {
                            // Check if stored place matches city, state, or display_name
                            place.city.as_ref().is_some_and(|c| fuzzy_match(c, stored_place))
                                || place.state.as_ref().is_some_and(|s| fuzzy_match(s, stored_place))
                                || fuzzy_match(&place.display_name, stored_place)
                        });

                        let is_mismatch = !country_matches || place_matches == Some(false);

                        if is_mismatch {
                            mismatches += 1;
                            println!(
                                "  {} {} ({})",
                                "⚠".yellow(),
                                fact_id,
                                person_name.dimmed()
                            );
                            println!(
                                "    Stored:    {}, {}",
                                location.place.as_deref().unwrap_or("-"),
                                location.country
                            );
                            println!(
                                "    GPS says:  {}",
                                place.display_name
                            );
                            if !country_matches {
                                println!(
                                    "    {}",
                                    format!(
                                        "Country mismatch: '{}' vs '{}'",
                                        location.country,
                                        place.country.as_deref().unwrap_or("unknown")
                                    ).yellow()
                                );
                            }
                            println!();
                        }
                    }
                    Err(e) => {
                        errors += 1;
                        println!(
                            "  {} {} ({}): {}",
                            "✗".red(),
                            fact_id,
                            person_name.dimmed(),
                            e
                        );
                    }
                }
            }

            // Summary
            println!(
                "GPS validation: {} checked, {} mismatch(es), {} error(s)",
                checked,
                if mismatches > 0 {
                    mismatches.to_string().yellow().to_string()
                } else {
                    "0".green().to_string()
                },
                if errors > 0 {
                    errors.to_string().red().to_string()
                } else {
                    "0".to_string()
                }
            );
        }

        // Handle --suggest flag: find locations without coordinates and suggest them
        if suggest {
            println!();
            println!("{}", "Suggesting GPS coordinates for locations without them...".cyan());

            // Collect facts with location but no coordinates (need owned data for apply)
            let facts_without_coords: Vec<_> = chronicle
                .persons
                .iter()
                .flat_map(|p| {
                    p.facts.iter().filter_map(|f| {
                        f.location.as_ref().and_then(|loc| {
                            if loc.coordinates.is_none() {
                                Some((p.name.clone(), f.id.clone(), loc.country.clone(), loc.place.clone()))
                            } else {
                                None
                            }
                        })
                    })
                })
                .collect();

            if facts_without_coords.is_empty() {
                println!("  No locations without GPS coordinates found.");
            } else {
                println!(
                    "  Looking up {} location(s) (1 req/sec rate limit)...",
                    facts_without_coords.len()
                );
                println!();

                let mut suggested = 0;
                let mut no_results = 0;
                let mut errors = 0;

                // Collect coordinates to apply (fact_id -> (lat, lon))
                let mut coords_to_apply: Vec<(String, f64, f64)> = Vec::new();

                for (person_name, fact_id, country, place) in facts_without_coords {
                    // Build search query from place and country
                    let query = if let Some(ref p) = place {
                        format!("{}, {}", p, country)
                    } else {
                        country.clone()
                    };

                    match client.forward_geocode(&query, 3) {
                        Ok(results) => {
                            if results.is_empty() {
                                no_results += 1;
                                println!(
                                    "  {} {} ({}): no results for \"{}\"",
                                    "?".dimmed(),
                                    fact_id,
                                    person_name.dimmed(),
                                    query
                                );
                            } else {
                                suggested += 1;

                                // If apply is set, collect the top result
                                if apply {
                                    let top = &results[0];
                                    coords_to_apply.push((fact_id.clone(), top.lat, top.lon));
                                }

                                println!(
                                    "  {} {} ({})",
                                    if apply { "✓".green() } else { "→".green() },
                                    fact_id,
                                    person_name.dimmed()
                                );
                                println!(
                                    "    Query: \"{}\"",
                                    query
                                );

                                for (i, place) in results.iter().enumerate() {
                                    let marker = if i == 0 { "★" } else { "○" };
                                    println!(
                                        "    {} {:.6}, {:.6} - {}",
                                        if i == 0 { marker.green() } else { marker.dimmed() },
                                        place.lat,
                                        place.lon,
                                        place.display_name
                                    );
                                }
                                println!();
                            }
                        }
                        Err(kinsaga::GeocodeError::NoResults) => {
                            no_results += 1;
                            println!(
                                "  {} {} ({}): no results for \"{}\"",
                                "?".dimmed(),
                                fact_id,
                                person_name.dimmed(),
                                query
                            );
                        }
                        Err(e) => {
                            errors += 1;
                            println!(
                                "  {} {} ({}): {}",
                                "✗".red(),
                                fact_id,
                                person_name.dimmed(),
                                e
                            );
                        }
                    }
                }

                // Summary
                println!();
                println!(
                    "GPS suggestions: {} found, {} no results, {} error(s)",
                    if suggested > 0 {
                        suggested.to_string().green().to_string()
                    } else {
                        "0".to_string()
                    },
                    no_results,
                    if errors > 0 {
                        errors.to_string().red().to_string()
                    } else {
                        "0".to_string()
                    }
                );

                // Apply coordinates if --apply flag is set
                if apply && !coords_to_apply.is_empty() {
                    let mut applied = 0;
                    for (fact_id, lat, lon) in &coords_to_apply {
                        // Find and update the fact
                        for person in &mut chronicle.persons {
                            for fact in &mut person.facts {
                                if &fact.id == fact_id
                                    && let Some(ref mut location) = fact.location
                                {
                                    location.coordinates = Some(Coordinates::new(*lat, *lon));
                                    applied += 1;
                                }
                            }
                        }
                    }

                    if in_place {
                        // Save the updated chronicle to file
                        save(file, &chronicle).context("Failed to save chronicle")?;
                        println!();
                        println!(
                            "{}",
                            format!("✓ Applied {} coordinate(s) and saved to file", applied).green()
                        );
                    } else {
                        // Output JSON to stdout
                        let json = serde_json::to_string_pretty(&chronicle)
                            .context("Failed to serialize chronicle")?;
                        println!();
                        println!("{}", "--- Updated JSON ---".cyan());
                        println!("{}", json);
                    }
                }
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_add_fact(
    file: &PathBuf,
    person_id: &str,
    date: &str,
    category_id: &str,
    text: &str,
    with: Option<Vec<String>>,
    country: Option<String>,
    place: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
    attachments: Option<Vec<String>>,
    attach_type: Option<String>,
    attach_title: Option<String>,
    dry_run: bool,
    propagate: bool,
) -> Result<()> {
    let mut chronicle = load(file).context("Failed to load chronicle")?;

    // Get category label for display
    let category_label = chronicle
        .find_category(category_id)
        .map(|c| c.label.clone())
        .unwrap_or_else(|| category_id.to_string());

    // Build location using library function
    let location = build_location(country, place, lat, lon)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Build attachments using library function
    let parsed_attachments = build_attachments(attachments, attach_type, attach_title)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Warn if --propagate is used without --with
    if propagate && with.is_none() {
        println!(
            "{}",
            "Warning: --propagate has no effect without --with".yellow()
        );
    }

    // Use library function to add fact
    let options = AddFactOptions {
        date: date.to_string(),
        category: category_id.to_string(),
        text: text.to_string(),
        with,
        location: location.clone(),
        attachments: parsed_attachments.clone(),
        propagate,
    };

    let result = add_fact(&mut chronicle, person_id, options)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Format location for display
    let location_display = location.as_ref().map(format_location);

    // Format attachments for display
    let attachments_display: Vec<String> = parsed_attachments
        .iter()
        .map(format_attachment)
        .collect();

    if dry_run {
        println!("{}", "Dry run - not saving changes".yellow());
        println!();
        println!("Would add {} fact(s):", result.facts_added.len());
        for (_, name, uuid) in &result.facts_added {
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
            format!("{} fact(s) added successfully", result.facts_added.len()).green()
        );
        for (_, name, uuid) in &result.facts_added {
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

#[allow(clippy::too_many_arguments)]
fn cmd_edit_fact(
    file: &PathBuf,
    uuid: &str,
    date: Option<String>,
    category: Option<String>,
    text: Option<String>,
    with: Option<Vec<String>>,
    clear_with: bool,
    country: Option<String>,
    place: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
    clear_location: bool,
    add_attachments: Option<Vec<String>>,
    remove_attachments: Option<Vec<String>>,
    clear_attachments: bool,
    dry_run: bool,
) -> Result<()> {
    use kinsaga::{Attachment, Coordinates, Location, Url};

    let mut chronicle = load(file).context("Failed to load chronicle")?;

    // Build location update
    let location_update = if clear_location {
        Some(LocationUpdate::Clear)
    } else if country.is_some() || place.is_some() || lat.is_some() || lon.is_some() {
        // Build a new location from provided fields
        // If we're updating, we need to get current location first for partial updates
        let fact_location = chronicle
            .persons
            .iter()
            .flat_map(|p| p.facts.iter())
            .find(|f| f.id == uuid)
            .and_then(|f| f.location.as_ref());

        let base_country = country
            .or_else(|| fact_location.map(|l| l.country.clone()))
            .ok_or_else(|| anyhow::anyhow!("--country is required when setting location"))?;

        let mut loc = Location::new(base_country);

        // Use new place if provided, otherwise keep existing
        if let Some(p) = place {
            loc = loc.with_place(p);
        } else if let Some(existing) = fact_location.and_then(|l| l.place.clone()) {
            loc = loc.with_place(existing);
        }

        // Handle coordinates
        if lat.is_some() || lon.is_some() {
            let lat_val = lat
                .or_else(|| fact_location.and_then(|l| l.coordinates.as_ref()).map(|c| c.lat))
                .ok_or_else(|| anyhow::anyhow!("--lat is required with --lon"))?;
            let lon_val = lon
                .or_else(|| fact_location.and_then(|l| l.coordinates.as_ref()).map(|c| c.lon))
                .ok_or_else(|| anyhow::anyhow!("--lon is required with --lat"))?;
            loc = loc.with_coordinates(Coordinates::new(lat_val, lon_val));
        } else if let Some(existing_coords) = fact_location.and_then(|l| l.coordinates.clone()) {
            loc = loc.with_coordinates(existing_coords);
        }

        Some(LocationUpdate::Set(loc))
    } else {
        None
    };

    // Build with update
    let with_update = if clear_with {
        Some(WithUpdate::Clear)
    } else {
        with.map(WithUpdate::Replace)
    };

    // Build attachment update
    let attachment_update = if clear_attachments {
        Some(AttachmentUpdate::Clear)
    } else if let Some(urls) = add_attachments {
        let mut attachments = Vec::new();
        for url_str in urls {
            let url = Url::parse(&url_str).map_err(|_| {
                anyhow::anyhow!(
                    "Invalid URL '{}'. URLs must include a scheme (e.g., file://, https://)",
                    url_str
                )
            })?;
            attachments.push(Attachment::new(url));
        }
        Some(AttachmentUpdate::Add(attachments))
    } else {
        remove_attachments.map(AttachmentUpdate::Remove)
    };

    let options = EditFactOptions {
        date: date.clone(),
        category: category.clone(),
        text: text.clone(),
        with: with_update,
        location: location_update,
        attachments: attachment_update,
    };

    // Check if any updates were specified
    if options.date.is_none()
        && options.category.is_none()
        && options.text.is_none()
        && options.with.is_none()
        && options.location.is_none()
        && options.attachments.is_none()
    {
        anyhow::bail!("No changes specified. Use --date, --category, --text, --with, --country, --place, --lat, --lon, --add-attach, --remove-attach, or clear flags.");
    }

    let result = edit_fact(&mut chronicle, uuid, options).map_err(|e| anyhow::anyhow!("{}", e))?;

    // Get the updated fact for display
    let updated_fact = chronicle
        .persons
        .iter()
        .flat_map(|p| p.facts.iter())
        .find(|f| f.id == uuid)
        .unwrap();

    let category_label = chronicle
        .find_category(&updated_fact.category)
        .map(|c| c.label.clone())
        .unwrap_or_else(|| updated_fact.category.clone());

    if dry_run {
        println!("{}", "Dry run - not saving changes".yellow());
        println!();
        println!("Would update fact for {}:", result.person_name.bold());
        println!(
            "  {} {} [{}] {}",
            "●".cyan(),
            updated_fact.date,
            category_label,
            updated_fact.text
        );
        if let Some(ref loc) = updated_fact.location {
            println!("  Location: {}", format_location(loc));
        }
        for att in &updated_fact.attachments {
            println!("  Attachment: {}", format_attachment(att));
        }
        if let Some(ref with_ids) = updated_fact.with {
            println!("  With: {}", with_ids.join(", "));
        }
        println!("  UUID: {}", uuid);
    } else {
        save(file, &chronicle).context("Failed to save chronicle")?;
        println!("{}", "Fact updated successfully".green());
        println!();
        println!("{}:", result.person_name.bold());
        println!(
            "  {} {} [{}] {}",
            "●".cyan(),
            updated_fact.date,
            category_label,
            updated_fact.text
        );
        if let Some(ref loc) = updated_fact.location {
            println!("  Location: {}", format_location(loc));
        }
        for att in &updated_fact.attachments {
            println!("  Attachment: {}", format_attachment(att));
        }
        if let Some(ref with_ids) = updated_fact.with {
            println!("  With: {}", with_ids.join(", "));
        }
        println!("  UUID: {}", uuid);
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
    let target = load(target_file).context("Failed to load target chronicle")?;
    let source = load(source_file).context("Failed to load source chronicle")?;

    // Convert CLI enums to library enums
    let lib_conflict = match on_conflict {
        ConflictStrategy::Skip => LibConflictStrategy::Skip,
        ConflictStrategy::Overwrite => LibConflictStrategy::Overwrite,
        ConflictStrategy::Fail => LibConflictStrategy::Fail,
    };
    let lib_duplicates = match duplicates {
        DuplicateStrategy::Skip => LibDuplicateStrategy::Skip,
        DuplicateStrategy::Add => LibDuplicateStrategy::Add,
    };

    let options = MergeOptions {
        on_conflict: lib_conflict,
        duplicates: lib_duplicates,
        regenerate_uuids,
    };

    println!(
        "Merging {} into {}...",
        source_file.display(),
        target_file.display()
    );
    println!();

    // Use library merge function
    let result = merge_chronicles(target, &source, &options)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Display events
    use kinsaga::{MergeEventType, MergeItemType};

    println!("{}", "Categories:".bold());
    let cat_events: Vec<_> = result.events.iter()
        .filter(|e| matches!(e.item_type, MergeItemType::Category))
        .collect();
    if cat_events.is_empty() && result.stats.categories_identical == 0 {
        println!("  (no changes)");
    } else {
        for event in cat_events {
            match event.event_type {
                MergeEventType::Added => println!("  {} {} (new)", "+".green(), event.id),
                MergeEventType::Skipped => println!("  {} {} (skipped: conflict)", "~".yellow(), event.id),
                MergeEventType::Overwritten => println!("  {} {} (overwritten)", "~".cyan(), event.id),
                MergeEventType::Merged => {}
            }
        }
        if result.stats.categories_added == 0 && result.stats.categories_skipped == 0 && result.stats.categories_overwritten == 0 {
            println!("  (no changes)");
        }
    }
    println!();

    println!("{}", "Persons:".bold());
    let person_events: Vec<_> = result.events.iter()
        .filter(|e| matches!(e.item_type, MergeItemType::Person))
        .collect();
    if person_events.is_empty() {
        println!("  (no changes)");
    } else {
        for event in person_events {
            match event.event_type {
                MergeEventType::Added => {
                    let details = event.details.as_deref().unwrap_or("");
                    println!("  {} {} (new, {})", "+".green(), event.id, details);
                }
                MergeEventType::Merged => {
                    let details = event.details.as_deref().unwrap_or("");
                    println!("  {} {} (merged: {})", "~".cyan(), event.id, details);
                }
                _ => {}
            }
        }
    }
    println!();

    // Display warnings
    if !result.warnings.is_empty() {
        println!("{}", "Warnings:".yellow());
        for warning in &result.warnings {
            println!("  {} {}", "!".yellow(), warning);
        }
        println!();
    }

    // Summary
    println!("{}", "Summary:".bold());
    println!(
        "  Categories: {} added, {} skipped, {} overwritten",
        result.stats.categories_added, result.stats.categories_skipped, result.stats.categories_overwritten
    );
    println!(
        "  Persons: {} added, {} merged",
        result.stats.persons_added, result.stats.persons_merged
    );
    println!(
        "  Facts: {} added, {} skipped (duplicates)",
        result.stats.facts_added, result.stats.facts_skipped
    );
    println!();

    // Save
    if dry_run {
        println!("{}", "Dry run - no changes saved".yellow());
    } else {
        save(target_file, &result.chronicle).context("Failed to save merged chronicle")?;
        println!("{}", format!("✓ Saved to {}", target_file.display()).green());
    }

    Ok(())
}

/// Embedded JSON Schema for chronicle files
const CHRONICLE_SCHEMA: &str = include_str!("../schema.json");

fn cmd_schema() -> Result<()> {
    println!("{}", CHRONICLE_SCHEMA);
    Ok(())
}
