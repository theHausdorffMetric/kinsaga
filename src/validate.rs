//! Validation logic for chronicles.

use crate::format::truncate_text;
use crate::{Chronicle, ChronicleDate};
use std::collections::HashSet;
use uuid::Uuid;

/// A validation issue found in a chronicle.
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    /// Person ID where the issue was found
    pub person_id: String,
    /// Fact ID (if applicable)
    pub fact_id: Option<String>,
    /// Human-readable description of the issue
    pub message: String,
    /// Issue type for programmatic handling
    pub issue_type: IssueType,
}

/// Type of validation issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueType {
    EmptyUuid,
    InvalidUuid,
    DuplicateUuid,
    DuplicatePersonId,
    DuplicateCategoryId,
    UnknownCategory,
    InvalidDate,
    UnknownPersonRef,
    EmptyCountry,
    InvalidCoordinates,
    InvalidMimeType,
    InvalidUrl,
    InvalidId,
}

impl IssueType {
    /// Returns whether this issue type is an error (vs warning).
    pub fn is_error(&self) -> bool {
        matches!(
            self,
            IssueType::DuplicateUuid | IssueType::DuplicatePersonId | IssueType::DuplicateCategoryId
        )
    }
}

/// Result of validating a chronicle.
#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    /// Warning-level issues
    pub warnings: Vec<ValidationIssue>,
    /// Error-level issues
    pub errors: Vec<ValidationIssue>,
    /// Facts that need UUID correction (person_id, fact_id)
    pub needs_correction: Vec<(String, String)>,
    /// Total number of persons
    pub person_count: usize,
    /// Total number of facts
    pub fact_count: usize,
}

