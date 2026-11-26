//! Scraper layer for public SASTRA pages and authenticated portal sources.

pub mod portal;
pub mod public;

pub use portal::student::*;
pub use public::SastraPublicSource;
