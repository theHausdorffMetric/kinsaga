//! Merge operations for chronicles.

use crate::format::truncate_text;
use crate::{Chronicle, Fact};
use std::collections::HashSet;
use thiserror::Error;
use uuid::Uuid;

/// Strategy for handling conflicts during merge.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConflictStrategy {
    /// Skip conflicting items (keep target values)
    #[default]
    Skip,
    /// Overwrite target with source values
    Overwrite,
    /// Fail on any conflict
    Fail,
}

/// Strategy for handling duplicate facts during merge.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DuplicateStrategy {
    /// Skip duplicate facts
    #[default]
    Skip,
    /// Add duplicates anyway (with new UUIDs)
    Add,
}

/// Options for merge operation.
#[derive(Debug, Clone, Default)]
pub struct MergeOptions {
    /// How to handle category/person conflicts
    pub on_conflict: ConflictStrategy,
    /// How to handle duplicate facts
    pub duplicates: DuplicateStrategy,
    /// Whether to regenerate all UUIDs from source
    pub regenerate_uuids: bool,
}

/// Statistics from a merge operation.
#[derive(Debug, Clone, Default)]
pub struct MergeStats {
    pub categories_added: usize,
    pub categories_skipped: usize,
    pub categories_overwritten: usize,
    pub categories_identical: usize,
    pub persons_added: usize,
    pub persons_merged: usize,
    pub facts_added: usize,
    pub facts_skipped: usize,
}

