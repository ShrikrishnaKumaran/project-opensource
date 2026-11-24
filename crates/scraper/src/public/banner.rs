// Scrapes the Academic Updates and News / Events sections
// from the public SASTRA homepage.

use html_scraper::{Html, Selector};
use reqwest::Client;

use domain::{BannerItem, DomainError};

use super::SASTRA_BASE;

/// Fetch and parse the SASTRA homepage.
///
/// Returns:
/// (academic_updates, news_events)
pub async fn scrape_homepage_banners(
    client: &Client,
) -> Result<(Vec<BannerItem>, Vec<BannerItem>), DomainError> {
    let html = fetch_page(client, "https://sastra.edu/").await?;

    println!("[NUTS] Homepage HTML received: {} bytes", html.len());

    let document = Html::parse_document(&html);

    let (academic_updates, news_events) = extract_homepage_banners(&document);

    println!(
        "[NUTS] Extracted {} academic updates and {} news events",
        academic_updates.len(),
        news_events.len()
    );

    Ok((academic_updates, news_events))
}

/// Extract Academic Updates and News / Events.
///
/// We locate each `.newseventcontent` section independently,
/// then extract its heading and article links.
pub(crate) fn extract_homepage_banners(document: &Html) -> (Vec<BannerItem>, Vec<BannerItem>) {
    let section_selector = Selector::parse(".newseventcontent").expect("valid section selector");

    let heading_selector = Selector::parse("h1").expect("valid heading selector");

    let link_selector =
        Selector::parse("a.mod-articles-category-title").expect("valid link selector");

    let mut academic_updates = Vec::new();
    let mut news_events = Vec::new();

    for section in document.select(&section_selector) {
        let heading = section
            .select(&heading_selector)
            .next()
            .map(|element| element.text().collect::<String>().trim().to_string());

        let Some(heading) = heading else {
            continue;
        };

        let items: Vec<BannerItem> = section
            .select(&link_selector)
            .filter_map(|element| {
                let title = element.text().collect::<String>().trim().to_string();

                if title.is_empty() {
                    return None;
                }

                let url = element.value().attr("href").map(resolve_url);

                Some(BannerItem { title, url })
            })
            .collect();

        if heading.eq_ignore_ascii_case("Academic Updates") {
            academic_updates = items;
        } else if heading.eq_ignore_ascii_case("News / Events")
            || heading.eq_ignore_ascii_case("News & Events")
        {
            news_events = items;
        }

        println!(
            "[NUTS] Section '{}': {} items",
            heading,
            if heading.eq_ignore_ascii_case("Academic Updates") {
                academic_updates.len()
            } else {
                news_events.len()
            }
        );
    }

    (academic_updates, news_events)
}

/// General-purpose banner extraction helper.
///
/// Tries the supplied CSS selectors in order and returns
/// the first selector that produces results.
pub(crate) fn extract_banner(document: &Html, selectors: &[&str]) -> Vec<BannerItem> {
    for selector_str in selectors {
        if let Ok(selector) = Selector::parse(selector_str) {
            let items: Vec<BannerItem> = document
                .select(&selector)
                .filter_map(|element| {
                    let title = element.text().collect::<String>().trim().to_string();

                    if title.is_empty() {
                        return None;
                    }

                    let url = element.value().attr("href").map(resolve_url);

                    Some(BannerItem { title, url })
                })
                .collect();

            if !items.is_empty() {
                return items;
            }
        }
    }

    Vec::new()
}

/// Resolve a SASTRA relative URL into an absolute URL.
pub(crate) fn resolve_url(href: &str) -> String {
    let href = href.trim();

    if href.starts_with("https://") || href.starts_with("http://") {
        href.to_string()
    } else if href.starts_with("//") {
        format!("https:{href}")
    } else if href.starts_with('/') {
        format!("{SASTRA_BASE}{href}")
    } else if href.is_empty() || href.starts_with('#') {
        SASTRA_BASE.to_string()
    } else {
        format!("{SASTRA_BASE}/{href}")
    }
}

/// Fetch a page and check its HTTP status before parsing.
pub(super) async fn fetch_page(client: &Client, url: &str) -> Result<String, DomainError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| DomainError::Network(format!("GET {url}: {error:?}")))?;

    let status = response.status();

    if !status.is_success() {
        return Err(DomainError::Network(format!("GET {url}: HTTP {status}")));
    }

    let body = response
        .text()
        .await
        .map_err(|error| DomainError::Network(format!("Read body {url}: {error}")))?;

    println!(
        "[NUTS] HTTP {status} from {url}, body size: {} bytes",
        body.len()
    );

    Ok(body)
}

// Test-visible re-exports.
#[cfg(test)]
pub(crate) use self::{extract_banner as extract_banner_pub, resolve_url as resolve_url_pub};
