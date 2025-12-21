//! Data model for family chronicles.

use serde::{Deserialize, Serialize};

/// A family chronicle containing persons and their life events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chronicle {
    /// Schema version (e.g., "1.0")
    pub version: String,

    /// Optional title for the chronicle
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Available categories for facts
    pub categories: Vec<Category>,

    /// Persons in the chronicle
    pub persons: Vec<Person>,
}

/// A category for classifying facts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    /// Unique identifier (e.g., "education")
    pub id: String,

    /// Display label (e.g., "Education & Job")
    pub label: String,

    /// Optional color for display (e.g., "#4A90D9")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// A person in the chronicle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    /// Unique identifier (e.g., "daniel")
    pub id: String,

    /// Display name (e.g., "Alice Smith")
    pub name: String,

    /// Life events for this person
    pub facts: Vec<Fact>,
}

/// A life event or fact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    /// Unique identifier (UUID)
    pub id: String,

    /// Date in ISO 8601 format, optionally with ? suffix for uncertainty
    /// Examples: "1987", "1987-03", "1987-03-03", "1987?"
    pub date: String,

    /// Category ID (references Category.id)
    pub category: String,

    /// Description of the event
    pub text: String,

    /// Optional list of other person IDs involved in this event
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with: Option<Vec<String>>,
}

impl Chronicle {
    /// Create a new empty chronicle with the given version.
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            title: None,
            categories: Vec::new(),
            persons: Vec::new(),
        }
    }

    /// Find a person by ID.
    pub fn find_person(&self, id: &str) -> Option<&Person> {
        self.persons.iter().find(|p| p.id == id)
    }

    /// Find a category by ID.
    pub fn find_category(&self, id: &str) -> Option<&Category> {
        self.categories.iter().find(|c| c.id == id)
    }

    /// Find a person by ID (mutable).
    pub fn find_person_mut(&mut self, id: &str) -> Option<&mut Person> {
        self.persons.iter_mut().find(|p| p.id == id)
    }
}

impl Person {
    /// Create a new person with no facts.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            facts: Vec::new(),
        }
    }
}

impl Fact {
    /// Create a new fact.
    pub fn new(
        id: impl Into<String>,
        date: impl Into<String>,
        category: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            date: date.into(),
            category: category.into(),
            text: text.into(),
            with: None,
        }
    }

    /// Add persons involved in this event.
    pub fn with_persons(mut self, persons: Vec<String>) -> Self {
        self.with = Some(persons);
        self
    }
}

impl Category {
    /// Create a new category.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            color: None,
        }
    }

    /// Set the display color.
    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_chronicle() {
        let mut chronicle = Chronicle::new("1.0");
        chronicle.title = Some("Test Chronicle".into());
        chronicle.categories.push(
            Category::new("family", "Family & Friends")
                .with_color("#E57373")
        );

        let mut person = Person::new("alice", "Alice Smith");
        person.facts.push(
            Fact::new(
                "uuid-1",
                "1990-05-15",
                "family",
                "Born in Springfield",
            )
        );
        chronicle.persons.push(person);

        let json = serde_json::to_string_pretty(&chronicle).unwrap();
        assert!(json.contains("\"version\": \"1.0\""));
        assert!(json.contains("\"id\": \"alice\""));
        assert!(json.contains("Born in Springfield"));
    }

    #[test]
    fn test_deserialize_chronicle() {
        let json = r#"{
            "version": "1.0",
            "categories": [
                { "id": "family", "label": "Family & Friends" }
            ],
            "persons": [
                {
                    "id": "alice",
                    "name": "Alice Smith",
                    "facts": [
                        {
                            "id": "uuid-1",
                            "date": "1990-05-15",
                            "category": "family",
                            "text": "Born in Springfield"
                        }
                    ]
                }
            ]
        }"#;

        let chronicle: Chronicle = serde_json::from_str(json).unwrap();
        assert_eq!(chronicle.version, "1.0");
        assert_eq!(chronicle.persons.len(), 1);
        assert_eq!(chronicle.persons[0].facts.len(), 1);
        assert_eq!(chronicle.persons[0].facts[0].text, "Born in Springfield");
    }

    #[test]
    fn test_find_person() {
        let mut chronicle = Chronicle::new("1.0");
        chronicle.persons.push(Person::new("alice", "Alice"));
        chronicle.persons.push(Person::new("bob", "Bob"));

        assert!(chronicle.find_person("alice").is_some());
        assert!(chronicle.find_person("unknown").is_none());
    }
}
