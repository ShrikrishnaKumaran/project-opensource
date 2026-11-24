//! Scrapes the faculty directory across all SASTRA schools.
//!
//! **Layer 1** — `https://www.sastra.edu/academics/faculty.html`
//!   Extracts per-school profile page links, e.g.:
//!   - SoC → `https://www.sastra.edu/staffprofiles/schools/soc.php`
//!   - SoE → `https://www.sastra.edu/staffprofiles/schools/soe.php`
//!
//! **Layer 2** — Each school profile page
//!   Extracts faculty cards: name, designation, dept, photo, profile URL.
//!
//! **Layer 3 (lazy)** — Individual faculty profile URLs
//!   Fetched only on demand via [`scrape_faculty_detail`].

use html_scraper::{Html, Selector};
use reqwest::Client;

use domain::{DomainError, FacultyDetail, FacultyMember};

use super::{banner::fetch_page, SASTRA_BASE};

const FACULTY_INDEX: &str = "https://www.sastra.edu/academics/faculty.html";

/// Layer 1 + 2: return the full faculty list (across all schools).
///
/// Detail fields are left as `None` — call [`scrape_faculty_detail`] lazily.
pub async fn scrape_all_faculty(client: &Client) -> Result<Vec<FacultyMember>, DomainError> {
    let schools = scrape_faculty_schools(client).await?;

    let mut all: Vec<FacultyMember> = Vec::new();
    for (_school_name, school_url) in &schools {
        let members = scrape_school_faculty(client, school_url)
            .await
            .unwrap_or_default();
        all.extend(members);
    }

    Ok(all)
}

