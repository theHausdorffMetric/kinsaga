//! Geocoding functionality using Nominatim (OpenStreetMap).

use crate::{Chronicle, Coordinates, Location};
use serde::Deserialize;
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;

/// Errors that can occur during geocoding operations.
#[derive(Debug, Error)]
pub enum GeocodeError {
    #[error("HTTP request failed: {0}")]
    HttpError(String),

    #[error("Failed to parse response: {0}")]
    ParseError(String),

    #[error("Nominatim error: {0}")]
    Nominatim(String),

    #[error("No results found")]
    NoResults,

    #[error("Invalid coordinates: lat={lat}, lon={lon}")]
    InvalidCoordinates { lat: f64, lon: f64 },
}

/// A place returned by Nominatim.
#[derive(Debug, Clone)]
pub struct GeocodedPlace {
    /// Display name (full address)
    pub display_name: String,
    /// Country name
    pub country: Option<String>,
    /// City/town/village
    pub city: Option<String>,
    /// State/province/region
    pub state: Option<String>,
    /// Latitude
    pub lat: f64,
    /// Longitude
    pub lon: f64,
}

/// Raw response from Nominatim reverse geocoding.
///
/// Nominatim reports unknown areas (e.g. open ocean) as
/// `{"error": "Unable to geocode"}` with HTTP 200, so all fields are
/// optional and an `error` field is captured.
#[derive(Debug, Deserialize)]
struct NominatimReverseResponse {
    error: Option<String>,
    display_name: Option<String>,
    lat: Option<String>,
    lon: Option<String>,
    address: Option<NominatimAddress>,
}

/// Address component from Nominatim.
#[derive(Debug, Deserialize)]
struct NominatimAddress {
    country: Option<String>,
    city: Option<String>,
    town: Option<String>,
    village: Option<String>,
    state: Option<String>,
}

/// Raw response from Nominatim forward geocoding (search).
#[derive(Debug, Deserialize)]
struct NominatimSearchResult {
    display_name: String,
    lat: String,
    lon: String,
    address: Option<NominatimAddress>,
}

/// Default Nominatim instance.
const DEFAULT_BASE_URL: &str = "https://nominatim.openstreetmap.org";

/// Client for Nominatim geocoding API.
pub struct NominatimClient {
    agent: ureq::Agent,
    user_agent: String,
    base_url: String,
    last_request: Option<Instant>,
    min_interval: Duration,
}

impl NominatimClient {
    /// Create a new Nominatim client.
    ///
    /// The user_agent should identify your application per Nominatim's usage policy.
    /// Requests time out (10 s connect, 30 s total) so a stalled connection
    /// cannot hang the caller.
    pub fn new(user_agent: impl Into<String>) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .new_agent();
        Self {
            agent,
            user_agent: user_agent.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
            last_request: None,
            min_interval: Duration::from_millis(1100), // Slightly over 1 second to be safe
        }
    }

    /// Use a different Nominatim instance (e.g. self-hosted, or a mock in tests).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into().trim_end_matches('/').to_string();
        self
    }

    /// Wait if necessary to respect rate limits.
    fn rate_limit(&mut self) {
        if let Some(last) = self.last_request {
            let elapsed = last.elapsed();
            if elapsed < self.min_interval {
                thread::sleep(self.min_interval - elapsed);
            }
        }
        self.last_request = Some(Instant::now());
    }

    /// Perform a GET request and return the response body.
    fn get(&mut self, url: &str) -> Result<String, GeocodeError> {
        self.rate_limit();
        let response = self
            .agent
            .get(url)
            .header("User-Agent", &self.user_agent)
            .call()
            .map_err(|e| GeocodeError::HttpError(e.to_string()))?;
        response
            .into_body()
            .read_to_string()
            .map_err(|e| GeocodeError::ParseError(e.to_string()))
    }

    /// Reverse geocode coordinates to get place information.
    pub fn reverse_geocode(&mut self, lat: f64, lon: f64) -> Result<GeocodedPlace, GeocodeError> {
        // Validate coordinates
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return Err(GeocodeError::InvalidCoordinates { lat, lon });
        }

        let url = format!(
            "{}/reverse?lat={}&lon={}&format=json&addressdetails=1",
            self.base_url, lat, lon
        );
        let body = self.get(&url)?;
        parse_reverse_response(&body)
    }

    /// Forward geocode a place name to get coordinates.
    ///
    /// Returns up to `limit` results, sorted by relevance.
    pub fn forward_geocode(
        &mut self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<GeocodedPlace>, GeocodeError> {
        // form_urlencoded (from the url crate already in-tree) encodes the
        // query; spaces become '+', which Nominatim accepts in query strings
        let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
        let url = format!(
            "{}/search?q={}&format=json&addressdetails=1&limit={}",
            self.base_url, encoded, limit
        );
        let body = self.get(&url)?;
        let places = parse_search_response(&body)?;
        if places.is_empty() {
            return Err(GeocodeError::NoResults);
        }
        Ok(places)
    }
}

