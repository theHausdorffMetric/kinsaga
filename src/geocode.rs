//! Geocoding functionality using Nominatim (OpenStreetMap).

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
#[derive(Debug, Deserialize)]
struct NominatimReverseResponse {
    display_name: String,
    lat: String,
    lon: String,
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

/// Client for Nominatim geocoding API.
pub struct NominatimClient {
    user_agent: String,
    last_request: Option<Instant>,
    min_interval: Duration,
}

impl NominatimClient {
    /// Create a new Nominatim client.
    ///
    /// The user_agent should identify your application per Nominatim's usage policy.
    pub fn new(user_agent: impl Into<String>) -> Self {
        Self {
            user_agent: user_agent.into(),
            last_request: None,
            min_interval: Duration::from_millis(1100), // Slightly over 1 second to be safe
        }
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

    /// Reverse geocode coordinates to get place information.
    pub fn reverse_geocode(&mut self, lat: f64, lon: f64) -> Result<GeocodedPlace, GeocodeError> {
        // Validate coordinates
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return Err(GeocodeError::InvalidCoordinates { lat, lon });
        }

        self.rate_limit();

        let url = format!(
            "https://nominatim.openstreetmap.org/reverse?lat={}&lon={}&format=json&addressdetails=1",
            lat, lon
        );

        let response = ureq::get(&url)
            .header("User-Agent", &self.user_agent)
            .call()
            .map_err(|e| GeocodeError::HttpError(e.to_string()))?;

        let body = response
            .into_body()
            .read_to_string()
            .map_err(|e| GeocodeError::ParseError(e.to_string()))?;

        let parsed: NominatimReverseResponse =
            serde_json::from_str(&body).map_err(|e| GeocodeError::ParseError(e.to_string()))?;

        let lat_parsed: f64 = parsed
            .lat
            .parse()
            .map_err(|_| GeocodeError::ParseError("Invalid latitude in response".to_string()))?;
        let lon_parsed: f64 = parsed
            .lon
            .parse()
            .map_err(|_| GeocodeError::ParseError("Invalid longitude in response".to_string()))?;

        // Extract city from various possible fields
        let city = parsed.address.as_ref().and_then(|a| {
            a.city
                .clone()
                .or_else(|| a.town.clone())
                .or_else(|| a.village.clone())
        });

        Ok(GeocodedPlace {
            display_name: parsed.display_name,
            country: parsed.address.as_ref().and_then(|a| a.country.clone()),
            city,
            state: parsed.address.as_ref().and_then(|a| a.state.clone()),
            lat: lat_parsed,
            lon: lon_parsed,
        })
    }