/// Layer 1: parse the faculty index page and return `(school_name, school_url)` pairs.
pub async fn scrape_faculty_schools(client: &Client) -> Result<Vec<(String, String)>, DomainError> {
    let html = fetch_page(client, FACULTY_INDEX).await?;
    let document = Html::parse_document(&html);

    let mut schools: Vec<(String, String)> = Vec::new();

    // The page has links to per-school staff profile pages.
    // Pattern: href contains "staffprofiles/schools/"
    let link_sel =
        Selector::parse("a[href]").map_err(|e| DomainError::Parse(format!("selector: {e:?}")))?;

    for el in document.select(&link_sel) {
        let href = match el.value().attr("href") {
            Some(h) => h,
            None => continue,
        };

        if !href.contains("staffprofiles") && !href.contains("staffprofile") {
            continue;
        }

        let full_url = resolve_url(href);
        let name = el.text().collect::<String>().trim().to_string();
        let name = if name.is_empty() {
            // Fall back to extracting from the URL itself, e.g. "soc" → "SoC"
            href.rsplit('/')
                .find(|s| !s.is_empty())
                .unwrap_or("Unknown")
                .trim_end_matches(".php")
                .to_uppercase()
        } else {
            name
        };

        // Avoid duplicates.
        if !schools.iter().any(|(_, u)| u == &full_url) {
            schools.push((name, full_url));
        }
    }

    // Regex fallback if selectors found nothing.
    if schools.is_empty() {
        let re = regex::Regex::new(r#"href="([^"]*staffprofile[^"]*\.php[^"]*)""#)
            .expect("static regex");
        let name_re = regex::Regex::new(r"<a[^>]*>([^<]+)</a>").expect("static regex");
        for cap in re.captures_iter(&html) {
            let href = &cap[1];
            let url = resolve_url(href);
            let name = name_re
                .captures(&html[cap.get(0).unwrap().start()..])
                .map(|c| c[1].trim().to_string())
                .unwrap_or_else(|| url.rsplit('/').next().unwrap_or("").to_string());
            if !schools.iter().any(|(_, u)| u == &url) {
                schools.push((name, url));
            }
        }
    }

    Ok(schools)
}

/// Layer 2: parse a single school's staff profile page.
pub async fn scrape_school_faculty(
    client: &Client,
    school_url: &str,
) -> Result<Vec<FacultyMember>, DomainError> {
    let html = fetch_page(client, school_url).await?;
    let document = Html::parse_document(&html);

    let mut members: Vec<FacultyMember> = Vec::new();

    // Faculty cards are typically `<div class="faculty-card">` or `<li>` or `<tr>`.
    // We try several selectors in order.
    let card_selectors = [
        "div.faculty-card",
        "div.staff-card",
        "div.faculty-item",
        ".faculty-block",
        ".staffcard",
        "li.faculty",
        "table tr", // some schools use a plain table
    ];

    'outer: for sel_str in &card_selectors {
        if let Ok(sel) = Selector::parse(sel_str) {
            let cards: Vec<_> = document.select(&sel).collect();
            if cards.is_empty() {
                continue;
            }

            for card in cards {
                // --- Name ---
                let name = extract_text_by_selectors(
                    &card,
                    &[
                        "h3",
                        "h4",
                        ".name",
                        ".faculty-name",
                        ".staff-name",
                        "strong",
                        "b",
                    ],
                )
                .unwrap_or_else(|| card.text().collect::<String>().trim().to_string());

                if name.is_empty() || name.to_lowercase().contains("name") {
                    continue; // skip header rows
                }

                // --- Designation ---
                let designation = extract_text_by_selectors(
                    &card,
                    &[".designation", ".title", ".post", "span.role", "em"],
                )
                .unwrap_or_default();

                // --- Department ---
                let department = extract_text_by_selectors(
                    &card,
                    &[".department", ".dept", ".school", "span.dept"],
                )
                .unwrap_or_default();

                // --- Email ---
                let email = card
                    .select(&Selector::parse("a[href^='mailto:']").unwrap())
                    .next()
                    .map(|a| {
                        a.value()
                            .attr("href")
                            .unwrap_or("")
                            .trim_start_matches("mailto:")
                            .to_string()
                    });

                // --- Photo ---
                let photo_url = card
                    .select(&Selector::parse("img").unwrap())
                    .next()
                    .and_then(|img| img.value().attr("src"))
                    .map(|src| resolve_url(src));

                // --- Profile link ---
                let profile_url = card
                    .select(&Selector::parse("a[href]").unwrap())
                    .find_map(|a| {
                        let href = a.value().attr("href")?;
                        if href.contains("staffprofile")
                            || href.contains("faculty")
                            || href.contains(".php")
                        {
                            Some(resolve_url(href))
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| school_url.to_string());

                members.push(FacultyMember {
                    name,
                    designation,
                    department,
                    email,
                    photo_url,
                    profile_url,
                    detail: None,
                });
            }

            if !members.is_empty() {
                break 'outer;
            }
        }
    }

    // If nothing found with structural selectors, do a table-based extraction.
    if members.is_empty() {
        members = table_fallback_faculty(&document, school_url);
    }

    Ok(members)
}

/// Layer 3 (lazy): fetch and parse an individual faculty member's profile page.
pub async fn scrape_faculty_detail(
    client: &Client,
    profile_url: &str,
) -> Result<FacultyDetail, DomainError> {
    let html = fetch_page(client, profile_url).await?;
    let document = Html::parse_document(&html);

    // --- Qualifications ---
    let qualifications = extract_section_text(
        &document,
        &[
            ".qualifications",
            "#qualifications",
            "th:contains('Qualification') + td",
            // Broad fallback: any `<td>` following a "Qualification" label
        ],
    )
    .or_else(|| regex_section(&html, "qualification"))
    .unwrap_or_default();

    // --- Experience ---
    let experience = extract_section_text(
        &document,
        &[
            ".experience",
            "#experience",
            "th:contains('Experience') + td",
        ],
    )
    .or_else(|| regex_section(&html, "experience"))
    .unwrap_or_default();

    // --- Research Areas ---
    let research_areas = extract_list_items(
        &document,
        &[
            ".research-areas li",
            "#research li",
            ".research li",
            "ul.research-interests li",
        ],
    )
    .or_else(|| regex_list_section(&html, "research"))
    .unwrap_or_default();

    // --- Publications ---
    let publications = extract_list_items(
        &document,
        &[
            ".publications li",
            "#publications li",
            "ol.publications li",
            "ul.publications li",
        ],
    )
    .or_else(|| regex_list_section(&html, "publication"))
    .unwrap_or_default();

    Ok(FacultyDetail {
        qualifications,
        experience,
        research_areas,
        publications,
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

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

fn extract_text_by_selectors(
    root: &html_scraper::ElementRef<'_>,
    selectors: &[&str],
) -> Option<String> {
    for sel_str in selectors {
        if let Ok(sel) = Selector::parse(sel_str) {
            if let Some(el) = root.select(&sel).next() {
                let text = el.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}

fn extract_section_text(document: &Html, selectors: &[&str]) -> Option<String> {
    for sel_str in selectors {
        if let Ok(sel) = Selector::parse(sel_str) {
            if let Some(el) = document.select(&sel).next() {
                let text = el.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}

fn extract_list_items(document: &Html, selectors: &[&str]) -> Option<Vec<String>> {
    for sel_str in selectors {
        if let Ok(sel) = Selector::parse(sel_str) {
            let items: Vec<String> = document
                .select(&sel)
                .map(|el| el.text().collect::<String>().trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if !items.is_empty() {
                return Some(items);
            }
        }
    }
    None
}

fn regex_section(html: &str, keyword: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find(keyword)?;
    let window = &html[start..(start + 1500).min(html.len())];

    let td_re = regex::Regex::new(r"(?is)<td[^>]*>(.*?)</td>").expect("static regex");
    let tag_re = regex::Regex::new(r"<[^>]+>").expect("static regex");

    let result = td_re.captures_iter(window).find_map(|cap| {
        let text = tag_re.replace_all(&cap[1], "").trim().to_string();

        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    });

    result
}

fn regex_list_section(html: &str, keyword: &str) -> Option<Vec<String>> {
    let lower = html.to_lowercase();
    let start = lower.find(keyword)?;
    let window = &html[start..(start + 4000).min(html.len())];
    let li_re = regex::Regex::new(r"(?s)<li[^>]*>(.*?)</li>").expect("static regex");
    let tag_re = regex::Regex::new(r"<[^>]+>").expect("static regex");
    let items: Vec<String> = li_re
        .captures_iter(window)
        .filter_map(|cap| {
            let text = tag_re.replace_all(&cap[1], "").trim().to_string();
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        })
        .take(50)
        .collect();
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

/// Last-resort faculty extraction from a plain `<table>`.
fn table_fallback_faculty(document: &Html, school_url: &str) -> Vec<FacultyMember> {
    let tr_sel = Selector::parse("table tr").unwrap();
    let td_sel = Selector::parse("td").unwrap();
    let tag_re = regex::Regex::new(r"<[^>]+>").expect("static regex");

    document
        .select(&tr_sel)
        .filter_map(|row| {
            let cells: Vec<String> = row
                .select(&td_sel)
                .map(|td| tag_re.replace_all(&td.inner_html(), "").trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            if cells.len() < 2 {
                return None;
            }
            let name = cells[0].clone();
            if name.to_lowercase().contains("name") || name.is_empty() {
                return None;
            }
            let designation = cells.get(1).cloned().unwrap_or_default();
            let department = cells.get(2).cloned().unwrap_or_default();

            Some(FacultyMember {
                name,
                designation,
                department,
                email: None,
                photo_url: None,
                profile_url: school_url.to_string(),
                detail: None,
            })
        })
        .collect()
}