impl ValidationResult {
    /// Returns true if there are no errors or warnings.
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty() && self.warnings.is_empty()
    }

    /// Returns true if there are no errors (warnings are acceptable).
    pub fn has_no_errors(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Validate a chronicle and return all issues found.
pub fn validate_chronicle(chronicle: &Chronicle) -> ValidationResult {
    let mut result = ValidationResult::default();
    let mut seen_uuids: HashSet<String> = HashSet::new();

    result.person_count = chronicle.persons.len();
    result.fact_count = chronicle.persons.iter().map(|p| p.facts.len()).sum();

    // Build category ID set for reference checking; duplicate category IDs
    // are an error (find_category silently returns the first match)
    let mut category_ids: HashSet<&str> = HashSet::new();
    for category in &chronicle.categories {
        if !category_ids.insert(category.id.as_str()) {
            result.errors.push(ValidationIssue {
                person_id: String::new(),
                fact_id: None,
                message: format!("Duplicate category ID '{}'", category.id),
                issue_type: IssueType::DuplicateCategoryId,
            });
        }
        if !is_valid_id(&category.id) {
            result.warnings.push(ValidationIssue {
                person_id: String::new(),
                fact_id: None,
                message: format!(
                    "Category ID '{}' doesn't match the schema pattern ^[a-z][a-z0-9_-]*$",
                    category.id
                ),
                issue_type: IssueType::InvalidId,
            });
        }
    }

    // Build person ID set for reference checking; duplicate person IDs are
    // an error (find_person silently returns the first match)
    let mut person_ids: HashSet<&str> = HashSet::new();
    for person in &chronicle.persons {
        if !person_ids.insert(person.id.as_str()) {
            result.errors.push(ValidationIssue {
                person_id: person.id.clone(),
                fact_id: None,
                message: format!("Duplicate person ID '{}'", person.id),
                issue_type: IssueType::DuplicatePersonId,
            });
        }
        if !is_valid_id(&person.id) {
            result.warnings.push(ValidationIssue {
                person_id: person.id.clone(),
                fact_id: None,
                message: format!(
                    "Person ID '{}' doesn't match the schema pattern ^[a-z][a-z0-9_-]*$",
                    person.id
                ),
                issue_type: IssueType::InvalidId,
            });
        }
    }

    for person in &chronicle.persons {
        for fact in &person.facts {
            // UUID validation
            if fact.id.is_empty() {
                result.warnings.push(ValidationIssue {
                    person_id: person.id.clone(),
                    fact_id: Some(fact.id.clone()),
                    message: format!("Empty UUID for fact '{}'", truncate_text(&fact.text, 30)),
                    issue_type: IssueType::EmptyUuid,
                });
                result.needs_correction.push((person.id.clone(), fact.id.clone()));
            } else if Uuid::parse_str(&fact.id).is_err() {
                result.warnings.push(ValidationIssue {
                    person_id: person.id.clone(),
                    fact_id: Some(fact.id.clone()),
                    message: format!("Invalid UUID format '{}' for fact '{}'", fact.id, truncate_text(&fact.text, 30)),
                    issue_type: IssueType::InvalidUuid,
                });
                result.needs_correction.push((person.id.clone(), fact.id.clone()));
            } else if seen_uuids.contains(&fact.id) {
                result.errors.push(ValidationIssue {
                    person_id: person.id.clone(),
                    fact_id: Some(fact.id.clone()),
                    message: format!("Duplicate UUID '{}' for fact '{}'", fact.id, truncate_text(&fact.text, 30)),
                    issue_type: IssueType::DuplicateUuid,
                });
                result.needs_correction.push((person.id.clone(), fact.id.clone()));
            } else {
                seen_uuids.insert(fact.id.clone());
            }

            // Category validation
            if !category_ids.contains(fact.category.as_str()) {
                result.warnings.push(ValidationIssue {
                    person_id: person.id.clone(),
                    fact_id: Some(fact.id.clone()),
                    message: format!("Unknown category '{}'", fact.category),
                    issue_type: IssueType::UnknownCategory,
                });
            }

            // Date validation
            if let Err(e) = ChronicleDate::parse(&fact.date) {
                result.warnings.push(ValidationIssue {
                    person_id: person.id.clone(),
                    fact_id: Some(fact.id.clone()),
                    message: format!("Invalid date '{}' - {}", fact.date, e),
                    issue_type: IssueType::InvalidDate,
                });
            }

            // 'with' references validation
            if let Some(ref with) = fact.with {
                for person_ref in with {
                    if !person_ids.contains(person_ref.as_str()) {
                        result.warnings.push(ValidationIssue {
                            person_id: person.id.clone(),
                            fact_id: Some(fact.id.clone()),
                            message: format!("Unknown person reference '{}'", person_ref),
                            issue_type: IssueType::UnknownPersonRef,
                        });
                    }
                }
            }

            // Location validation
            if let Some(ref location) = fact.location {
                if location.country.trim().is_empty() {
                    result.warnings.push(ValidationIssue {
                        person_id: person.id.clone(),
                        fact_id: Some(fact.id.clone()),
                        message: "Empty country in location".to_string(),
                        issue_type: IssueType::EmptyCountry,
                    });
                }

                if let Some(ref coords) = location.coordinates
                    && !coords.is_valid()
                {
                    result.warnings.push(ValidationIssue {
                        person_id: person.id.clone(),
                        fact_id: Some(fact.id.clone()),
                        message: format!(
                            "Invalid GPS coordinates (lat: {}, lon: {}). Valid ranges: lat -90..90, lon -180..180",
                            coords.lat, coords.lon
                        ),
                        issue_type: IssueType::InvalidCoordinates,
                    });
                }
            }

            // Attachments validation
            for (i, attachment) in fact.attachments.iter().enumerate() {
                if url::Url::parse(&attachment.url).is_err() {
                    result.warnings.push(ValidationIssue {
                        person_id: person.id.clone(),
                        fact_id: Some(fact.id.clone()),
                        message: format!(
                            "Attachment {}: invalid URL '{}' (must include a scheme, e.g. file://, https://)",
                            i + 1,
                            attachment.url
                        ),
                        issue_type: IssueType::InvalidUrl,
                    });
                }
                if let Some(ref content_type) = attachment.content_type
                    && !is_valid_mime_type(content_type)
                {
                    result.warnings.push(ValidationIssue {
                        person_id: person.id.clone(),
                        fact_id: Some(fact.id.clone()),
                        message: format!(
                            "Attachment {}: invalid MIME type '{}' (expected format: type/subtype)",
                            i + 1, content_type
                        ),
                        issue_type: IssueType::InvalidMimeType,
                    });
                }
            }
        }
    }

    result
}

/// Generate new UUIDs for facts with empty, invalid, or duplicate UUIDs.
///
/// Facts are processed in document order. For duplicate UUIDs, the first
/// occurrence keeps its original ID and only later occurrences are
/// regenerated, so stable references to the original fact survive.
pub fn correct_uuids(mut chronicle: Chronicle, corrections: &[(String, String)]) -> Chronicle {
    let mut seen: HashSet<String> = HashSet::new();

    for person in &mut chronicle.persons {
        for fact in &mut person.facts {
            let flagged = corrections
                .iter()
                .any(|(pid, fid)| pid == &person.id && fid == &fact.id);
            // A flagged ID is kept only if it is a valid UUID seen here first
            // (the first occurrence of a duplicate pair keeps its ID).
            let keeps_id = Uuid::parse_str(&fact.id).is_ok() && !seen.contains(&fact.id);
            if flagged && !keeps_id {
                fact.id = Uuid::new_v4().to_string();
            }
            seen.insert(fact.id.clone());
        }
    }

    chronicle
}

/// Check if an ID matches the schema pattern `^[a-z][a-z0-9_-]*$`
/// (used for person and category IDs).
pub fn is_valid_id(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Check if a string is a valid MIME type (basic format: type/subtype).
pub fn is_valid_mime_type(s: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Category, Fact, Person};

    #[test]
    fn test_is_valid_id() {
        assert!(is_valid_id("alice"));
        assert!(is_valid_id("john-doe"));
        assert!(is_valid_id("cat_2"));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("Alice"));
        assert!(!is_valid_id("2cool"));
        assert!(!is_valid_id("has space"));
        assert!(!is_valid_id("umläut"));
    }

    #[test]
    fn test_validate_invalid_id_pattern_warns() {
        let mut chronicle = Chronicle::new("1.0");
        chronicle.categories.push(Category::new("Family", "Family"));
        chronicle.persons.push(Person::new("Alice Smith", "Alice"));

        let result = validate_chronicle(&chronicle);
        let id_warnings: Vec<_> = result
            .warnings
            .iter()
            .filter(|i| i.issue_type == IssueType::InvalidId)
            .collect();
        assert_eq!(id_warnings.len(), 2);
    }

    #[test]
    fn test_is_valid_mime_type() {
        assert!(is_valid_mime_type("image/jpeg"));
        assert!(is_valid_mime_type("application/pdf"));
        assert!(is_valid_mime_type("text/plain"));
        assert!(is_valid_mime_type("application/vnd.ms-excel"));
        assert!(is_valid_mime_type("image/svg+xml"));

        assert!(!is_valid_mime_type("invalid"));
        assert!(!is_valid_mime_type(""));
        assert!(!is_valid_mime_type("/jpeg"));
        assert!(!is_valid_mime_type("image/"));
        assert!(!is_valid_mime_type("image/jpeg/extra"));
    }

    #[test]
    fn test_validate_empty_chronicle() {
        let chronicle = Chronicle::new("1.0");
        let result = validate_chronicle(&chronicle);
        assert!(result.is_valid());
        assert_eq!(result.person_count, 0);
        assert_eq!(result.fact_count, 0);
    }

    #[test]
    fn test_validate_valid_chronicle() {
        let mut chronicle = Chronicle::new("1.0");
        chronicle.categories.push(Category::new("family", "Family"));

        let mut person = Person::new("alice", "Alice");
        person.facts.push(Fact::new(
            "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
            "2020-01-01",
            "family",
            "Test event",
        ));
        chronicle.persons.push(person);

        let result = validate_chronicle(&chronicle);
        assert!(result.is_valid());
    }

    #[test]
    fn test_validate_invalid_uuid() {
        let mut chronicle = Chronicle::new("1.0");
        chronicle.categories.push(Category::new("family", "Family"));

        let mut person = Person::new("alice", "Alice");
        person.facts.push(Fact::new("not-a-uuid", "2020-01-01", "family", "Test"));
        chronicle.persons.push(person);

        let result = validate_chronicle(&chronicle);
        assert!(!result.is_valid());
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].issue_type, IssueType::InvalidUuid);
    }

    #[test]
    fn test_validate_duplicate_uuid() {
        let mut chronicle = Chronicle::new("1.0");
        chronicle.categories.push(Category::new("family", "Family"));

        let uuid = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let mut person = Person::new("alice", "Alice");
        person.facts.push(Fact::new(uuid, "2020-01-01", "family", "Event 1"));
        person.facts.push(Fact::new(uuid, "2020-01-02", "family", "Event 2"));
        chronicle.persons.push(person);

        let result = validate_chronicle(&chronicle);
        assert!(!result.has_no_errors());
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].issue_type, IssueType::DuplicateUuid);
    }

    #[test]
    fn test_chronicle_with_invalid_attachment_url_loads_and_warns() {
        // A malformed attachment URL must be a warning, not a load failure
        let json = r#"{
            "version": "1.0",
            "categories": [{ "id": "family", "label": "Family" }],
            "persons": [{
                "id": "alice", "name": "Alice",
                "facts": [{
                    "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
                    "date": "2020-01-01", "category": "family", "text": "Event",
                    "attachments": [{ "url": "not a url" }]
                }]
            }]
        }"#;

        let chronicle = crate::io::from_json(json).expect("must load despite bad URL");
        let result = validate_chronicle(&chronicle);
        assert!(result
            .warnings
            .iter()
            .any(|i| i.issue_type == IssueType::InvalidUrl));
    }

    #[test]
    fn test_validate_duplicate_person_and_category_ids() {
        let mut chronicle = Chronicle::new("1.0");
        chronicle.categories.push(Category::new("family", "Family"));
        chronicle.categories.push(Category::new("family", "Family 2"));
        chronicle.persons.push(Person::new("alice", "Alice"));
        chronicle.persons.push(Person::new("alice", "Alice 2"));

        let result = validate_chronicle(&chronicle);
        assert!(!result.has_no_errors());
        assert!(result
            .errors
            .iter()
            .any(|i| i.issue_type == IssueType::DuplicateCategoryId));
        assert!(result
            .errors
            .iter()
            .any(|i| i.issue_type == IssueType::DuplicatePersonId));
    }

    #[test]
    fn test_correct_uuids() {
        let mut chronicle = Chronicle::new("1.0");
        chronicle.categories.push(Category::new("family", "Family"));

        let mut person = Person::new("alice", "Alice");
        person.facts.push(Fact::new("invalid", "2020-01-01", "family", "Test"));
        chronicle.persons.push(person);

        let corrections = vec![("alice".to_string(), "invalid".to_string())];
        let fixed = correct_uuids(chronicle, &corrections);

        assert_ne!(fixed.persons[0].facts[0].id, "invalid");
        assert!(Uuid::parse_str(&fixed.persons[0].facts[0].id).is_ok());
    }

    #[test]
    fn test_correct_uuids_duplicate_keeps_first_occurrence() {
        let mut chronicle = Chronicle::new("1.0");
        chronicle.categories.push(Category::new("family", "Family"));

        let uuid = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let mut person = Person::new("alice", "Alice");
        person.facts.push(Fact::new(uuid, "2020-01-01", "family", "Event 1"));
        person.facts.push(Fact::new(uuid, "2020-01-02", "family", "Event 2"));
        chronicle.persons.push(person);

        let result = validate_chronicle(&chronicle);
        let fixed = correct_uuids(chronicle, &result.needs_correction);

        let facts = &fixed.persons[0].facts;
        assert_eq!(facts[0].id, uuid, "first occurrence keeps its UUID");
        assert_ne!(facts[1].id, uuid, "duplicate gets a new UUID");
        assert!(Uuid::parse_str(&facts[1].id).is_ok());
    }

    #[test]
    fn test_correct_uuids_cross_person_duplicate() {
        let mut chronicle = Chronicle::new("1.0");
        chronicle.categories.push(Category::new("family", "Family"));

        let uuid = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let mut alice = Person::new("alice", "Alice");
        alice.facts.push(Fact::new(uuid, "2020-01-01", "family", "Alice event"));
        chronicle.persons.push(alice);
        let mut bob = Person::new("bob", "Bob");
        bob.facts.push(Fact::new(uuid, "2020-01-02", "family", "Bob event"));
        chronicle.persons.push(bob);

        let result = validate_chronicle(&chronicle);
        let fixed = correct_uuids(chronicle, &result.needs_correction);

        assert_eq!(fixed.persons[0].facts[0].id, uuid, "Alice keeps her UUID");
        assert_ne!(fixed.persons[1].facts[0].id, uuid, "Bob's duplicate is regenerated");
    }

    #[test]
    fn test_correct_uuids_multiple_empty_ids() {
        let mut chronicle = Chronicle::new("1.0");
        chronicle.categories.push(Category::new("family", "Family"));

        let mut person = Person::new("alice", "Alice");
        person.facts.push(Fact::new("", "2020-01-01", "family", "Event 1"));
        person.facts.push(Fact::new("", "2020-01-02", "family", "Event 2"));
        chronicle.persons.push(person);

        let result = validate_chronicle(&chronicle);
        let fixed = correct_uuids(chronicle, &result.needs_correction);

        let facts = &fixed.persons[0].facts;
        assert!(Uuid::parse_str(&facts[0].id).is_ok());
        assert!(Uuid::parse_str(&facts[1].id).is_ok());
        assert_ne!(facts[0].id, facts[1].id, "each fact gets a distinct UUID");
    }
}