    /// Forward geocode a place name to get coordinates.
    ///
    /// Returns up to `limit` results, sorted by relevance.
    pub fn forward_geocode(
        &mut self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<GeocodedPlace>, GeocodeError> {
        self.rate_limit();

        let encoded_query = urlencoding::encode(query);
        let url = format!(
            "https://nominatim.openstreetmap.org/search?q={}&format=json&addressdetails=1&limit={}",
            encoded_query, limit
        );

        let response = ureq::get(&url)
            .header("User-Agent", &self.user_agent)
            .call()
            .map_err(|e| GeocodeError::HttpError(e.to_string()))?;

        let body = response
            .into_body()
            .read_to_string()
            .map_err(|e| GeocodeError::ParseError(e.to_string()))?;

        let results: Vec<NominatimSearchResult> =
            serde_json::from_str(&body).map_err(|e| GeocodeError::ParseError(e.to_string()))?;

        if results.is_empty() {
            return Err(GeocodeError::NoResults);
        }

        let places = results
            .into_iter()
            .filter_map(|r| {
                let lat: f64 = r.lat.parse().ok()?;
                let lon: f64 = r.lon.parse().ok()?;

                let city = r.address.as_ref().and_then(|a| {
                    a.city
                        .clone()
                        .or_else(|| a.town.clone())
                        .or_else(|| a.village.clone())
                });

                Some(GeocodedPlace {
                    display_name: r.display_name,
                    country: r.address.as_ref().and_then(|a| a.country.clone()),
                    city,
                    state: r.address.as_ref().and_then(|a| a.state.clone()),
                    lat,
                    lon,
                })
            })
            .collect();

        Ok(places)
    }
}

/// Result of validating a location's GPS coordinates.
#[derive(Debug)]
pub struct GpsValidationResult {
    /// The fact ID
    pub fact_id: String,
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

/// ISO 3166-1 alpha-2 country codes mapped to common names (including native names).
const COUNTRY_CODES: &[(&str, &[&str])] = &[
    ("ch", &["switzerland", "schweiz", "suisse", "svizzera", "svizra"]),
    ("de", &["germany", "deutschland"]),
    ("at", &["austria", "österreich", "oesterreich"]),
    ("fr", &["france"]),
    ("it", &["italy", "italia"]),
    ("es", &["spain", "españa", "espana"]),
    ("pt", &["portugal"]),
    ("gb", &["united kingdom", "uk", "great britain", "england"]),
    ("us", &["united states", "usa", "america"]),
    ("ca", &["canada"]),
    ("au", &["australia"]),
    ("nz", &["new zealand"]),
    ("jp", &["japan", "日本", "nippon", "nihon"]),
    ("cn", &["china", "中国", "zhongguo"]),
    ("kr", &["south korea", "korea", "대한민국", "한국"]),
    ("in", &["india"]),
    ("br", &["brazil", "brasil"]),
    ("mx", &["mexico", "méxico"]),
    ("ar", &["argentina"]),
    ("nl", &["netherlands", "holland", "nederland"]),
    ("be", &["belgium", "belgique", "belgië", "belgie"]),
    ("pl", &["poland", "polska"]),
    ("cz", &["czech republic", "czechia", "česko", "cesko"]),
    ("se", &["sweden", "sverige"]),
    ("no", &["norway", "norge"]),
    ("dk", &["denmark", "danmark"]),
    ("fi", &["finland", "suomi"]),
    ("ru", &["russia", "россия", "rossiya"]),
    ("gr", &["greece", "ελλάδα", "ellada"]),
    ("tr", &["turkey", "türkiye", "turkiye"]),
    ("ie", &["ireland", "éire", "eire"]),
    ("za", &["south africa"]),
    ("eg", &["egypt", "مصر"]),
    ("il", &["israel", "ישראל"]),
    ("ae", &["united arab emirates", "uae"]),
    ("sg", &["singapore"]),
    ("th", &["thailand", "ประเทศไทย"]),
    ("vn", &["vietnam", "việt nam"]),
    ("id", &["indonesia"]),
    ("my", &["malaysia"]),
    ("ph", &["philippines"]),
];

/// Check if a string matches a country code or any of its names.
fn matches_country_code(code: &str, name: &str) -> bool {
    let code_lower = code.to_lowercase();
    let name_lower = name.to_lowercase();

    for (iso_code, names) in COUNTRY_CODES {
        if code_lower == *iso_code {
            // Code matches, check if name matches any of the country names
            for country_name in *names {
                if name_lower.contains(country_name) || country_name.contains(&name_lower) {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if two country names refer to the same country (via ISO code lookup).
fn same_country(a: &str, b: &str) -> bool {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();

    for (_iso_code, names) in COUNTRY_CODES {
        let a_matches = names.iter().any(|n| a_lower.contains(n) || n.contains(&a_lower));
        let b_matches = names.iter().any(|n| b_lower.contains(n) || n.contains(&b_lower));

        if a_matches && b_matches {
            return true;
        }
    }
    false
}

/// Check if two strings match (case-insensitive, with some normalization).
/// Also handles ISO 3166-1 alpha-2 country codes and country name variants.
pub fn fuzzy_match(a: &str, b: &str) -> bool {
    let normalize = |s: &str| {
        s.to_lowercase()
            .replace(['-', '_'], " ")
            .trim()
            .to_string()
    };

    let a_norm = normalize(a);
    let b_norm = normalize(b);

    // Exact match after normalization
    if a_norm == b_norm {
        return true;
    }

    // One contains the other
    if a_norm.contains(&b_norm) || b_norm.contains(&a_norm) {
        return true;
    }

    // Check if one is an ISO country code matching the other
    if matches_country_code(a, b) || matches_country_code(b, a) {
        return true;
    }

    // Check if both are names for the same country (e.g., "Japan" and "日本")
    if same_country(a, b) {
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
    fn test_invalid_coordinates() {
        let mut client = NominatimClient::new("test/1.0");
        let result = client.reverse_geocode(100.0, 0.0);
        assert!(matches!(result, Err(GeocodeError::InvalidCoordinates { .. })));
    }
}
