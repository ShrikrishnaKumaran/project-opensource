//! Public SASTRA website scrapers (sastra.edu — no login required).
//!
//! Modules:
//! - [`banner`]     — homepage academic-updates & news-events banners
//! - [`curriculum`] — curriculum & syllabus PDF links (all 8 semesters)
//! - [`faculty`]    — faculty directory across all schools
//! - [`calendar`]   — academic calendar events

pub mod banner;
pub mod calendar;
pub mod curriculum;
pub mod faculty;

use domain::{DomainError, PublicData, PublicSource};
use reqwest::{
    header::{HeaderMap, HeaderValue},
    redirect::Policy,
    Client,
};
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

pub const SASTRA_BASE: &str = "https://www.sastra.edu";
pub const APPS_BASE: &str = "https://apps.sastra.edu";
pub const PUBLIC_UA: &str = "Mozilla/5.0 (Linux; Android 14; Pixel 9) \
                             AppleWebKit/537.36 (KHTML, like Gecko) \
                             Chrome/135.0.0.0 Mobile Safari/537.36 \
                             NUTS/1.0 (sastra student app)";

/// Build a lightweight HTTP client for public (unauthenticated) requests.

pub fn public_client() -> Client {
    let mut headers = HeaderMap::new();
    headers.insert(
        "accept",
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        ),
    );
    headers.insert(
        "accept-language",
        HeaderValue::from_static("en-US,en;q=0.9"),
    );

    Client::builder()
        // 1. Force IPv4 to avoid broken AAAA routes
        .local_address(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
        // 2. Limit idle/active pool pressure on sastra.edu's server
        .pool_max_idle_per_host(2)
        .danger_accept_invalid_certs(true)
        .http1_only()
        .redirect(Policy::limited(5))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30))
        .default_headers(headers)
        .user_agent(PUBLIC_UA)
        .build()
        .expect("failed to build public HTTP client")
}


/// Concrete implementation of [`PublicSource`] for the SASTRA public website.
#[derive(Clone)]
pub struct SastraPublicSource {
    client: Client,
}

impl Default for SastraPublicSource {
    fn default() -> Self {
        Self::new()
    }
}

impl SastraPublicSource {
    pub fn new() -> Self {
        Self {
            client: public_client(),
        }
    }

    /// Fetch all public data concurrently and return a combined [`PublicData`].
    ///
    /// Individual section failures are logged but do not abort the whole fetch —
    /// the caller always gets a (possibly partial) result.
    pub async fn fetch_public_data(&self) -> Result<PublicData, DomainError> {
        // Run all four scrapers concurrently.
        let (banners_res, curriculum_res, faculty_res, calendar_res) = tokio::join!(
            banner::scrape_homepage_banners(&self.client),
            curriculum::scrape_full_curriculum(&self.client),
            faculty::scrape_all_faculty(&self.client),
            calendar::scrape_academic_calendar(&self.client),
        );

        let (academic_updates, news_events) = match banners_res {
            Ok(data) => {
                println!(
                    "[NUTS] Homepage banners: {} academic, {} news",
                    data.0.len(),
                    data.1.len()
                );
                data
            }
            Err(e) => {
                eprintln!("[NUTS] Homepage banner scraping failed: {e}");
                (Vec::new(), Vec::new())
            }
        };

        Ok(PublicData {
            academic_updates,
            news_events,
            curriculum: curriculum_res.unwrap_or_default(),
            faculty: faculty_res.unwrap_or_default(),
            calendar: calendar_res.unwrap_or_default(),
        })
    }
}

impl PublicSource for SastraPublicSource {
    async fn fetch_public_data(&self) -> Result<PublicData, DomainError> {
        Self::fetch_public_data(self).await
    }
}
