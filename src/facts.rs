//! Fact creation and manipulation operations.

use crate::{Attachment, Chronicle, ChronicleDate, Coordinates, Fact, Location};
use crate::filter::{filter_facts, FactFilter};
use crate::validate::is_valid_mime_type;
use url::Url;
use uuid::Uuid;

/// Options for adding a new fact.
#[derive(Debug, Clone, Default)]
pub struct AddFactOptions {
    /// Date in ISO 8601 format
    pub date: String,
    /// Category ID
    pub category: String,
    /// Event description
    pub text: String,
    /// Other person IDs involved
    pub with: Option<Vec<String>>,
    /// Location where the event occurred
    pub location: Option<Location>,
    /// Attachments for this fact
    pub attachments: Vec<Attachment>,
    /// Whether to propagate to persons in `with`
    pub propagate: bool,
}

/// Result of adding a fact.
#[derive(Debug, Clone)]
pub struct AddFactResult {
    /// Facts that were added: (person_id, person_name, fact_id)
    pub facts_added: Vec<(String, String, String)>,
}

/// Error that can occur when adding a fact.
#[derive(Debug, Clone)]
pub struct AddFactError {
    pub message: String,
}

impl std::fmt::Display for AddFactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AddFactError {}

/// Add a fact to a person's timeline.
///
/// If `options.propagate` is true, the fact will also be added to all persons
/// listed in `options.with`, with appropriate cross-references.
pub fn add_fact(
    chronicle: &mut Chronicle,
    person_id: &str,
    options: AddFactOptions,
) -> Result<AddFactResult, AddFactError> {
    // Validate person exists
    if chronicle.find_person(person_id).is_none() {
        return Err(AddFactError {
            message: format!(
                "Person '{}' not found. Available persons: {}",
                person_id,
                chronicle
                    .persons
                    .iter()
                    .map(|p| p.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }

    // Validate date format
    if let Err(e) = ChronicleDate::parse(&options.date) {
        return Err(AddFactError {
            message: format!("Invalid date format '{}': {}", options.date, e),
        });
    }

    // Validate category exists
    if chronicle.find_category(&options.category).is_none() {
        return Err(AddFactError {
            message: format!(
                "Category '{}' not found. Available categories: {}",
                options.category,
                chronicle
                    .categories
                    .iter()
                    .map(|c| c.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }

    // Validate 'with' references
    if let Some(ref with_ids) = options.with {
        for with_id in with_ids {
            if chronicle.find_person(with_id).is_none() {
                return Err(AddFactError {
                    message: format!(
                        "Person '{}' in 'with' not found. Available persons: {}",
                        with_id,
                        chronicle
                            .persons
                            .iter()
                            .map(|p| p.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                });
            }
        }
    }

    // Validate location if present
    if let Some(ref location) = options.location {
        if location.country.trim().is_empty() {
            return Err(AddFactError {
                message: "Country cannot be empty".to_string(),
            });
        }
        if let Some(ref coords) = location.coordinates {
            if !coords.is_valid() {
                return Err(AddFactError {
                    message: format!(
                        "Invalid GPS coordinates (lat: {}, lon: {}). Valid ranges: lat -90..90, lon -180..180",
                        coords.lat, coords.lon
                    ),
                });
            }
        }
    }

    // Validate attachments
    for attachment in &options.attachments {
        if let Some(ref content_type) = attachment.content_type {
            if !is_valid_mime_type(content_type) {
                return Err(AddFactError {
                    message: format!(
                        "Invalid MIME type '{}'. Expected format: type/subtype (e.g., image/jpeg)",
                        content_type
                    ),
                });
            }
        }
    }

    let mut facts_added = Vec::new();

    // Create fact for the main person
    let fact_id = Uuid::new_v4().to_string();
    let mut fact = Fact::new(&fact_id, &options.date, &options.category, &options.text);
    if let Some(ref with_ids) = options.with {
        fact = fact.with_persons(with_ids.clone());
    }
    if let Some(ref location) = options.location {
        fact = fact.with_location(location.clone());
    }
    if !options.attachments.is_empty() {
        fact = fact.with_attachments(options.attachments.clone());
    }

    // Get person name for result
    let person_name = chronicle.find_person(person_id).unwrap().name.clone();
    facts_added.push((person_id.to_string(), person_name, fact_id.clone()));

    // Add fact to main person
    let person = chronicle.find_person_mut(person_id).unwrap();
    person.facts.push(fact);

    // If propagate is enabled, create facts for 'with' persons
    if options.propagate {
        if let Some(ref with_ids) = options.with {
            for target_id in with_ids {
                // Build the 'with' list: original person + other with persons
                let mut target_with: Vec<String> = vec![person_id.to_string()];
                for other_id in with_ids {
                    if other_id != target_id {
                        target_with.push(other_id.clone());
                    }
                }

                // Create fact for this person
                let target_fact_id = Uuid::new_v4().to_string();
                let mut target_fact = Fact::new(
                    &target_fact_id,
                    &options.date,
                    &options.category,
                    &options.text,
                )
                .with_persons(target_with);

                if let Some(ref location) = options.location {
                    target_fact = target_fact.with_location(location.clone());
                }
                if !options.attachments.is_empty() {
                    target_fact = target_fact.with_attachments(options.attachments.clone());
                }

                let target_name = chronicle.find_person(target_id).unwrap().name.clone();
                facts_added.push((target_id.clone(), target_name, target_fact_id));

                let target_person = chronicle.find_person_mut(target_id).unwrap();
                target_person.facts.push(target_fact);
            }
        }
    }

    Ok(AddFactResult { facts_added })
}

/// A fact with source information for timeline display.
#[derive(Debug, Clone)]
pub struct TimelineFact<'a> {
    /// The fact
    pub fact: &'a Fact,
    /// If shared from another person, their name
    pub shared_from: Option<&'a str>,
}

/// Collect facts for a person's timeline, optionally including shared facts.
///
/// Returns facts sorted by date.
pub fn collect_timeline_facts<'a>(
    chronicle: &'a Chronicle,
    person_id: &str,
    filter: &FactFilter,
    include_shared: bool,
) -> Vec<TimelineFact<'a>> {
    let person = match chronicle.find_person(person_id) {
        Some(p) => p,
        None => return Vec::new(),
    };

    // Collect own facts
    let mut all_facts: Vec<TimelineFact> = filter_facts(person, filter)
        .into_iter()
        .map(|fact| TimelineFact {
            fact,
            shared_from: None,
        })
        .collect();

    // Build set of (date, category, text) for owned facts (for deduplication)
    let owned_keys: std::collections::HashSet<(&str, &str, &str)> = all_facts
        .iter()
        .map(|f| {
            (
                f.fact.date.as_str(),
                f.fact.category.as_str(),
                f.fact.text.as_str(),
            )
        })
        .collect();

    // Collect shared facts if requested
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

                if !is_shared {
                    continue;
                }

                // Skip if this fact duplicates an owned fact
                let key = (
                    fact.date.as_str(),
                    fact.category.as_str(),
                    fact.text.as_str(),
                );
                if owned_keys.contains(&key) {
                    continue;
                }

                // Apply filters
                if !filter.matches(fact) {
                    continue;
                }

                all_facts.push(TimelineFact {
                    fact,
                    shared_from: Some(&other_person.name),
                });
            }
        }
    }

    // Sort by date
    all_facts.sort_by(|a, b| {
        let date_a = ChronicleDate::parse(&a.fact.date).ok();
        let date_b = ChronicleDate::parse(&b.fact.date).ok();
        date_a.cmp(&date_b)
    });

    all_facts
}

/// Build a location from optional components.
pub fn build_location(
    country: Option<String>,
    city: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
) -> Result<Option<Location>, AddFactError> {
    if let Some(country_name) = country {
        let mut loc = Location::new(country_name);
        if let Some(city_name) = city {
            loc = loc.with_city(city_name);
        }
        if let (Some(lat_val), Some(lon_val)) = (lat, lon) {
            let coords = Coordinates::new(lat_val, lon_val);
            if !coords.is_valid() {
                return Err(AddFactError {
                    message: format!(
                        "Invalid GPS coordinates (lat: {}, lon: {}). Valid ranges: lat -90..90, lon -180..180",
                        lat_val, lon_val
                    ),
                });
            }
            loc = loc.with_coordinates(coords);
        }
        Ok(Some(loc))
    } else {
        Ok(None)
    }
}

/// Parse attachment URLs and build attachment objects.
pub fn build_attachments(
    urls: Option<Vec<String>>,
    content_type: Option<String>,
    title: Option<String>,
) -> Result<Vec<Attachment>, AddFactError> {
    let Some(url_strings) = urls else {
        return Ok(Vec::new());
    };

    let mut attachments = Vec::new();
    for url_str in url_strings {
        let url = Url::parse(&url_str).map_err(|_| AddFactError {
            message: format!(
                "Invalid URL '{}'. URLs must include a scheme (e.g., file://, https://, s3://)",
                url_str
            ),
        })?;

        let mut attachment = Attachment::new(url);
        if let Some(ref ct) = content_type {
            if !is_valid_mime_type(ct) {
                return Err(AddFactError {
                    message: format!(
                        "Invalid MIME type '{}'. Expected format: type/subtype (e.g., image/jpeg)",
                        ct
                    ),
                });
            }
            attachment = attachment.with_content_type(ct);
        }
        if let Some(ref t) = title {
            attachment = attachment.with_title(t);
        }
        attachments.push(attachment);
    }

    Ok(attachments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Category, Person};

    fn create_test_chronicle() -> Chronicle {
        let mut chronicle = Chronicle::new("1.0");
        chronicle.categories.push(Category::new("family", "Family"));
        chronicle.categories.push(Category::new("travel", "Travel"));

        let mut alice = Person::new("alice", "Alice Smith");
        alice.facts.push(Fact::new(
            "uuid-1",
            "2020-01-01",
            "family",
            "Test event",
        ));
        chronicle.persons.push(alice);

        let bob = Person::new("bob", "Bob Johnson");
        chronicle.persons.push(bob);

        chronicle
    }

    #[test]
    fn test_add_fact_basic() {
        let mut chronicle = create_test_chronicle();

        let options = AddFactOptions {
            date: "2021-06-15".to_string(),
            category: "family".to_string(),
            text: "New event".to_string(),
            ..Default::default()
        };

        let result = add_fact(&mut chronicle, "alice", options).unwrap();

        assert_eq!(result.facts_added.len(), 1);
        assert_eq!(chronicle.find_person("alice").unwrap().facts.len(), 2);
    }

    #[test]
    fn test_add_fact_with_propagate() {
        let mut chronicle = create_test_chronicle();

        let options = AddFactOptions {
            date: "2021-06-15".to_string(),
            category: "family".to_string(),
            text: "Shared event".to_string(),
            with: Some(vec!["bob".to_string()]),
            propagate: true,
            ..Default::default()
        };

        let result = add_fact(&mut chronicle, "alice", options).unwrap();

        assert_eq!(result.facts_added.len(), 2);
        assert_eq!(chronicle.find_person("alice").unwrap().facts.len(), 2);
        assert_eq!(chronicle.find_person("bob").unwrap().facts.len(), 1);
    }

    #[test]
    fn test_add_fact_invalid_person() {
        let mut chronicle = create_test_chronicle();

        let options = AddFactOptions {
            date: "2021-06-15".to_string(),
            category: "family".to_string(),
            text: "Test".to_string(),
            ..Default::default()
        };

        let result = add_fact(&mut chronicle, "unknown", options);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_fact_invalid_category() {
        let mut chronicle = create_test_chronicle();

        let options = AddFactOptions {
            date: "2021-06-15".to_string(),
            category: "unknown".to_string(),
            text: "Test".to_string(),
            ..Default::default()
        };

        let result = add_fact(&mut chronicle, "alice", options);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_fact_with_location() {
        let mut chronicle = create_test_chronicle();

        let options = AddFactOptions {
            date: "2021-06-15".to_string(),
            category: "travel".to_string(),
            text: "Trip".to_string(),
            location: Some(
                Location::new("France")
                    .with_city("Paris")
                    .with_coordinates(Coordinates::new(48.8566, 2.3522)),
            ),
            ..Default::default()
        };

        let result = add_fact(&mut chronicle, "alice", options).unwrap();
        assert_eq!(result.facts_added.len(), 1);

        let fact = &chronicle.find_person("alice").unwrap().facts[1];
        assert!(fact.location.is_some());
        assert_eq!(fact.location.as_ref().unwrap().country, "France");
    }

    #[test]
    fn test_collect_timeline_facts() {
        let mut chronicle = create_test_chronicle();

        // Add a fact from bob with alice
        let bob = chronicle.find_person_mut("bob").unwrap();
        bob.facts.push(
            Fact::new("uuid-2", "2020-06-15", "family", "Shared from Bob")
                .with_persons(vec!["alice".to_string()]),
        );

        let filter = FactFilter::new();
        let facts = collect_timeline_facts(&chronicle, "alice", &filter, true);

        assert_eq!(facts.len(), 2);
        // One shared fact from Bob
        assert!(facts.iter().any(|f| f.shared_from.is_some()));
    }

    #[test]
    fn test_build_location() {
        let loc = build_location(
            Some("France".to_string()),
            Some("Paris".to_string()),
            Some(48.8566),
            Some(2.3522),
        )
        .unwrap();

        assert!(loc.is_some());
        let loc = loc.unwrap();
        assert_eq!(loc.country, "France");
        assert_eq!(loc.city, Some("Paris".to_string()));
        assert!(loc.coordinates.is_some());
    }

    #[test]
    fn test_build_location_invalid_coords() {
        let result = build_location(
            Some("Test".to_string()),
            None,
            Some(100.0), // Invalid
            Some(0.0),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_build_attachments() {
        let attachments = build_attachments(
            Some(vec!["https://example.com/photo.jpg".to_string()]),
            Some("image/jpeg".to_string()),
            Some("My Photo".to_string()),
        )
        .unwrap();

        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].content_type, Some("image/jpeg".to_string()));
        assert_eq!(attachments[0].title, Some("My Photo".to_string()));
    }
}
