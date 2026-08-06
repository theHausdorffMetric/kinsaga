//! Person management operations.

use crate::validate::is_valid_id;
use crate::{Chronicle, Person};
use thiserror::Error;

/// Error that can occur when managing persons.
#[derive(Debug, Error)]
pub enum PersonError {
    #[error("Person ID '{id}' already exists")]
    DuplicateId { id: String },

    #[error("Person '{id}' not found. Available persons: {}", .available.join(", "))]
    NotFound { id: String, available: Vec<String> },

    #[error("Invalid person ID '{id}': must match ^[a-z][a-z0-9_-]*$")]
    InvalidId { id: String },

    #[error("Person name cannot be empty")]
    EmptyName,

    #[error(
        "Person '{id}' is referenced in the 'with' field of {count} fact(s); \
         use --force to remove anyway and strip the references"
    )]
    Referenced { id: String, count: usize },
}

/// Add a new person with no facts.
pub fn add_person(chronicle: &mut Chronicle, id: &str, name: &str) -> Result<(), PersonError> {
    if !is_valid_id(id) {
        return Err(PersonError::InvalidId { id: id.to_string() });
    }
    if name.trim().is_empty() {
        return Err(PersonError::EmptyName);
    }
    if chronicle.find_person(id).is_some() {
        return Err(PersonError::DuplicateId { id: id.to_string() });
    }
    chronicle.persons.push(Person::new(id, name));
    Ok(())
}

/// Result of removing a person.
#[derive(Debug)]
pub struct RemovePersonResult {
    /// The removed person (with all their facts)
    pub person: Person,
    /// Number of 'with' references to this person stripped from other facts
    pub references_stripped: usize,
}

