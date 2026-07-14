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

    /// Last updated timestamp (ISO 8601 UTC, e.g., "2025-12-26T14:30:00Z")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,

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

    /// Optional location where the event occurred
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,

    /// Attachments (photos, documents, links)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
}

/// GPS coordinates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Coordinates {
    /// Latitude (-90.0 to 90.0)
    pub lat: f64,

    /// Longitude (-180.0 to 180.0)
    pub lon: f64,
}

/// A geographic location.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Location {
    /// Country name or ISO code (required)
    pub country: String,

    /// Optional place name (city, address, landmark, venue, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,

    /// Optional GPS coordinates
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinates: Option<Coordinates>,
}

/// An attachment (photo, document, link).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Attachment {
    /// URL to the resource (file://, https://, s3://, etc.)
    ///
    /// Stored as the raw string: a malformed URL in a chronicle file is a
    /// validation warning (see `validate_chronicle`) rather than a load
    /// failure, and saving never rewrites (normalizes) user data.
    pub url: String,

    /// Optional MIME content type (e.g., "image/jpeg", "application/pdf")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,

    /// Optional human-readable title/description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl Chronicle {
    /// Create a new empty chronicle with the given version.
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            title: None,
            last_updated: None,
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
            location: None,
            attachments: Vec::new(),
        }
    }

    /// Add persons involved in this event.
    pub fn with_persons(mut self, persons: Vec<String>) -> Self {
        self.with = Some(persons);
        self
    }

    /// Set the location for this event.
    pub fn with_location(mut self, location: Location) -> Self {
        self.location = Some(location);
        self
    }

    /// Add an attachment to this event.
    pub fn with_attachment(mut self, attachment: Attachment) -> Self {
        self.attachments.push(attachment);
        self
    }

    /// Add multiple attachments to this event.
    pub fn with_attachments(mut self, attachments: Vec<Attachment>) -> Self {
        self.attachments.extend(attachments);
        self
    }
}

impl Coordinates {
    /// Create new GPS coordinates.
    pub fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }

    /// Check if coordinates are valid (lat: -90..90, lon: -180..180).
    pub fn is_valid(&self) -> bool {
        (-90.0..=90.0).contains(&self.lat) && (-180.0..=180.0).contains(&self.lon)
    }
}

impl Location {
    /// Create a new location with just a country.
    pub fn new(country: impl Into<String>) -> Self {
        Self {
            country: country.into(),
            place: None,
            coordinates: None,
        }
    }

    /// Set the place name (city, address, landmark, etc.).
    pub fn with_place(mut self, place: impl Into<String>) -> Self {
        self.place = Some(place.into());
        self
    }

    /// Set GPS coordinates.
    pub fn with_coordinates(mut self, coordinates: Coordinates) -> Self {
        self.coordinates = Some(coordinates);
        self
    }
}

