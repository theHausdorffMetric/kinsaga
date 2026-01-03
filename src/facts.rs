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
        if let Some(ref coords) = location.coordinates
            && !coords.is_valid()
        {
            return Err(AddFactError {
                message: format!(
                    "Invalid GPS coordinates (lat: {}, lon: {}). Valid ranges: lat -90..90, lon -180..180",
                    coords.lat, coords.lon
                ),
            });
        }
    }

    // Validate attachments
    for attachment in &options.attachments {
        if let Some(ref content_type) = attachment.content_type
            && !is_valid_mime_type(content_type)
        {
            return Err(AddFactError {
                message: format!(
                    "Invalid MIME type '{}'. Expected format: type/subtype (e.g., image/jpeg)",
                    content_type
                ),
            });
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
    if options.propagate
        && let Some(ref with_ids) = options.with
    {
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
    place: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
) -> Result<Option<Location>, AddFactError> {
    if let Some(country_name) = country {
        let mut loc = Location::new(country_name);
        if let Some(place_name) = place {
            loc = loc.with_place(place_name);
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

// ============================================================================
// Edit Fact
// ============================================================================

/// How to update the 'with' field.
#[derive(Debug, Clone)]
pub enum WithUpdate {
    /// Replace with new list of person IDs
    Replace(Vec<String>),
    /// Clear all 'with' references
    Clear,
}

/// How to update the location field.
#[derive(Debug, Clone)]
pub enum LocationUpdate {
    /// Set or replace the location
    Set(Location),
    /// Remove the location entirely
    Clear,
}

/// How to update attachments.
#[derive(Debug, Clone)]
pub enum AttachmentUpdate {
    /// Add new attachments
    Add(Vec<Attachment>),
    /// Remove attachments by URL
    Remove(Vec<String>),
    /// Clear all attachments
    Clear,
}

/// Options for editing an existing fact.
#[derive(Debug, Clone, Default)]
pub struct EditFactOptions {
    /// New date (if Some, replaces existing)
    pub date: Option<String>,
    /// New category ID (if Some, replaces existing)
    pub category: Option<String>,
    /// New text (if Some, replaces existing)
    pub text: Option<String>,
    /// Update to 'with' field
    pub with: Option<WithUpdate>,
    /// Update to location
    pub location: Option<LocationUpdate>,
    /// Update to attachments
    pub attachments: Option<AttachmentUpdate>,
}

/// Result of editing a fact.
#[derive(Debug, Clone)]
pub struct EditFactResult {
    /// Person ID who owns the fact
    pub person_id: String,
    /// Person's display name
    pub person_name: String,
    /// The edited fact's ID
    pub fact_id: String,
}

/// Find a fact by UUID across all persons.
/// Returns (person_index, fact_index) if found.
fn find_fact_by_id(chronicle: &Chronicle, fact_id: &str) -> Option<(usize, usize)> {
    for (person_idx, person) in chronicle.persons.iter().enumerate() {
        for (fact_idx, fact) in person.facts.iter().enumerate() {
            if fact.id == fact_id {
                return Some((person_idx, fact_idx));
            }
        }
    }
    None
}

/// Edit an existing fact by UUID.
///
/// Finds the fact across all persons and applies the specified updates.
/// Only fields with Some values are modified.
pub fn edit_fact(
    chronicle: &mut Chronicle,
    fact_id: &str,
    options: EditFactOptions,
) -> Result<EditFactResult, AddFactError> {
    // Find the fact
    let (person_idx, fact_idx) = find_fact_by_id(chronicle, fact_id).ok_or_else(|| {
        AddFactError {
            message: format!("Fact with UUID '{}' not found", fact_id),
        }
    })?;

    // Validate new date if provided
    if let Some(ref date) = options.date
        && let Err(e) = ChronicleDate::parse(date)
    {
        return Err(AddFactError {
            message: format!("Invalid date format '{}': {}", date, e),
        });
    }

    // Validate new category if provided
    if let Some(ref category) = options.category
        && chronicle.find_category(category).is_none()
    {
        return Err(AddFactError {
            message: format!(
                "Category '{}' not found. Available categories: {}",
                category,
                chronicle
                    .categories
                    .iter()
                    .map(|c| c.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }

    // Validate 'with' references if provided
    if let Some(WithUpdate::Replace(ref with_ids)) = options.with {
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

    // Validate location if provided
    if let Some(LocationUpdate::Set(ref location)) = options.location {
        if location.country.trim().is_empty() {
            return Err(AddFactError {
                message: "Country cannot be empty".to_string(),
            });
        }
        if let Some(ref coords) = location.coordinates
            && !coords.is_valid()
        {
            return Err(AddFactError {
                message: format!(
                    "Invalid GPS coordinates (lat: {}, lon: {}). Valid ranges: lat -90..90, lon -180..180",
                    coords.lat, coords.lon
                ),
            });
        }
    }

    // Validate attachments if adding
    if let Some(AttachmentUpdate::Add(ref attachments)) = options.attachments {
        for attachment in attachments {
            if let Some(ref content_type) = attachment.content_type
                && !is_valid_mime_type(content_type)
            {
                return Err(AddFactError {
                    message: format!(
                        "Invalid MIME type '{}'. Expected format: type/subtype (e.g., image/jpeg)",
                        content_type
                    ),
                });
            }
        }
    }

    // Get person info for result
    let person_id = chronicle.persons[person_idx].id.clone();
    let person_name = chronicle.persons[person_idx].name.clone();

    // Apply updates
    let fact = &mut chronicle.persons[person_idx].facts[fact_idx];

    if let Some(date) = options.date {
        fact.date = date;
    }

    if let Some(category) = options.category {
        fact.category = category;
    }

    if let Some(text) = options.text {
        fact.text = text;
    }

    match options.with {
        Some(WithUpdate::Replace(with_ids)) => {
            fact.with = if with_ids.is_empty() {
                None
            } else {
                Some(with_ids)
            };
        }
        Some(WithUpdate::Clear) => {
            fact.with = None;
        }
        None => {}
    }

    match options.location {
        Some(LocationUpdate::Set(location)) => {
            fact.location = Some(location);
        }
        Some(LocationUpdate::Clear) => {
            fact.location = None;
        }
        None => {}
    }

    match options.attachments {
        Some(AttachmentUpdate::Add(new_attachments)) => {
            fact.attachments.extend(new_attachments);
        }
        Some(AttachmentUpdate::Remove(urls_to_remove)) => {
            fact.attachments
                .retain(|a| !urls_to_remove.contains(&a.url.to_string()));
        }
        Some(AttachmentUpdate::Clear) => {
            fact.attachments.clear();
        }
        None => {}
    }

    Ok(EditFactResult {
        person_id,
        person_name,
        fact_id: fact_id.to_string(),
    })
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
                    .with_place("Eiffel Tower, Paris")
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
        assert_eq!(loc.place, Some("Paris".to_string()));
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

    // ========================================================================
    // Edit Fact Tests
    // ========================================================================

    #[test]
    fn test_edit_fact_date() {
        let mut chronicle = create_test_chronicle();

        let result = edit_fact(
            &mut chronicle,
            "uuid-1",
            EditFactOptions {
                date: Some("2021-06-15".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.person_id, "alice");
        let fact = &chronicle.find_person("alice").unwrap().facts[0];
        assert_eq!(fact.date, "2021-06-15");
        assert_eq!(fact.text, "Test event"); // unchanged
    }

    #[test]
    fn test_edit_fact_text() {
        let mut chronicle = create_test_chronicle();

        edit_fact(
            &mut chronicle,
            "uuid-1",
            EditFactOptions {
                text: Some("Updated text".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let fact = &chronicle.find_person("alice").unwrap().facts[0];
        assert_eq!(fact.text, "Updated text");
        assert_eq!(fact.date, "2020-01-01"); // unchanged
    }

    #[test]
    fn test_edit_fact_category() {
        let mut chronicle = create_test_chronicle();

        edit_fact(
            &mut chronicle,
            "uuid-1",
            EditFactOptions {
                category: Some("travel".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let fact = &chronicle.find_person("alice").unwrap().facts[0];
        assert_eq!(fact.category, "travel");
    }

    #[test]
    fn test_edit_fact_location_set() {
        let mut chronicle = create_test_chronicle();

        edit_fact(
            &mut chronicle,
            "uuid-1",
            EditFactOptions {
                location: Some(LocationUpdate::Set(
                    Location::new("France").with_place("Paris"),
                )),
                ..Default::default()
            },
        )
        .unwrap();

        let fact = &chronicle.find_person("alice").unwrap().facts[0];
        assert!(fact.location.is_some());
        assert_eq!(fact.location.as_ref().unwrap().country, "France");
    }

    #[test]
    fn test_edit_fact_location_clear() {
        let mut chronicle = create_test_chronicle();

        // First set a location
        edit_fact(
            &mut chronicle,
            "uuid-1",
            EditFactOptions {
                location: Some(LocationUpdate::Set(Location::new("France"))),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(chronicle.find_person("alice").unwrap().facts[0]
            .location
            .is_some());

        // Then clear it
        edit_fact(
            &mut chronicle,
            "uuid-1",
            EditFactOptions {
                location: Some(LocationUpdate::Clear),
                ..Default::default()
            },
        )
        .unwrap();

        let fact = &chronicle.find_person("alice").unwrap().facts[0];
        assert!(fact.location.is_none());
    }

    #[test]
    fn test_edit_fact_with_replace() {
        let mut chronicle = create_test_chronicle();

        edit_fact(
            &mut chronicle,
            "uuid-1",
            EditFactOptions {
                with: Some(WithUpdate::Replace(vec!["bob".to_string()])),
                ..Default::default()
            },
        )
        .unwrap();

        let fact = &chronicle.find_person("alice").unwrap().facts[0];
        assert_eq!(fact.with, Some(vec!["bob".to_string()]));
    }

    #[test]
    fn test_edit_fact_with_clear() {
        let mut chronicle = create_test_chronicle();

        // First set a 'with'
        edit_fact(
            &mut chronicle,
            "uuid-1",
            EditFactOptions {
                with: Some(WithUpdate::Replace(vec!["bob".to_string()])),
                ..Default::default()
            },
        )
        .unwrap();

        // Then clear it
        edit_fact(
            &mut chronicle,
            "uuid-1",
            EditFactOptions {
                with: Some(WithUpdate::Clear),
                ..Default::default()
            },
        )
        .unwrap();

        let fact = &chronicle.find_person("alice").unwrap().facts[0];
        assert!(fact.with.is_none());
    }

    #[test]
    fn test_edit_fact_not_found() {
        let mut chronicle = create_test_chronicle();

        let result = edit_fact(
            &mut chronicle,
            "nonexistent-uuid",
            EditFactOptions {
                text: Some("New text".to_string()),
                ..Default::default()
            },
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("not found"));
    }

    #[test]
    fn test_edit_fact_invalid_category() {
        let mut chronicle = create_test_chronicle();

        let result = edit_fact(
            &mut chronicle,
            "uuid-1",
            EditFactOptions {
                category: Some("nonexistent".to_string()),
                ..Default::default()
            },
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("not found"));
    }

    #[test]
    fn test_edit_fact_multiple_fields() {
        let mut chronicle = create_test_chronicle();

        edit_fact(
            &mut chronicle,
            "uuid-1",
            EditFactOptions {
                date: Some("2022-01-01".to_string()),
                text: Some("Completely updated".to_string()),
                category: Some("travel".to_string()),
                location: Some(LocationUpdate::Set(
                    Location::new("Japan").with_place("Tokyo"),
                )),
                ..Default::default()
            },
        )
        .unwrap();

        let fact = &chronicle.find_person("alice").unwrap().facts[0];
        assert_eq!(fact.date, "2022-01-01");
        assert_eq!(fact.text, "Completely updated");
        assert_eq!(fact.category, "travel");
        assert_eq!(fact.location.as_ref().unwrap().country, "Japan");
    }

    #[test]
    fn test_edit_fact_attachments_add() {
        let mut chronicle = create_test_chronicle();
        let url = Url::parse("https://example.com/photo.jpg").unwrap();

        edit_fact(
            &mut chronicle,
            "uuid-1",
            EditFactOptions {
                attachments: Some(AttachmentUpdate::Add(vec![Attachment::new(url)])),
                ..Default::default()
            },
        )
        .unwrap();

        let fact = &chronicle.find_person("alice").unwrap().facts[0];
        assert_eq!(fact.attachments.len(), 1);
    }

    #[test]
    fn test_edit_fact_attachments_remove() {
        let mut chronicle = create_test_chronicle();
        let url = Url::parse("https://example.com/photo.jpg").unwrap();

        // First add an attachment
        edit_fact(
            &mut chronicle,
            "uuid-1",
            EditFactOptions {
                attachments: Some(AttachmentUpdate::Add(vec![Attachment::new(url)])),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            chronicle.find_person("alice").unwrap().facts[0]
                .attachments
                .len(),
            1
        );

        // Then remove it
        edit_fact(
            &mut chronicle,
            "uuid-1",
            EditFactOptions {
                attachments: Some(AttachmentUpdate::Remove(vec![
                    "https://example.com/photo.jpg".to_string(),
                ])),
                ..Default::default()
            },
        )
        .unwrap();

        let fact = &chronicle.find_person("alice").unwrap().facts[0];
        assert_eq!(fact.attachments.len(), 0);
    }

    #[test]
    fn test_edit_fact_attachments_clear() {
        let mut chronicle = create_test_chronicle();
        let url1 = Url::parse("https://example.com/photo1.jpg").unwrap();
        let url2 = Url::parse("https://example.com/photo2.jpg").unwrap();

        // Add two attachments
        edit_fact(
            &mut chronicle,
            "uuid-1",
            EditFactOptions {
                attachments: Some(AttachmentUpdate::Add(vec![
                    Attachment::new(url1),
                    Attachment::new(url2),
                ])),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            chronicle.find_person("alice").unwrap().facts[0]
                .attachments
                .len(),
            2
        );

        // Clear all
        edit_fact(
            &mut chronicle,
            "uuid-1",
            EditFactOptions {
                attachments: Some(AttachmentUpdate::Clear),
                ..Default::default()
            },
        )
        .unwrap();

        let fact = &chronicle.find_person("alice").unwrap().facts[0];
        assert_eq!(fact.attachments.len(), 0);
    }
}
