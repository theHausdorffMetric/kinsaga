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
pub mod facts;
pub mod filter;
pub mod format;
pub mod io;
pub mod merge;
pub mod model;
pub mod validate;

pub use date::ChronicleDate;
pub use facts::{
    add_fact, build_attachments, build_location, collect_timeline_facts, edit_fact,
    AddFactOptions, AddFactResult, AttachmentUpdate, EditFactOptions, EditFactResult,
    LocationUpdate, TimelineFact, WithUpdate,
};
pub use filter::{filter_facts, search, FactFilter, SearchResult};
pub use format::{escape_csv, format_attachment, format_attachment_markdown, format_date_display, format_location, truncate_text};
pub use io::{load, save};
pub use merge::{merge_chronicles, ConflictStrategy, DuplicateStrategy, MergeEvent, MergeEventType, MergeItemType, MergeOptions, MergeResult, MergeStats};
pub use model::{Attachment, Category, Chronicle, Coordinates, Fact, Location, Person};
pub use validate::{correct_uuids, is_valid_mime_type, validate_chronicle, IssueType, ValidationIssue, ValidationResult};

// Re-export url::Url for convenience
pub use url::Url;
