//! Search and filter functionality for chronicles.

use crate::date::ChronicleDate;
use crate::{Chronicle, Fact, Person};

/// A filter for querying facts.
#[derive(Debug, Default, Clone)]
pub struct FactFilter {
    /// Filter by category ID
    pub category: Option<String>,

    /// Filter by minimum year (inclusive)
    pub from_year: Option<u16>,

    /// Filter by maximum year (inclusive)
    pub to_year: Option<u16>,

    /// Filter by text content (case-insensitive substring match)
    pub text: Option<String>,
}

impl FactFilter {
    /// Create a new empty filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by category.
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Filter by minimum year.
    pub fn from_year(mut self, year: u16) -> Self {
        self.from_year = Some(year);
        self
    }

    /// Filter by maximum year.
    pub fn to_year(mut self, year: u16) -> Self {
        self.to_year = Some(year);
        self
    }

    /// Filter by text content.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Check if a fact matches this filter.
    pub fn matches(&self, fact: &Fact) -> bool {
        // Category filter
        if let Some(ref cat) = self.category {
            if &fact.category != cat {
                return false;
            }
        }

        // Parse the date for year filtering
        if self.from_year.is_some() || self.to_year.is_some() {
            if let Ok(date) = ChronicleDate::parse(&fact.date) {
                if let Some(year) = date.year {
                    if let Some(from) = self.from_year {
                        if year < from {
                            return false;
                        }
                    }
                    if let Some(to) = self.to_year {
                        if year > to {
                            return false;
                        }
                    }
                }
            }
        }

        // Text filter (case-insensitive) - searches in text, location, and attachment titles
        if let Some(ref text) = self.text {
            let text_lower = text.to_lowercase();

            // Check main fact text
            let in_text = fact.text.to_lowercase().contains(&text_lower);

            // Check location (country and name)
            let in_location = fact.location.as_ref().is_some_and(|loc| {
                loc.country.to_lowercase().contains(&text_lower)
                    || loc
                        .name
                        .as_ref()
                        .is_some_and(|n| n.to_lowercase().contains(&text_lower))
            });

            // Check attachment titles
            let in_attachments = fact.attachments.iter().any(|a| {
                a.title
                    .as_ref()
                    .is_some_and(|t| t.to_lowercase().contains(&text_lower))
            });

            if !in_text && !in_location && !in_attachments {
                return false;
            }
        }

        true
    }
}

/// Result of a search across the chronicle.
#[derive(Debug, Clone)]
pub struct SearchResult<'a> {
    /// The person this fact belongs to
    pub person: &'a Person,

    /// The matching fact
    pub fact: &'a Fact,
}

/// Filter facts for a specific person.
pub fn filter_facts<'a>(person: &'a Person, filter: &FactFilter) -> Vec<&'a Fact> {
    person.facts.iter().filter(|f| filter.matches(f)).collect()
}

/// Search for facts across all persons in the chronicle.
pub fn search<'a>(chronicle: &'a Chronicle, filter: &FactFilter) -> Vec<SearchResult<'a>> {
    let mut results = Vec::new();

    for person in &chronicle.persons {
        for fact in &person.facts {
            if filter.matches(fact) {
                results.push(SearchResult { person, fact });
            }
        }
    }

    results
}

/// Sort facts by date.
pub fn sort_facts_by_date(facts: &mut [&Fact]) {
    facts.sort_by(|a, b| {
        let date_a = ChronicleDate::parse(&a.date).ok();
        let date_b = ChronicleDate::parse(&b.date).ok();
        match (date_a, date_b) {
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Category;

    fn sample_chronicle() -> Chronicle {
        let mut chronicle = Chronicle::new("1.0");
        chronicle
            .categories
            .push(Category::new("education", "Education"));
        chronicle.categories.push(Category::new("family", "Family"));
        chronicle.categories.push(Category::new("travel", "Travel"));

        let mut alice = Person::new("alice", "Alice Smith");
        alice
            .facts
            .push(Fact::new("1", "1990-05-15", "family", "Born in Springfield"));
        alice
            .facts
            .push(Fact::new("2", "2001", "education", "Started high school"));
        alice.facts.push(Fact::new(
            "3",
            "2015",
            "travel",
            "Moved to New York, Brooklyn",
        ));
        alice
            .facts
            .push(Fact::new("4", "2008", "education", "Graduated university"));
        chronicle.persons.push(alice);

        let mut bob = Person::new("bob", "Bob Johnson");
        bob
            .facts
            .push(Fact::new("5", "1988-03-22", "family", "Born in Shelbyville"));
        bob.facts.push(Fact::new(
            "6",
            "2020",
            "travel",
            "New York vacation trip",
        ));
        chronicle.persons.push(bob);

        chronicle
    }

    #[test]
    fn test_filter_by_category() {
        let chronicle = sample_chronicle();
        let alice = chronicle.find_person("alice").unwrap();

        let filter = FactFilter::new().with_category("education");
        let facts = filter_facts(alice, &filter);

        assert_eq!(facts.len(), 2);
        assert!(facts.iter().all(|f| f.category == "education"));
    }

    #[test]
    fn test_filter_by_year_range() {
        let chronicle = sample_chronicle();
        let alice = chronicle.find_person("alice").unwrap();

        let filter = FactFilter::new().from_year(2005).to_year(2010);
        let facts = filter_facts(alice, &filter);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].text, "Graduated university");
    }

    #[test]
    fn test_filter_by_text() {
        let chronicle = sample_chronicle();
        let filter = FactFilter::new().with_text("new york");

        let results = search(&chronicle, &filter);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_across_persons() {
        let chronicle = sample_chronicle();
        let filter = FactFilter::new().with_category("family");

        let results = search(&chronicle, &filter);
        assert_eq!(results.len(), 2);

        let persons: Vec<_> = results.iter().map(|r| &r.person.id).collect();
        assert!(persons.contains(&&"alice".to_string()));
        assert!(persons.contains(&&"bob".to_string()));
    }

    #[test]
    fn test_combined_filters() {
        let chronicle = sample_chronicle();
        let filter = FactFilter::new()
            .with_category("education")
            .from_year(2005);

        let alice = chronicle.find_person("alice").unwrap();
        let facts = filter_facts(alice, &filter);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].text, "Graduated university");
    }

    #[test]
    fn test_sort_facts_by_date() {
        let chronicle = sample_chronicle();
        let alice = chronicle.find_person("alice").unwrap();

        let mut facts: Vec<&Fact> = alice.facts.iter().collect();
        sort_facts_by_date(&mut facts);

        // Should be in chronological order
        assert_eq!(facts[0].date, "1990-05-15");
        assert_eq!(facts[1].date, "2001");
        assert_eq!(facts[2].date, "2008");
        assert_eq!(facts[3].date, "2015");
    }
}