/// Remove a person and all their facts.
///
/// If other persons' facts reference the person in their 'with' field,
/// this fails unless `force` is set, in which case the references are
/// stripped from those facts.
pub fn remove_person(
    chronicle: &mut Chronicle,
    id: &str,
    force: bool,
) -> Result<RemovePersonResult, PersonError> {
    let Some(idx) = chronicle.persons.iter().position(|p| p.id == id) else {
        return Err(PersonError::NotFound {
            id: id.to_string(),
            available: chronicle.persons.iter().map(|p| p.id.clone()).collect(),
        });
    };

    // Count 'with' references to this person from other persons' facts
    let reference_count: usize = chronicle
        .persons
        .iter()
        .filter(|p| p.id != id)
        .flat_map(|p| &p.facts)
        .filter_map(|f| f.with.as_ref())
        .map(|w| w.iter().filter(|r| *r == id).count())
        .sum();

    if reference_count > 0 && !force {
        return Err(PersonError::Referenced {
            id: id.to_string(),
            count: reference_count,
        });
    }

    // Strip any remaining references, dropping 'with' lists that become
    // empty. Same scope as the count above: the removed person's own facts
    // are deleted wholesale below, so a (malformed) self-reference there
    // neither blocks removal nor inflates `references_stripped`.
    let mut references_stripped = 0;
    if reference_count > 0 {
        for person in &mut chronicle.persons {
            if person.id == id {
                continue;
            }
            for fact in &mut person.facts {
                if let Some(ref mut with) = fact.with {
                    let before = with.len();
                    with.retain(|r| r != id);
                    references_stripped += before - with.len();
                    if with.is_empty() {
                        fact.with = None;
                    }
                }
            }
        }
    }

    let person = chronicle.persons.remove(idx);
    Ok(RemovePersonResult {
        person,
        references_stripped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Category, Fact};

    fn create_test_chronicle() -> Chronicle {
        let mut chronicle = Chronicle::new("1.0");
        chronicle.categories.push(Category::new("family", "Family"));

        let mut alice = Person::new("alice", "Alice Smith");
        alice.facts.push(
            Fact::new("uuid-1", "2020-01-01", "family", "Shared event")
                .with_persons(vec!["bob".to_string()]),
        );
        chronicle.persons.push(alice);
        chronicle.persons.push(Person::new("bob", "Bob Johnson"));

        chronicle
    }

    #[test]
    fn test_add_person() {
        let mut chronicle = create_test_chronicle();
        add_person(&mut chronicle, "carol", "Carol White").unwrap();

        let carol = chronicle.find_person("carol").unwrap();
        assert_eq!(carol.name, "Carol White");
        assert!(carol.facts.is_empty());
    }

    #[test]
    fn test_add_person_duplicate_id_rejected() {
        let mut chronicle = create_test_chronicle();
        let result = add_person(&mut chronicle, "alice", "Another Alice");
        assert!(matches!(result, Err(PersonError::DuplicateId { .. })));
    }

    #[test]
    fn test_add_person_empty_name_rejected() {
        let mut chronicle = create_test_chronicle();
        assert!(matches!(
            add_person(&mut chronicle, "carol", ""),
            Err(PersonError::EmptyName)
        ));
        assert!(matches!(
            add_person(&mut chronicle, "carol", "   "),
            Err(PersonError::EmptyName)
        ));
    }

    #[test]
    fn test_add_person_invalid_id_rejected() {
        let mut chronicle = create_test_chronicle();
        assert!(matches!(
            add_person(&mut chronicle, "Carol", "Carol"),
            Err(PersonError::InvalidId { .. })
        ));
        assert!(matches!(
            add_person(&mut chronicle, "2cool", "Carol"),
            Err(PersonError::InvalidId { .. })
        ));
    }

    #[test]
    fn test_remove_person_unreferenced() {
        let mut chronicle = create_test_chronicle();
        // alice references bob, but nobody references alice
        let result = remove_person(&mut chronicle, "alice", false).unwrap();
        assert_eq!(result.person.id, "alice");
        assert_eq!(result.person.facts.len(), 1);
        assert_eq!(result.references_stripped, 0);
        assert!(chronicle.find_person("alice").is_none());
    }

    #[test]
    fn test_remove_person_referenced_requires_force() {
        let mut chronicle = create_test_chronicle();
        // bob is referenced by alice's fact
        let result = remove_person(&mut chronicle, "bob", false);
        assert!(matches!(
            result,
            Err(PersonError::Referenced { count: 1, .. })
        ));
        // Chronicle unchanged
        assert!(chronicle.find_person("bob").is_some());
    }

    #[test]
    fn test_remove_person_force_strips_references() {
        let mut chronicle = create_test_chronicle();
        let result = remove_person(&mut chronicle, "bob", true).unwrap();
        assert_eq!(result.references_stripped, 1);
        assert!(chronicle.find_person("bob").is_none());
        // The empty 'with' list is dropped entirely
        assert!(
            chronicle.find_person("alice").unwrap().facts[0]
                .with
                .is_none()
        );
    }

    #[test]
    fn test_remove_person_self_reference_neither_blocks_nor_counts() {
        // A (hand-edited) self-reference in the removed person's own facts:
        // it must not block removal without --force, and must not inflate
        // references_stripped — count and strip use the same scope.
        let mut chronicle = create_test_chronicle();
        chronicle.find_person_mut("bob").unwrap().facts.push(
            Fact::new("uuid-2", "2021-01-01", "family", "Self ref")
                .with_persons(vec!["bob".to_string()]),
        );

        let result = remove_person(&mut chronicle, "bob", true).unwrap();
        // Only alice's reference counts; bob's self-reference vanished with him
        assert_eq!(result.references_stripped, 1);
        assert!(chronicle.find_person("bob").is_none());
    }

    #[test]
    fn test_remove_person_not_found() {
        let mut chronicle = create_test_chronicle();
        assert!(matches!(
            remove_person(&mut chronicle, "unknown", false),
            Err(PersonError::NotFound { .. })
        ));
    }
}