/// Extract the city from the address fields Nominatim may use.
fn extract_city(address: Option<&NominatimAddress>) -> Option<String> {
    address.and_then(|a| {
        a.city
            .clone()
            .or_else(|| a.town.clone())
            .or_else(|| a.village.clone())
    })
}

/// Parse a Nominatim reverse-geocoding response body.
fn parse_reverse_response(body: &str) -> Result<GeocodedPlace, GeocodeError> {
    let parsed: NominatimReverseResponse =
        serde_json::from_str(body).map_err(|e| GeocodeError::ParseError(e.to_string()))?;

    if let Some(error) = parsed.error {
        // "Unable to geocode" means the coordinates hit no known area
        // (e.g. open ocean); other messages are genuine service errors
        return if error == "Unable to geocode" {
            Err(GeocodeError::NoResults)
        } else {
            Err(GeocodeError::Nominatim(error))
        };
    }

    let display_name = parsed
        .display_name
        .ok_or_else(|| GeocodeError::ParseError("missing display_name in response".to_string()))?;
    let lat: f64 = parsed
        .lat
        .as_deref()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| GeocodeError::ParseError("invalid latitude in response".to_string()))?;
    let lon: f64 = parsed
        .lon
        .as_deref()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| GeocodeError::ParseError("invalid longitude in response".to_string()))?;

    Ok(GeocodedPlace {
        city: extract_city(parsed.address.as_ref()),
        country: parsed.address.as_ref().and_then(|a| a.country.clone()),
        state: parsed.address.as_ref().and_then(|a| a.state.clone()),
        display_name,
        lat,
        lon,
    })
}

/// Parse a Nominatim search (forward-geocoding) response body.
fn parse_search_response(body: &str) -> Result<Vec<GeocodedPlace>, GeocodeError> {
    let results: Vec<NominatimSearchResult> = match serde_json::from_str(body) {
        Ok(results) => results,
        Err(parse_err) => {
            // Errors come back as a JSON object instead of an array,
            // either {"error": "..."} or {"error": {"code": .., "message": ".."}}
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(body)
                && let Some(error) = value.get("error")
            {
                let message = error
                    .as_str()
                    .map(String::from)
                    .or_else(|| {
                        error
                            .get("message")
                            .and_then(|m| m.as_str())
                            .map(String::from)
                    })
                    .unwrap_or_else(|| error.to_string());
                return Err(GeocodeError::Nominatim(message));
            }
            return Err(GeocodeError::ParseError(parse_err.to_string()));
        }
    };

    Ok(results
        .into_iter()
        .filter_map(|r| {
            let lat: f64 = r.lat.parse().ok()?;
            let lon: f64 = r.lon.parse().ok()?;
            Some(GeocodedPlace {
                city: extract_city(r.address.as_ref()),
                country: r.address.as_ref().and_then(|a| a.country.clone()),
                state: r.address.as_ref().and_then(|a| a.state.clone()),
                display_name: r.display_name,
                lat,
                lon,
            })
        })
        .collect())
}

/// Result of validating a location's GPS coordinates.
#[derive(Debug)]
pub struct GpsValidationResult {
    /// The fact ID
    pub fact_id: String,
    /// Name of the person the fact belongs to
    pub person_name: String,
    /// The stored location description
    pub stored_country: String,
    /// The stored place (if any)
    pub stored_place: Option<String>,
    /// The stored coordinates
    pub stored_lat: f64,
    pub stored_lon: f64,
    /// What Nominatim returned for these coordinates
    pub nominatim_result: GeocodedPlace,
    /// Whether the country matches (case-insensitive)
    pub country_matches: bool,
    /// Whether the place roughly matches (if stored)
    pub place_matches: Option<bool>,
}

