//! Merge operations for chronicles.

use crate::format::truncate_text;
use crate::{Chronicle, Fact};
use std::collections::HashSet;
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
#[derive(Debug, Clone)]
pub struct MergeError {
    pub message: String,
}

impl std::fmt::Display for MergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for MergeError {}

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

    // Build lookup maps for target
    let target_category_ids: HashSet<String> =
        target.categories.iter().map(|c| c.id.clone()).collect();
    let target_person_ids: HashSet<String> =
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
                        return Err(MergeError {
                            message: format!(
                                "Category conflict: '{}' exists with different values",
                                source_cat.id
                            ),
                        });
                    }
                }
            } else {
                stats.categories_identical += 1;
            }
        } else {
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
                    }
                    ConflictStrategy::Overwrite => {
                        if let Some(p) = target.find_person_mut(&source_person.id) {
                            p.name = source_person.name.clone();
                        }
                    }
                    ConflictStrategy::Fail => {
                        return Err(MergeError {
                            message: format!(
                                "Person conflict: '{}' has different name ('{}' vs '{}')",
                                source_person.id, target_name, source_person.name
                            ),
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

                // Create fact with potentially new UUID
                let mut new_fact = Fact::new(
                    &new_uuid,
                    &source_fact.date,
                    &source_fact.category,
                    &source_fact.text,
                );
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