impl Attachment {
    /// Create a new attachment with just a URL.
    ///
    /// The URL is not validated here; use `facts::build_attachments` or
    /// `validate_chronicle` for format checking.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            content_type: None,
            title: None,
        }
    }

    /// Set the MIME content type.
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// Set the title/description.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
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
        chronicle
            .categories
            .push(Category::new("family", "Family & Friends").with_color("#E57373"));

        let mut person = Person::new("alice", "Alice Smith");
        person.facts.push(Fact::new(
            "uuid-1",
            "1990-05-15",
            "family",
            "Born in Springfield",
        ));
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

    #[test]
    fn test_coordinates_validation() {
        // Valid coordinates
        assert!(Coordinates::new(0.0, 0.0).is_valid());
        assert!(Coordinates::new(90.0, 180.0).is_valid());
        assert!(Coordinates::new(-90.0, -180.0).is_valid());
        assert!(Coordinates::new(48.8566, 2.3522).is_valid()); // Paris

        // Invalid coordinates
        assert!(!Coordinates::new(91.0, 0.0).is_valid());
        assert!(!Coordinates::new(-91.0, 0.0).is_valid());
        assert!(!Coordinates::new(0.0, 181.0).is_valid());
        assert!(!Coordinates::new(0.0, -181.0).is_valid());
    }

    #[test]
    fn test_location_builder() {
        let loc = Location::new("France")
            .with_place("Paris")
            .with_coordinates(Coordinates::new(48.8566, 2.3522));

        assert_eq!(loc.country, "France");
        assert_eq!(loc.place, Some("Paris".to_string()));
        assert!(loc.coordinates.is_some());
        let coords = loc.coordinates.unwrap();
        assert!((coords.lat - 48.8566).abs() < 0.0001);
    }

    #[test]
    fn test_attachment_builder() {
        let att = Attachment::new("https://example.com/photo.jpg")
            .with_content_type("image/jpeg")
            .with_title("Wedding photo");

        assert_eq!(att.url, "https://example.com/photo.jpg");
        assert_eq!(att.content_type, Some("image/jpeg".to_string()));
        assert_eq!(att.title, Some("Wedding photo".to_string()));
    }

    #[test]
    fn test_fact_with_location_and_attachments() {
        let fact = Fact::new("uuid-1", "2020-06-15", "family", "Wedding day")
            .with_location(Location::new("France").with_place("Paris"))
            .with_attachment(
                Attachment::new("file:///photos/wedding.jpg").with_title("Wedding photo"),
            );

        assert!(fact.location.is_some());
        assert_eq!(fact.location.as_ref().unwrap().country, "France");
        assert_eq!(fact.attachments.len(), 1);
        assert_eq!(fact.attachments[0].title, Some("Wedding photo".to_string()));
    }

    #[test]
    fn test_serialize_fact_with_location() {
        let fact = Fact::new("uuid-1", "2020-06-15", "travel", "Visited Eiffel Tower")
            .with_location(
                Location::new("France")
                    .with_place("Eiffel Tower, Paris")
                    .with_coordinates(Coordinates::new(48.8584, 2.2945)),
            );

        let json = serde_json::to_string_pretty(&fact).unwrap();
        assert!(json.contains("\"country\": \"France\""));
        assert!(json.contains("\"place\": \"Eiffel Tower, Paris\""));
        assert!(json.contains("\"lat\": 48.8584"));
    }

    #[test]
    fn test_serialize_fact_with_attachments() {
        let fact = Fact::new("uuid-1", "2020-06-15", "travel", "Visited Eiffel Tower")
            .with_attachment(
                Attachment::new("s3://bucket/photos/eiffel.jpg")
                    .with_content_type("image/jpeg")
                    .with_title("Eiffel Tower photo"),
            );

        let json = serde_json::to_string_pretty(&fact).unwrap();
        assert!(json.contains("s3://bucket/photos/eiffel.jpg"));
        assert!(json.contains("\"content_type\": \"image/jpeg\""));
        assert!(json.contains("\"title\": \"Eiffel Tower photo\""));
    }

    #[test]
    fn test_deserialize_fact_with_location_and_attachments() {
        let json = r#"{
            "id": "uuid-1",
            "date": "2020-06-15",
            "category": "travel",
            "text": "Visited Eiffel Tower",
            "location": {
                "country": "France",
                "place": "Eiffel Tower, Paris",
                "coordinates": { "lat": 48.8584, "lon": 2.2945 }
            },
            "attachments": [
                {
                    "url": "https://example.com/photo.jpg",
                    "content_type": "image/jpeg",
                    "title": "Eiffel Tower"
                }
            ]
        }"#;

        let fact: Fact = serde_json::from_str(json).unwrap();
        assert!(fact.location.is_some());
        let loc = fact.location.unwrap();
        assert_eq!(loc.country, "France");
        assert_eq!(loc.place, Some("Eiffel Tower, Paris".to_string()));
        assert!(loc.coordinates.is_some());

        assert_eq!(fact.attachments.len(), 1);
        assert_eq!(fact.attachments[0].url, "https://example.com/photo.jpg");
        assert_eq!(
            fact.attachments[0].content_type,
            Some("image/jpeg".to_string())
        );
    }

    #[test]
    fn test_fact_without_optional_fields_serializes_cleanly() {
        let fact = Fact::new("uuid-1", "2020-06-15", "family", "Simple event");
        let json = serde_json::to_string(&fact).unwrap();

        // Should not contain empty location or attachments
        assert!(!json.contains("location"));
        assert!(!json.contains("attachments"));
        assert!(!json.contains("with"));
    }
}