impl GpsValidationResult {
    /// Check if this result indicates a mismatch.
    pub fn is_mismatch(&self) -> bool {
        !self.country_matches || self.place_matches == Some(false)
    }
}

/// Outcome of checking one fact's GPS coordinates.
#[derive(Debug)]
pub enum GpsCheckOutcome {
    /// Lookup succeeded; see the result for whether it matches
    Checked(GpsValidationResult),
    /// Lookup failed (network, parse, ...)
    Failed {
        fact_id: String,
        person_name: String,
        error: GeocodeError,
    },
}

/// A coordinate suggestion for a fact whose location has no GPS data.
#[derive(Debug)]
pub struct GpsSuggestion {
    pub fact_id: String,
    pub person_name: String,
    /// The query that was sent to Nominatim
    pub query: String,
    /// Candidate places, best match first
    pub candidates: Vec<GeocodedPlace>,
}

/// Outcome of looking up coordinate suggestions for one fact.
#[derive(Debug)]
pub enum GpsSuggestOutcome {
    Suggested(GpsSuggestion),
    NoResults {
        fact_id: String,
        person_name: String,
        query: String,
    },
    Failed {
        fact_id: String,
        person_name: String,
        query: String,
        error: GeocodeError,
    },
}

/// Number of facts whose location has GPS coordinates.
pub fn count_facts_with_coordinates(chronicle: &Chronicle) -> usize {
    chronicle
        .persons
        .iter()
        .flat_map(|p| &p.facts)
        .filter(|f| f.location.as_ref().is_some_and(|l| l.coordinates.is_some()))
        .count()
}

/// Number of facts that have a location but no GPS coordinates.
pub fn count_facts_without_coordinates(chronicle: &Chronicle) -> usize {
    chronicle
        .persons
        .iter()
        .flat_map(|p| &p.facts)
        .filter(|f| f.location.as_ref().is_some_and(|l| l.coordinates.is_none()))
        .count()
}

/// Compare a stored location against a geocoded place.
/// Returns `(country_matches, place_matches)`.
fn evaluate_location_match(location: &Location, place: &GeocodedPlace) -> (bool, Option<bool>) {
    let country_matches = place
        .country
        .as_ref()
        .is_some_and(|c| fuzzy_match(c, &location.country));

    let place_matches = location.place.as_ref().map(|stored_place| {
        place
            .city
            .as_ref()
            .is_some_and(|c| fuzzy_match(c, stored_place))
            || place
                .state
                .as_ref()
                .is_some_and(|s| fuzzy_match(s, stored_place))
            || fuzzy_match(&place.display_name, stored_place)
    });

    (country_matches, place_matches)
}

/// Validate the GPS coordinates of every fact that has them.
///
/// Lookups are rate-limited by the client (1 req/sec). `on_outcome` is
/// invoked after each lookup so callers can report progress; the complete
/// list of outcomes is returned.
pub fn validate_gps(
    chronicle: &Chronicle,
    client: &mut NominatimClient,
    mut on_outcome: impl FnMut(&GpsCheckOutcome),
) -> Vec<GpsCheckOutcome> {
    let mut outcomes = Vec::new();
    for person in &chronicle.persons {
        for fact in &person.facts {
            let Some(location) = fact.location.as_ref() else {
                continue;
            };
            let Some(coords) = location.coordinates.as_ref() else {
                continue;
            };

            let outcome = match client.reverse_geocode(coords.lat, coords.lon) {
                Ok(place) => {
                    let (country_matches, place_matches) =
                        evaluate_location_match(location, &place);
                    GpsCheckOutcome::Checked(GpsValidationResult {
                        fact_id: fact.id.clone(),
                        person_name: person.name.clone(),
                        stored_country: location.country.clone(),
                        stored_place: location.place.clone(),
                        stored_lat: coords.lat,
                        stored_lon: coords.lon,
                        nominatim_result: place,
                        country_matches,
                        place_matches,
                    })
                }
                Err(error) => GpsCheckOutcome::Failed {
                    fact_id: fact.id.clone(),
                    person_name: person.name.clone(),
                    error,
                },
            };
            on_outcome(&outcome);
            outcomes.push(outcome);
        }
    }
    outcomes
}