/// A single merge event for logging/display.
#[derive(Debug, Clone)]
pub struct MergeEvent {
    pub event_type: MergeEventType,
    pub item_type: MergeItemType,
    pub id: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeEventType {
    Added,
    Skipped,
    Overwritten,
    Merged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeItemType {
    Category,
    Person,
}

/// Result of a merge operation.
#[derive(Debug, Clone)]
pub struct MergeResult {
    /// The merged chronicle
    pub chronicle: Chronicle,
    /// Statistics about what was merged
    pub stats: MergeStats,
    /// Events that occurred during merge (for logging)
    pub events: Vec<MergeEvent>,
    /// Warnings (e.g., unknown person references)
    pub warnings: Vec<String>,
}

/// Error that can occur during merge.
#[derive(Debug, Clone, Error)]
pub enum MergeError {
    #[error("Category conflict: '{id}' exists with different values")]
    CategoryConflict { id: String },

    #[error("Person conflict: '{id}' has different name ('{target_name}' vs '{source_name}')")]
    PersonNameConflict {
        id: String,
        target_name: String,
        source_name: String,
    },
}

/// Merge a source chronicle into a target chronicle.
pub fn merge_chronicles(
    mut target: Chronicle,
    source: &Chronicle,
    options: &MergeOptions,
) -> Result<MergeResult, MergeError> {
    let mut stats = MergeStats::default();
    let mut events = Vec::new();
    let mut warnings = Vec::new();

    // Collect existing UUIDs in target (for collision detection)
    let mut existing_uuids: HashSet<String> = HashSet::new();
    for person in &target.persons {
        for fact in &person.facts {
            existing_uuids.insert(fact.id.clone());
        }
    }

    // Build lookup maps for target. These are kept up to date as items are
    // added, so a malformed source that lists the same person or category
    // twice cannot create duplicate entries in the target.
    let mut target_category_ids: HashSet<String> =
        target.categories.iter().map(|c| c.id.clone()).collect();
    let mut target_person_ids: HashSet<String> =
        target.persons.iter().map(|p| p.id.clone()).collect();

    // === Merge Categories ===
    for source_cat in &source.categories {
        if target_category_ids.contains(&source_cat.id) {
            let target_cat = target.find_category(&source_cat.id).unwrap();
            let has_diff =
                target_cat.label != source_cat.label || target_cat.color != source_cat.color;

            if has_diff {
                match options.on_conflict {
                    ConflictStrategy::Skip => {
                        events.push(MergeEvent {
                            event_type: MergeEventType::Skipped,
                            item_type: MergeItemType::Category,
                            id: source_cat.id.clone(),
                            details: Some("conflict".to_string()),
                        });
                        stats.categories_skipped += 1;
                    }
                    ConflictStrategy::Overwrite => {
                        if let Some(cat) = target.categories.iter_mut().find(|c| c.id == source_cat.id) {
                            cat.label = source_cat.label.clone();
                            cat.color = source_cat.color.clone();
                        }
                        events.push(MergeEvent {
                            event_type: MergeEventType::Overwritten,
                            item_type: MergeItemType::Category,
                            id: source_cat.id.clone(),
                            details: None,
                        });
                        stats.categories_overwritten += 1;
                    }
                    ConflictStrategy::Fail => {
                        return Err(MergeError::CategoryConflict {
                            id: source_cat.id.clone(),
                        });
                    }
                }
            } else {
                stats.categories_identical += 1;
            }
        } else {
            target_category_ids.insert(source_cat.id.clone());
            target.categories.push(source_cat.clone());
            events.push(MergeEvent {
                event_type: MergeEventType::Added,
                item_type: MergeItemType::Category,
                id: source_cat.id.clone(),
                details: None,
            });
            stats.categories_added += 1;
        }
    }

    // === Merge Persons and Facts ===
    for source_person in &source.persons {
        if target_person_ids.contains(&source_person.id) {
            // Person exists - merge facts
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
                match options.on_conflict {
                    ConflictStrategy::Skip => {
                        // Keep target name, but still merge facts
                        events.push(MergeEvent {
                            event_type: MergeEventType::Skipped,
                            item_type: MergeItemType::Person,
                            id: source_person.id.clone(),
                            details: Some(format!(
                                "name conflict: kept '{}' over '{}'",
                                target_name, source_person.name
                            )),
                        });
                    }
                    ConflictStrategy::Overwrite => {
                        if let Some(p) = target.find_person_mut(&source_person.id) {
                            p.name = source_person.name.clone();
                        }
                        events.push(MergeEvent {
                            event_type: MergeEventType::Overwritten,
                            item_type: MergeItemType::Person,
                            id: source_person.id.clone(),
                            details: Some(format!(
                                "name overwritten: '{}' -> '{}'",
                                target_name, source_person.name
                            )),
                        });
                    }
                    ConflictStrategy::Fail => {
                        return Err(MergeError::PersonNameConflict {
                            id: source_person.id.clone(),
                            target_name,
                            source_name: source_person.name.clone(),
                        });
                    }
                }
            }

            let mut facts_added = 0;
            let mut facts_skipped = 0;
            let mut facts_to_add: Vec<Fact> = Vec::new();

            for source_fact in &source_person.facts {
                let fact_key = (
                    source_fact.date.clone(),
                    source_fact.category.clone(),
                    source_fact.text.clone(),
                );

                let is_duplicate = existing_facts.contains(&fact_key);

                if is_duplicate {
                    match options.duplicates {
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
                let new_uuid = if options.regenerate_uuids || existing_uuids.contains(&source_fact.id) {
                    let uuid = Uuid::new_v4().to_string();
                    existing_uuids.insert(uuid.clone());
                    uuid
                } else {
                    existing_uuids.insert(source_fact.id.clone());
                    source_fact.id.clone()
                };

                // Clone the fact wholesale so any future Fact fields
                // survive merges, then set the (possibly new) UUID
                let mut new_fact = source_fact.clone();
                new_fact.id = new_uuid;
                facts_to_add.push(new_fact);
                facts_added += 1;
                stats.facts_added += 1;
            }

            // Add all facts to target person
            if let Some(p) = target.find_person_mut(&source_person.id) {
                p.facts.extend(facts_to_add);
            }

            if facts_added > 0 || facts_skipped > 0 {
                events.push(MergeEvent {
                    event_type: MergeEventType::Merged,
                    item_type: MergeItemType::Person,
                    id: source_person.id.clone(),
                    details: Some(format!(
                        "{} facts added, {} skipped",
                        facts_added, facts_skipped
                    )),
                });
                stats.persons_merged += 1;
            }
        } else {
            // New person - add entirely
            let mut new_person = source_person.clone();

            // Handle UUIDs
            if options.regenerate_uuids {
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
            events.push(MergeEvent {
                event_type: MergeEventType::Added,
                item_type: MergeItemType::Person,
                id: source_person.id.clone(),
                details: Some(format!("{} facts", fact_count)),
            });

            target_person_ids.insert(source_person.id.clone());
            target.persons.push(new_person);
            stats.persons_added += 1;
            stats.facts_added += fact_count;
        }
    }

    // === Validate 'with' references ===
    let final_person_ids: HashSet<&str> = target.persons.iter().map(|p| p.id.as_str()).collect();
    for person in &target.persons {
        for fact in &person.facts {
            if let Some(ref with) = fact.with {
                for with_id in with {
                    if !final_person_ids.contains(with_id.as_str()) {
                        warnings.push(format!(
                            "Person '{}', fact '{}': references unknown person '{}'",
                            person.id,
                            truncate_text(&fact.text, 30),
                            with_id
                        ));
                    }
                }
            }
        }
    }

    Ok(MergeResult {
        chronicle: target,
        stats,
        events,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Category, Person};

    fn create_test_chronicle(id_suffix: &str) -> Chronicle {
        let mut chronicle = Chronicle::new("1.0");
        chronicle.categories.push(Category::new("family", "Family"));

        let mut person = Person::new(
            format!("person{}", id_suffix),
            format!("Person {}", id_suffix),
        );
        person.facts.push(Fact::new(
            format!("uuid-{}", id_suffix),
            "2020-01-01",
            "family",
            format!("Event {}", id_suffix),
        ));
        chronicle.persons.push(person);

        chronicle
    }

    #[test]
    fn test_merge_new_person() {
        let target = create_test_chronicle("1");
        let source = create_test_chronicle("2");

        let result = merge_chronicles(target, &source, &MergeOptions::default()).unwrap();

        assert_eq!(result.chronicle.persons.len(), 2);
        assert_eq!(result.stats.persons_added, 1);
        assert_eq!(result.stats.facts_added, 1);
    }

    #[test]
    fn test_merge_new_category() {
        let mut target = Chronicle::new("1.0");
        target.categories.push(Category::new("family", "Family"));

        let mut source = Chronicle::new("1.0");
        source.categories.push(Category::new("travel", "Travel"));

        let result = merge_chronicles(target, &source, &MergeOptions::default()).unwrap();

        assert_eq!(result.chronicle.categories.len(), 2);
        assert_eq!(result.stats.categories_added, 1);
    }

    #[test]
    fn test_merge_category_conflict_skip() {
        let mut target = Chronicle::new("1.0");
        target.categories.push(Category::new("family", "Family Target"));

        let mut source = Chronicle::new("1.0");
        source.categories.push(Category::new("family", "Family Source"));

        let result = merge_chronicles(target, &source, &MergeOptions::default()).unwrap();

        assert_eq!(result.chronicle.categories.len(), 1);
        assert_eq!(result.chronicle.categories[0].label, "Family Target");
        assert_eq!(result.stats.categories_skipped, 1);
    }

    #[test]
    fn test_merge_category_conflict_overwrite() {
        let mut target = Chronicle::new("1.0");
        target.categories.push(Category::new("family", "Family Target"));

        let mut source = Chronicle::new("1.0");
        source.categories.push(Category::new("family", "Family Source"));

        let options = MergeOptions {
            on_conflict: ConflictStrategy::Overwrite,
            ..Default::default()
        };
        let result = merge_chronicles(target, &source, &options).unwrap();

        assert_eq!(result.chronicle.categories[0].label, "Family Source");
        assert_eq!(result.stats.categories_overwritten, 1);
    }

    #[test]
    fn test_merge_category_conflict_fail() {
        let mut target = Chronicle::new("1.0");
        target.categories.push(Category::new("family", "Family Target"));

        let mut source = Chronicle::new("1.0");
        source.categories.push(Category::new("family", "Family Source"));

        let options = MergeOptions {
            on_conflict: ConflictStrategy::Fail,
            ..Default::default()
        };
        let result = merge_chronicles(target, &source, &options);

        assert!(result.is_err());
    }

    #[test]
    fn test_merge_duplicate_facts_skip() {
        let mut target = Chronicle::new("1.0");
        target.categories.push(Category::new("family", "Family"));
        let mut person = Person::new("alice", "Alice");
        person.facts.push(Fact::new("uuid-1", "2020-01-01", "family", "Same event"));
        target.persons.push(person);

        let mut source = Chronicle::new("1.0");
        source.categories.push(Category::new("family", "Family"));
        let mut person = Person::new("alice", "Alice");
        person.facts.push(Fact::new("uuid-2", "2020-01-01", "family", "Same event"));
        source.persons.push(person);

        let result = merge_chronicles(target, &source, &MergeOptions::default()).unwrap();

        assert_eq!(result.chronicle.persons[0].facts.len(), 1);
        assert_eq!(result.stats.facts_skipped, 1);
    }

    #[test]
    fn test_merge_source_with_duplicate_person_ids_creates_no_duplicates() {
        let target = Chronicle::new("1.0");

        // Malformed source listing the same person ID twice
        let mut source = Chronicle::new("1.0");
        source.categories.push(Category::new("family", "Family"));
        let mut p1 = Person::new("alice", "Alice");
        p1.facts.push(Fact::new("uuid-1", "2020-01-01", "family", "Event 1"));
        let mut p2 = Person::new("alice", "Alice");
        p2.facts.push(Fact::new("uuid-2", "2020-01-02", "family", "Event 2"));
        source.persons.push(p1);
        source.persons.push(p2);

        let result = merge_chronicles(target, &source, &MergeOptions::default()).unwrap();

        // The second occurrence must merge into the first, not duplicate it
        assert_eq!(result.chronicle.persons.len(), 1);
        assert_eq!(result.chronicle.persons[0].facts.len(), 2);
    }

    #[test]
    fn test_merge_preserves_all_fact_fields() {
        use crate::{Attachment, Location};
        use url::Url;

        // Target already contains the fact's UUID, forcing the clone+new-id path
        let mut target = Chronicle::new("1.0");
        target.categories.push(Category::new("travel", "Travel"));
        let mut alice = Person::new("alice", "Alice");
        alice.facts.push(Fact::new(
            "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
            "2019-01-01",
            "travel",
            "Existing event",
        ));
        target.persons.push(alice);

        let mut source = Chronicle::new("1.0");
        source.categories.push(Category::new("travel", "Travel"));
        let mut alice = Person::new("alice", "Alice");
        let url = Url::parse("https://example.com/photo.jpg").unwrap();
        alice.facts.push(
            Fact::new(
                "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
                "2020-06-15",
                "travel",
                "Rich event",
            )
            .with_persons(vec!["alice".to_string()])
            .with_location(Location::new("France").with_place("Paris"))
            .with_attachment(Attachment::new(url).with_title("Photo")),
        );
        source.persons.push(alice);

        let result = merge_chronicles(target, &source, &MergeOptions::default()).unwrap();

        let facts = &result.chronicle.persons[0].facts;
        assert_eq!(facts.len(), 2);
        let merged = &facts[1];
        assert_ne!(merged.id, "a1b2c3d4-e5f6-7890-abcd-ef1234567890");
        assert_eq!(merged.location.as_ref().unwrap().country, "France");
        assert_eq!(merged.attachments.len(), 1);
        assert_eq!(merged.with, Some(vec!["alice".to_string()]));
    }

    #[test]
    fn test_merge_name_conflict_skip_emits_event() {
        let mut target = Chronicle::new("1.0");
        target.persons.push(Person::new("alice", "Alice Target"));

        let mut source = Chronicle::new("1.0");
        source.persons.push(Person::new("alice", "Alice Source"));

        let result = merge_chronicles(target, &source, &MergeOptions::default()).unwrap();

        assert_eq!(result.chronicle.persons[0].name, "Alice Target");
        assert!(result.events.iter().any(|e| {
            e.event_type == MergeEventType::Skipped
                && e.item_type == MergeItemType::Person
                && e.id == "alice"
        }));
    }

    #[test]
    fn test_merge_regenerate_uuids() {
        let target = Chronicle::new("1.0");
        let source = create_test_chronicle("1");

        let options = MergeOptions {
            regenerate_uuids: true,
            ..Default::default()
        };
        let result = merge_chronicles(target, &source, &options).unwrap();

        assert_ne!(result.chronicle.persons[0].facts[0].id, "uuid-1");
    }
}
