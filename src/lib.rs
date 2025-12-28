//! # Kinsaga
//!
//! A family chronicle library for managing timestamped, categorized life events.
//!
//! Kinsaga provides data structures and utilities for creating and managing
//! family chronicles where life events are:
//! - **Timestamped** with flexible date precision (year, month, day, or uncertain)
//! - **Categorized** into customizable life themes
//! - **Person-centric** - each family member has their own timeline
//!
//! ## Example
//!
//! ```rust
//! use kinsaga::model::{Chronicle, Person, Fact, Category};
//!
//! let mut chronicle = Chronicle::new("1.0");
//! chronicle.title = Some("Family Chronicle".into());
//!
//! chronicle.categories.push(
//!     Category::new("family", "Family & Friends").with_color("#E57373")
//! );
//!
//! let mut person = Person::new("alice", "Alice Smith");
//! person.facts.push(Fact::new(
//!     "uuid-1",
//!     "1990-05-15",
//!     "family",
//!     "Born in Springfield",
//! ));
//! chronicle.persons.push(person);
//!
//! let json = serde_json::to_string_pretty(&chronicle).unwrap();
//! println!("{}", json);
//! ```

pub mod date;
pub mod filter;
pub mod io;
pub mod model;

pub use date::ChronicleDate;
pub use filter::{search, FactFilter, SearchResult};
pub use io::{load, save};
pub use model::{Attachment, Category, Chronicle, Coordinates, Fact, Location, Person};

// Re-export url::Url for convenience
pub use url::Url;