/// Build the Nominatim search query for a stored location.
fn suggestion_query(country: &str, place: Option<&str>) -> String {
    match place {
        Some(p) => format!("{}, {}", p, country),
        None => country.to_string(),
    }
}

/// Look up coordinate suggestions for every fact that has a location but no
/// GPS coordinates.
///
/// Lookups are rate-limited by the client. `on_outcome` is invoked after
/// each lookup so callers can report progress; the complete list of
/// outcomes is returned.
pub fn suggest_coordinates(
    chronicle: &Chronicle,
    client: &mut NominatimClient,
    limit: usize,
    mut on_outcome: impl FnMut(&GpsSuggestOutcome),
) -> Vec<GpsSuggestOutcome> {
    let mut outcomes = Vec::new();
    for person in &chronicle.persons {
        for fact in &person.facts {
            let Some(location) = fact.location.as_ref() else {
                continue;
            };
            if location.coordinates.is_some() {
                continue;
            }

            let query = suggestion_query(&location.country, location.place.as_deref());
            let outcome = match client.forward_geocode(&query, limit) {
                Ok(candidates) if candidates.is_empty() => GpsSuggestOutcome::NoResults {
                    fact_id: fact.id.clone(),
                    person_name: person.name.clone(),
                    query,
                },
                Ok(candidates) => GpsSuggestOutcome::Suggested(GpsSuggestion {
                    fact_id: fact.id.clone(),
                    person_name: person.name.clone(),
                    query,
                    candidates,
                }),
                Err(GeocodeError::NoResults) => GpsSuggestOutcome::NoResults {
                    fact_id: fact.id.clone(),
                    person_name: person.name.clone(),
                    query,
                },
                Err(error) => GpsSuggestOutcome::Failed {
                    fact_id: fact.id.clone(),
                    person_name: person.name.clone(),
                    query,
                    error,
                },
            };
            on_outcome(&outcome);
            outcomes.push(outcome);
        }
    }
    outcomes
}

/// Apply the top candidate of each suggestion to the chronicle.
///
/// Returns the number of facts updated.
pub fn apply_suggestions(chronicle: &mut Chronicle, suggestions: &[GpsSuggestion]) -> usize {
    let mut applied = 0;
    for suggestion in suggestions {
        let Some(top) = suggestion.candidates.first() else {
            continue;
        };
        for person in &mut chronicle.persons {
            for fact in &mut person.facts {
                if fact.id == suggestion.fact_id
                    && let Some(ref mut location) = fact.location
                {
                    location.coordinates = Some(Coordinates::new(top.lat, top.lon));
                    applied += 1;
                }
            }
        }
    }
    applied
}

