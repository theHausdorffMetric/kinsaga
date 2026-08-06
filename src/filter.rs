//! Search and filter functionality for chronicles.

use crate::date::{ChronicleDate, cmp_date_strings};
use crate::{Chronicle, Fact, Person};
use regex::Regex;

/// Text matching mode for a filter.
#[derive(Debug, Clone)]
enum TextFilter {
    /// Case-insensitive substring match (stored lowercased)
    Substring(String),
    /// Case-insensitive regex match, compiled once when the filter is built
    Pattern(Regex),
}

/// A filter for querying facts.
#[derive(Debug, Default, Clone)]
pub struct FactFilter {
    /// Filter by category ID
    pub category: Option<String>,

    /// Filter by minimum year (inclusive)
    pub from_year: Option<u16>,

    /// Filter by maximum year (inclusive)
    pub to_year: Option<u16>,

    /// Text filter (substring or precompiled regex)
    text: Option<TextFilter>,
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

    /// Filter by text content (case-insensitive substring match).
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(TextFilter::Substring(text.into().to_lowercase()));
        self
    }

    /// Filter by a regex pattern (case-insensitive).
    ///
    /// The pattern is compiled once here; an invalid pattern is an error
    /// instead of silently matching nothing.
    pub fn with_regex_text(mut self, pattern: &str) -> Result<Self, regex::Error> {
        let regex = Regex::new(&format!("(?i){}", pattern))?;
        self.text = Some(TextFilter::Pattern(regex));
        Ok(self)
    }

    /// Check if a fact matches this filter.
    pub fn matches(&self, fact: &Fact) -> bool {
        // Category filter
        if let Some(ref cat) = self.category
            && &fact.category != cat
        {
            return false;
        }

        // Year filtering: when a year range is set, facts whose date cannot
        // be parsed are excluded — they cannot be placed in the range
        if self.from_year.is_some() || self.to_year.is_some() {
            match ChronicleDate::parse(&fact.date).ok().and_then(|d| d.year) {
                Some(year) => {
                    if let Some(from) = self.from_year
                        && year < from
                    {
                        return false;
                    }
                    if let Some(to) = self.to_year
                        && year > to
                    {
                        return false;
                    }
                }
                None => return false,
            }
        }

        // Text filter (case-insensitive) - searches across all text fields combined
        // This allows regex patterns to match across fields (e.g., "foo.*bar" matches
        // if "foo" is in text and "bar" is in location)
        if let Some(ref text_filter) = self.text {
            // Build combined searchable text from all fields
            let mut combined = fact.text.clone();
            if let Some(ref loc) = fact.location {
                combined.push(' ');
                combined.push_str(&loc.country);
                if let Some(ref place) = loc.place {
                    combined.push(' ');
                    combined.push_str(place);
                }
            }
            for attachment in &fact.attachments {
                if let Some(ref title) = attachment.title {
                    combined.push(' ');
                    combined.push_str(title);
                }
            }

            let matches = match text_filter {
                TextFilter::Substring(needle) => combined.to_lowercase().contains(needle),
                TextFilter::Pattern(regex) => regex.is_match(&combined),
            };

            if !matches {
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
///
/// Results are grouped by person (chronicle document order) and sorted
/// chronologically within each person; facts with unparseable dates sort
/// last within their group.
pub fn search<'a>(chronicle: &'a Chronicle, filter: &FactFilter) -> Vec<SearchResult<'a>> {
    let mut results = Vec::new();

    for person in &chronicle.persons {
        let mut matches: Vec<&Fact> = person.facts.iter().filter(|f| filter.matches(f)).collect();
        matches.sort_by(|a, b| cmp_date_strings(&a.date, &b.date));
        for fact in matches {
            results.push(SearchResult { person, fact });
        }
    }

    results
}

/// Sort facts by date. Facts with unparseable dates sort last.
pub fn sort_facts_by_date(facts: &mut [&Fact]) {
    facts.sort_by(|a, b| cmp_date_strings(&a.date, &b.date));
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
        alice.facts.push(Fact::new(
            "1",
            "1990-05-15",
            "family",
            "Born in Springfield",
        ));
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
        bob.facts.push(Fact::new(
            "5",
            "1988-03-22",
            "family",
            "Born in Shelbyville",
        ));
        bob.facts
            .push(Fact::new("6", "2020", "travel", "New York vacation trip"));
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
        let filter = FactFilter::new().with_category("education").from_year(2005);

        let alice = chronicle.find_person("alice").unwrap();
        let facts = filter_facts(alice, &filter);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].text, "Graduated university");
    }

    #[test]
    fn test_search_results_sorted_within_person() {
        let mut chronicle = Chronicle::new("1.0");
        chronicle.categories.push(Category::new("family", "Family"));
        let mut alice = Person::new("alice", "Alice");
        alice.facts.push(Fact::new("1", "2015", "family", "Later"));
        alice
            .facts
            .push(Fact::new("2", "1990", "family", "Earlier"));
        alice
            .facts
            .push(Fact::new("3", "someday", "family", "Undated"));
        chronicle.persons.push(alice);

        let results = search(&chronicle, &FactFilter::new());
        let texts: Vec<&str> = results.iter().map(|r| r.fact.text.as_str()).collect();
        assert_eq!(texts, ["Earlier", "Later", "Undated"]);
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

    #[test]
    fn test_regex_or_pattern() {
        let chronicle = sample_chronicle();
        // Match "Springfield" OR "Shelbyville" using regex OR
        let filter = FactFilter::new()
            .with_regex_text("springfield|shelbyville")
            .unwrap();

        let results = search(&chronicle, &filter);
        assert_eq!(results.len(), 2); // One from Alice, one from Bob
    }

    #[test]
    fn test_regex_case_insensitive() {
        let chronicle = sample_chronicle();
        // Regex should be case-insensitive
        let filter = FactFilter::new().with_regex_text("SPRINGFIELD").unwrap();

        let results = search(&chronicle, &filter);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_regex_word_boundary() {
        let chronicle = sample_chronicle();
        // Match "New" as word start
        let filter = FactFilter::new().with_regex_text(r"\bNew\b").unwrap();

        let results = search(&chronicle, &filter);
        assert_eq!(results.len(), 2); // "New York" appears twice
    }

    #[test]
    fn test_regex_invalid_pattern_is_error() {
        // An invalid pattern is a build-time error, not a silent no-match
        assert!(FactFilter::new().with_regex_text("[invalid").is_err());
    }

    #[test]
    fn test_year_filter_excludes_unparseable_dates() {
        let mut chronicle = Chronicle::new("1.0");
        chronicle.categories.push(Category::new("family", "Family"));
        let mut alice = Person::new("alice", "Alice");
        alice
            .facts
            .push(Fact::new("1", "not-a-date", "family", "Broken date"));
        alice
            .facts
            .push(Fact::new("2", "2010", "family", "Good date"));
        chronicle.persons.push(alice);

        let alice = chronicle.find_person("alice").unwrap();
        // Without a year filter both facts pass
        assert_eq!(filter_facts(alice, &FactFilter::new()).len(), 2);
        // With a year filter, the unparseable date cannot be placed in
        // the range and is excluded
        let filter = FactFilter::new().from_year(2000);
        let facts = filter_facts(alice, &filter);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].text, "Good date");
    }

    #[test]
    fn test_sort_unparseable_dates_last() {
        let broken = Fact::new("1", "someday", "family", "Broken");
        let old = Fact::new("2", "1990", "family", "Old");
        let new = Fact::new("3", "2020", "family", "New");

        let mut facts: Vec<&Fact> = vec![&broken, &new, &old];
        sort_facts_by_date(&mut facts);

        assert_eq!(facts[0].text, "Old");
        assert_eq!(facts[1].text, "New");
        assert_eq!(facts[2].text, "Broken");
    }

    #[test]
    fn test_regex_cross_field_matching() {
        use crate::Location;

        let mut chronicle = Chronicle::new("1.0");
        chronicle.categories.push(Category::new("travel", "Travel"));

        let mut alice = Person::new("alice", "Alice");
        let mut fact = Fact::new("1", "2024", "travel", "Hiking with Smilla");
        fact.location = Some(Location::new("Switzerland").with_place("Mettmenalp"));
        alice.facts.push(fact);
        chronicle.persons.push(alice);

        // Match "Smilla" in text AND "Mettmen" in location using regex
        let filter = FactFilter::new()
            .with_regex_text("smilla.*mettmen|mettmen.*smilla")
            .unwrap();

        let results = search(&chronicle, &filter);
        assert_eq!(results.len(), 1);

        // Single term still works
        let filter2 = FactFilter::new().with_text("smilla");
        assert_eq!(search(&chronicle, &filter2).len(), 1);

        // Non-matching cross-field
        let filter3 = FactFilter::new().with_regex_text("smilla.*zurich").unwrap();
        assert_eq!(search(&chronicle, &filter3).len(), 0);
    }
}
