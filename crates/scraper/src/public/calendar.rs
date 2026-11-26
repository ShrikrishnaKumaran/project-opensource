//! Scrapes the academic calendar from SASTRA.
//!
//! Target: `https://www.sastra.edu/academics/calendar.html`
//!
//! The page may contain:
//!   a) An HTML `<table>` with event names and date ranges
//!   b) Direct PDF download links for the full calendar
//!   c) Both

use html_scraper::{Html, Selector};
use reqwest::Client;

use domain::{CalendarEntry, DomainError};

use super::{banner::fetch_page, SASTRA_BASE};

const CALENDAR_PAGE: &str = "https://www.sastra.edu/academics/calendar.html";

/// Fetch and parse the academic calendar page.
///
/// Returns a list of [`CalendarEntry`] items.  If a PDF link is available for
/// the whole calendar, the first entry will carry that link.
pub async fn scrape_academic_calendar(client: &Client) -> Result<Vec<CalendarEntry>, DomainError> {
    let html = fetch_page(client, CALENDAR_PAGE).await?;
    let document = Html::parse_document(&html);

    let mut entries: Vec<CalendarEntry> = Vec::new();

    // ── Step 1: collect PDF links for calendar downloads ─────────────────────
    let pdf_links = collect_calendar_pdfs(&document);

    // If there are PDF links, add them as top-level entries.
    for (label, url) in &pdf_links {
        entries.push(CalendarEntry {
            event: label.clone(),
            date_range: String::new(),
            pdf_url: Some(url.clone()),
        });
    }

    // ── Step 2: parse any embedded HTML table of events ──────────────────────
    let table_entries = parse_calendar_table(&document);
    entries.extend(table_entries);

    // ── Step 3: regex fallback if both methods returned nothing ───────────────
    if entries.is_empty() {
        entries = regex_fallback_calendar(&html);
    }

    Ok(entries)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Collect `(label, absolute_url)` pairs for calendar PDF links.
fn collect_calendar_pdfs(document: &Html) -> Vec<(String, String)> {
    let Ok(link_sel) = Selector::parse("a[href$='.pdf'], a[href*='calendar'], a[href*='Calendar']")
    else {
        return Vec::new();
    };

    document
        .select(&link_sel)
        .filter_map(|el| {
            let href = el.value().attr("href")?;
            let label = el.text().collect::<String>().trim().to_string();
            let label = if label.is_empty() {
                // Derive a label from the URL filename.
                href.rsplit('/').next().unwrap_or("Calendar").to_string()
            } else {
                label
            };
            let url = resolve_url(href);
            Some((label, url))
        })
        .collect()
}

/// Parse an embedded `<table>` where rows have event name + date range columns.
fn parse_calendar_table(document: &Html) -> Vec<CalendarEntry> {
    let Ok(row_sel) = Selector::parse("table tr") else {
        return Vec::new();
    };
    let cell_sel = Selector::parse("td, th").unwrap();
    let link_sel = Selector::parse("a[href]").unwrap();
    let tag_re = regex::Regex::new(r"<[^>]+>").expect("static regex");

    let mut entries = Vec::new();

    for row in document.select(&row_sel) {
        let cells: Vec<String> = row
            .select(&cell_sel)
            .map(|td| tag_re.replace_all(&td.inner_html(), "").trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // Expect at least two columns: event and date.
        if cells.len() < 2 {
            continue;
        }

        let event = cells[0].clone();
        // Skip pure header rows.
        if event.to_lowercase().contains("event") && event.to_lowercase().contains("date") {
            continue;
        }
        if event.to_lowercase() == "s.no"
            || event.to_lowercase() == "sl.no"
            || event.to_lowercase() == "#"
        {
            continue;
        }

        let date_range = cells.get(1).cloned().unwrap_or_default();

        // Check for an inline PDF link in this row.
        let pdf_url = row.select(&link_sel).find_map(|a| {
            let href = a.value().attr("href")?;
            if std::path::Path::new(href)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
                || href.contains("calendar")
            {
                Some(resolve_url(href))
            } else {
                None
            }
        });

        entries.push(CalendarEntry {
            event,
            date_range,
            pdf_url,
        });
    }

    entries
}

/// Last-resort: scan raw HTML for date-like patterns near event keywords.
fn regex_fallback_calendar(html: &str) -> Vec<CalendarEntry> {
    // Match lines like: "Commencement of Odd Semester — 01 July 2025"
    let re = regex::Regex::new(
        r"(?i)([A-Z][^<\n]{5,80}?)[\s\-–—:]+(\d{1,2}[\s\-/]\w{3,9}[\s\-/]\d{4}(?:\s+to\s+\d{1,2}[\s\-/]\w{3,9}[\s\-/]\d{4})?)",
    )
    .expect("static regex");

    let tag_re = regex::Regex::new(r"<[^>]+>").expect("static regex");
    let clean = tag_re.replace_all(html, " ");

    re.captures_iter(&clean)
        .map(|cap| CalendarEntry {
            event: cap[1].trim().to_string(),
            date_range: cap[2].trim().to_string(),
            pdf_url: None,
        })
        .take(50)
        .collect()
}

fn resolve_url(href: &str) -> String {
    let href = href.trim();
    if href.starts_with("http") {
        href.to_string()
    } else if href.starts_with('/') {
        format!("{SASTRA_BASE}{href}")
    } else {
        format!("{SASTRA_BASE}/{href}")
    }
}