/// Native-language and colloquial country-name aliases, keyed by ISO 3166-1
/// alpha-2 code. The alpha-2/alpha-3 codes and official English names come
/// from `rust_iso3166` (full 249-entry coverage); this overlay adds what
/// Nominatim actually returns for local places ("Schweiz", "日本",
/// "Україна") plus colloquialisms the ISO list doesn't know ("UK", "USA",
/// "Holland", "Russia" — the official name is "Russian Federation").
const NATIVE_ALIASES: &[(&str, &[&str])] = &[
    (
        "CH",
        &["switzerland", "schweiz", "suisse", "svizzera", "svizra"],
    ),
    ("DE", &["germany", "deutschland"]),
    ("AT", &["austria", "österreich", "oesterreich"]),
    ("FR", &["france"]),
    ("IT", &["italy", "italia"]),
    ("ES", &["spain", "españa", "espana"]),
    ("PT", &["portugal"]),
    ("GB", &["united kingdom", "uk", "great britain", "england"]),
    ("US", &["united states", "usa", "america"]),
    ("CA", &["canada"]),
    ("AU", &["australia"]),
    ("NZ", &["new zealand"]),
    ("JP", &["japan", "日本", "nippon", "nihon"]),
    ("CN", &["china", "中国", "zhongguo"]),
    ("KR", &["south korea", "korea", "대한민국", "한국"]),
    ("IN", &["india"]),
    ("BR", &["brazil", "brasil"]),
    ("MX", &["mexico", "méxico"]),
    ("AR", &["argentina"]),
    ("NL", &["netherlands", "holland", "nederland"]),
    ("BE", &["belgium", "belgique", "belgië", "belgie"]),
    ("PL", &["poland", "polska"]),
    ("CZ", &["czech republic", "czechia", "česko", "cesko"]),
    ("SE", &["sweden", "sverige"]),
    ("NO", &["norway", "norge"]),
    ("DK", &["denmark", "danmark"]),
    ("FI", &["finland", "suomi"]),
    ("RU", &["russia", "россия", "rossiya"]),
    ("GR", &["greece", "ελλάδα", "ellada"]),
    ("TR", &["turkey", "türkiye", "turkiye"]),
    ("IE", &["ireland", "éire", "eire"]),
    ("ZA", &["south africa"]),
    ("EG", &["egypt", "مصر"]),
    ("IL", &["israel", "ישראל"]),
    ("AE", &["united arab emirates", "uae"]),
    ("SG", &["singapore"]),
    ("TH", &["thailand", "ประเทศไทย"]),
    ("VN", &["vietnam", "việt nam"]),
    ("ID", &["indonesia"]),
    ("MY", &["malaysia"]),
    ("PH", &["philippines"]),
    ("UA", &["ukraine", "україна", "ukraina"]),
    ("HR", &["croatia", "hrvatska"]),
    ("EE", &["estonia", "eesti"]),
    ("HU", &["hungary", "magyarország", "magyarorszag"]),
    ("RO", &["romania", "românia"]),
];

/// Check if `needle` occurs in `haystack` on word boundaries, i.e. not as
/// part of a longer alphanumeric run. Plain substring matching would let
/// short strings like ISO codes match unrelated names ("CH" in "China",
/// "US" in "Russia", "uk" in "Ukraine").
fn contains_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let begin = start + pos;
        let end = begin + needle.len();
        let before_ok = haystack[..begin]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let after_ok = haystack[end..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
        // Boundary check failed: advance one character and keep searching
        start = begin + haystack[begin..].chars().next().map_or(1, |c| c.len_utf8());
    }
    false
}

/// Resolve a string that *is* a country designation — an alpha-2/alpha-3
/// code, an alias, or an official ISO name, exactly — to its alpha-2 code.
///
/// Compound strings (a Nominatim display name like "Mettmenalp, Glarus,
/// Schweiz") deliberately do not resolve here; see [`country_code_of`].
fn country_code_exact(s: &str) -> Option<&'static str> {
    let trimmed = s.trim();
    if trimmed.len() == 2
        && let Some(c) = rust_iso3166::from_alpha2(&trimmed.to_uppercase())
    {
        return Some(c.alpha2);
    }
    if trimmed.len() == 3
        && let Some(c) = rust_iso3166::from_alpha3(&trimmed.to_uppercase())
    {
        return Some(c.alpha2);
    }

    let lower = trimmed.to_lowercase();
    for (code, names) in NATIVE_ALIASES {
        if names.iter().any(|n| *n == lower) {
            return Some(code);
        }
    }
    rust_iso3166::ALL
        .iter()
        .find(|c| c.name.to_lowercase() == lower)
        .map(|c| c.alpha2)
}

/// Resolve a country string to its ISO 3166-1 alpha-2 code, leniently.
///
/// Everything [`country_code_exact`] accepts, plus word-boundary
/// containment in either direction: "United States" ⊂ "United States of
/// America", "Bolivia" ⊂ "Bolivia (Plurinational State of)", "Schweiz"
/// inside "Schweiz/Suisse/Svizzera/Svizra". Exact official-name matches
/// win before containment, so "Sudan" can't land on "South Sudan".
fn country_code_of(s: &str) -> Option<&'static str> {
    if let Some(code) = country_code_exact(s) {
        return Some(code);
    }
    let lower = s.trim().to_lowercase();
    for c in rust_iso3166::ALL {
        let official = c.name.to_lowercase();
        if contains_word(&official, &lower) || contains_word(&lower, &official) {
            return Some(c.alpha2);
        }
    }
    for (code, names) in NATIVE_ALIASES {
        if names
            .iter()
            .any(|n| contains_word(&lower, n) || contains_word(n, &lower))
        {
            return Some(code);
        }
    }
    None
}

/// Check if two strings match (case-insensitive, with some normalization).
/// Also handles ISO 3166-1 country codes and country name variants in any
/// language covered by [`NATIVE_ALIASES`].
pub fn fuzzy_match(a: &str, b: &str) -> bool {
    let normalize = |s: &str| s.to_lowercase().replace(['-', '_'], " ").trim().to_string();

    let a_norm = normalize(a);
    let b_norm = normalize(b);

    // Exact match after normalization
    if a_norm == b_norm {
        return true;
    }

    // Both sides are unambiguous country designations: the codes decide —
    // equal is a match, different is a definitive non-match ("Sudan" vs
    // "South Sudan" must not fall through to substring containment).
    if let (Some(ca), Some(cb)) = (country_code_exact(a), country_code_exact(b)) {
        return ca == cb;
    }

    // Lenient country resolution for compound strings (display names):
    // both sides resolving to the same alpha-2 code is a match. Runs
    // before the containment shortcut so short codes never
    // substring-match unrelated names ("CH" in "China").
    if let (Some(ca), Some(cb)) = (country_code_of(a), country_code_of(b))
        && ca == cb
    {
        return true;
    }

    // One contains the other as a whole word (e.g. "Paris" in "Paris, France")
    if contains_word(&a_norm, &b_norm) || contains_word(&b_norm, &a_norm) {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_match_exact() {
        assert!(fuzzy_match("France", "France"));
        assert!(fuzzy_match("france", "FRANCE"));
    }

    #[test]
    fn test_fuzzy_match_contains() {
        assert!(fuzzy_match("Paris", "Paris, France"));
        assert!(fuzzy_match("New York City", "New York"));
    }

    #[test]
    fn test_fuzzy_match_normalized() {
        // Hyphens and underscores are normalized to spaces
        assert!(fuzzy_match("Ile-de-France", "Ile de France"));
        assert!(fuzzy_match("New_York", "new york"));
    }

    #[test]
    fn test_fuzzy_match_no_match() {
        assert!(!fuzzy_match("France", "Germany"));
        assert!(!fuzzy_match("Paris", "London"));
    }

    #[test]
    fn test_fuzzy_match_iso_country_codes() {
        // Swiss codes
        assert!(fuzzy_match("CH", "Switzerland"));
        assert!(fuzzy_match("CH", "Schweiz"));
        assert!(fuzzy_match("CH", "Schweiz/Suisse/Svizzera/Svizra"));

        // Japanese codes
        assert!(fuzzy_match("JP", "Japan"));
        assert!(fuzzy_match("JP", "日本"));

        // German codes
        assert!(fuzzy_match("DE", "Germany"));
        assert!(fuzzy_match("DE", "Deutschland"));

        // US codes
        assert!(fuzzy_match("US", "United States"));
        assert!(fuzzy_match("US", "USA"));

        // Reverse order should also work
        assert!(fuzzy_match("Switzerland", "CH"));
        assert!(fuzzy_match("日本", "JP"));

        // Different names for the same country should match
        assert!(fuzzy_match("Japan", "日本"));
        assert!(fuzzy_match("Deutschland", "Germany"));
        assert!(fuzzy_match("Schweiz", "Switzerland"));
    }

    #[test]
    fn test_fuzzy_match_full_iso_coverage() {
        // Countries beyond the alias overlay resolve via rust_iso3166 —
        // these all reported false mismatches before
        assert!(fuzzy_match("UA", "Ukraine"));
        assert!(fuzzy_match("Ukraine", "Україна"));
        assert!(fuzzy_match("HR", "Croatia"));
        assert!(fuzzy_match("Hrvatska", "Croatia"));
        assert!(fuzzy_match("EE", "Estonia"));
        assert!(fuzzy_match("Eesti", "Estonia"));
        // Official long forms match their common short forms
        assert!(fuzzy_match("US", "United States of America"));
        assert!(fuzzy_match("Bolivia", "Bolivia (Plurinational State of)"));
        // Alpha-3 codes resolve too
        assert!(fuzzy_match("UKR", "Ukraine"));
        // No new false positives
        assert!(!fuzzy_match("UA", "United Arab Emirates"));
        assert!(!fuzzy_match("Niger", "Nigeria"));
        assert!(!fuzzy_match("Sudan", "South Sudan"));
    }

    #[test]
    fn test_fuzzy_match_no_short_code_false_positives() {
        // ISO codes must not substring-match unrelated country names
        assert!(!fuzzy_match("CH", "China"));
        assert!(!fuzzy_match("US", "Russia"));
        assert!(!fuzzy_match("IN", "Argentina"));
        assert!(!fuzzy_match("Ukraine", "United Kingdom"));
    }

    #[test]
    fn test_contains_word() {
        assert!(contains_word("paris, france", "paris"));
        assert!(contains_word("new york city", "new york"));
        assert!(contains_word("schweiz/suisse/svizzera", "schweiz"));
        assert!(!contains_word("china", "ch"));
        assert!(!contains_word("russia", "us"));
        assert!(!contains_word("ukraine", "uk"));
        assert!(!contains_word("jerusalem", "usa"));
        assert!(!contains_word("paris", ""));
    }

    #[test]
    fn test_invalid_coordinates() {
        let mut client = NominatimClient::new("test/1.0");
        let result = client.reverse_geocode(100.0, 0.0);
        assert!(matches!(
            result,
            Err(GeocodeError::InvalidCoordinates { .. })
        ));
    }

    #[test]
    fn test_parse_reverse_response_success() {
        let body = r#"{
            "display_name": "Mettmenalp, Schwändi, Glarus, Schweiz",
            "lat": "46.960566", "lon": "9.099201",
            "address": {"country": "Schweiz", "village": "Schwändi", "state": "Glarus"}
        }"#;
        let place = parse_reverse_response(body).unwrap();
        assert_eq!(place.country.as_deref(), Some("Schweiz"));
        assert_eq!(
            place.city.as_deref(),
            Some("Schwändi"),
            "village used as city fallback"
        );
        assert_eq!(place.state.as_deref(), Some("Glarus"));
        assert!((place.lat - 46.960566).abs() < 1e-9);
    }

    #[test]
    fn test_parse_reverse_response_unable_to_geocode_is_no_results() {
        // Nominatim answers HTTP 200 with an error field for e.g. ocean coords
        let body = r#"{"error": "Unable to geocode"}"#;
        assert!(matches!(
            parse_reverse_response(body),
            Err(GeocodeError::NoResults)
        ));
    }

    #[test]
    fn test_parse_reverse_response_other_error() {
        let body = r#"{"error": "Rate limited"}"#;
        assert!(matches!(
            parse_reverse_response(body),
            Err(GeocodeError::Nominatim(msg)) if msg == "Rate limited"
        ));
    }

    #[test]
    fn test_parse_search_response_success_and_error_envelope() {
        let body = r#"[{
            "display_name": "Paris, France",
            "lat": "48.8566", "lon": "2.3522",
            "address": {"country": "France", "city": "Paris"}
        }]"#;
        let places = parse_search_response(body).unwrap();
        assert_eq!(places.len(), 1);
        assert_eq!(places[0].city.as_deref(), Some("Paris"));

        // Error object instead of the expected array
        let err_obj = r#"{"error": {"code": 400, "message": "Parameter 'q' missing"}}"#;
        assert!(matches!(
            parse_search_response(err_obj),
            Err(GeocodeError::Nominatim(msg)) if msg.contains("missing")
        ));

        let err_str = r#"{"error": "boom"}"#;
        assert!(matches!(
            parse_search_response(err_str),
            Err(GeocodeError::Nominatim(msg)) if msg == "boom"
        ));
    }

    #[test]
    fn test_reverse_geocode_against_mock_server() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let body = r#"{"display_name":"Mettmenalp, Glarus, Schweiz","lat":"46.96","lon":"9.09","address":{"country":"Schweiz","village":"Schwändi","state":"Glarus"}}"#;
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
            request
        });

        let mut client =
            NominatimClient::new("kinsaga-test/1.0").with_base_url(format!("http://{}", addr));
        let place = client.reverse_geocode(46.96, 9.09).unwrap();
        assert_eq!(place.country.as_deref(), Some("Schweiz"));
        assert_eq!(place.city.as_deref(), Some("Schwändi"));

        let request = server.join().unwrap();
        assert!(request.starts_with("GET /reverse?lat=46.96&lon=9.09"));
        // Header names are lowercased on the wire
        assert!(
            request
                .to_lowercase()
                .contains("user-agent: kinsaga-test/1.0")
        );
    }

    fn geocoded(display: &str, country: Option<&str>, city: Option<&str>) -> GeocodedPlace {
        GeocodedPlace {
            display_name: display.to_string(),
            country: country.map(String::from),
            city: city.map(String::from),
            state: None,
            lat: 47.0,
            lon: 9.0,
        }
    }

    #[test]
    fn test_evaluate_location_match() {
        use crate::Location;

        // Country and place both match
        let loc = Location::new("CH").with_place("Mettmenalp");
        let place = geocoded(
            "Mettmenalp, Glarus Süd, Schweiz",
            Some("Schweiz"),
            Some("Mettmenalp"),
        );
        assert_eq!(evaluate_location_match(&loc, &place), (true, Some(true)));

        // Wrong country
        let loc = Location::new("France").with_place("Paris");
        let place = geocoded("Berlin, Deutschland", Some("Deutschland"), Some("Berlin"));
        assert_eq!(evaluate_location_match(&loc, &place), (false, Some(false)));

        // No stored place -> place match is None
        let loc = Location::new("Japan");
        let place = geocoded("Tokyo, 日本", Some("日本"), Some("Tokyo"));
        assert_eq!(evaluate_location_match(&loc, &place), (true, None));

        // Nominatim returned no country -> country cannot match
        let loc = Location::new("France");
        let place = geocoded("somewhere", None, None);
        assert_eq!(evaluate_location_match(&loc, &place), (false, None));
    }

    #[test]
    fn test_suggestion_query() {
        assert_eq!(suggestion_query("France", Some("Paris")), "Paris, France");
        assert_eq!(suggestion_query("France", None), "France");
    }

    #[test]
    fn test_count_and_apply_suggestions() {
        use crate::{Category, Fact, Location, Person};

        let mut chronicle = Chronicle::new("1.0");
        chronicle.categories.push(Category::new("travel", "Travel"));
        let mut alice = Person::new("alice", "Alice");
        let mut with_coords = Fact::new("uuid-1", "2020", "travel", "Trip A");
        with_coords.location = Some(
            Location::new("France")
                .with_place("Paris")
                .with_coordinates(Coordinates::new(48.8566, 2.3522)),
        );
        alice.facts.push(with_coords);
        let mut without_coords = Fact::new("uuid-2", "2021", "travel", "Trip B");
        without_coords.location = Some(Location::new("Japan").with_place("Tokyo"));
        alice.facts.push(without_coords);
        alice
            .facts
            .push(Fact::new("uuid-3", "2022", "travel", "No location"));
        chronicle.persons.push(alice);

        assert_eq!(count_facts_with_coordinates(&chronicle), 1);
        assert_eq!(count_facts_without_coordinates(&chronicle), 1);

        let suggestion = GpsSuggestion {
            fact_id: "uuid-2".to_string(),
            person_name: "Alice".to_string(),
            query: "Tokyo, Japan".to_string(),
            candidates: vec![geocoded("Tokyo, 日本", Some("日本"), Some("Tokyo"))],
        };
        let applied = apply_suggestions(&mut chronicle, &[suggestion]);
        assert_eq!(applied, 1);

        let coords = chronicle.persons[0].facts[1]
            .location
            .as_ref()
            .unwrap()
            .coordinates
            .as_ref()
            .unwrap();
        assert_eq!(coords.lat, 47.0);
        assert_eq!(coords.lon, 9.0);
        // Facts without location or with existing coordinates untouched
        assert!(chronicle.persons[0].facts[2].location.is_none());
    }
}
