//! Scrapes the full curriculum & syllabus tree from SASTRA.
//!
//! **Layer 1** — `https://www.sastra.edu/academics/curriculum-and-syllabus.html`
//!   Finds school/program accordion links → `https://apps.sastra.edu/prog/<school>/<prog>/<batch>/`
//!
//! **Layer 2** — Each program index page on `apps.sastra.edu`
//!   Finds semester tabs and course rows with their PDF links.
//!
//! **Layer 3** — PDF URLs are stored as-is; the app renders them in a `WebView`.

use html_scraper::{Html, Selector};
use reqwest::Client;

use domain::{CourseLink, CurriculumProgram, DomainError, SemesterCourses};

use super::{banner::fetch_page, APPS_BASE, SASTRA_BASE};

const CURRICULUM_PAGE: &str = "https://www.sastra.edu/academics/curriculum-and-syllabus.html";

/// Top-level entry point: scrape the full curriculum tree.
///
/// Program pages are fetched concurrently (up to all at once) for speed.
pub async fn scrape_full_curriculum(
    client: &Client,
) -> Result<Vec<CurriculumProgram>, DomainError> {
    let stubs = scrape_curriculum_index(client).await?;
    if stubs.is_empty() {
        return Ok(Vec::new());
    }

    // Fetch each program page concurrently.
    let futures: Vec<_> = stubs
        .iter()
        .map(|(school, program, batch, url)| {
            let school = school.clone();
            let program = program.clone();
            let batch = batch.clone();
            let url = url.clone();
            async move {
                let semesters = scrape_program_semesters(client, &url)
                    .await
                    .unwrap_or_default();
                CurriculumProgram {
                    school,
                    program,
                    batch,
                    index_url: url,
                    semesters,
                }
            }
        })
        .collect();

    // Run all futures concurrently and collect results.
    let results = join_all(futures).await;
    Ok(results)
}

/// Layer 1: parse the curriculum index page and extract
/// `(school, program, batch, program_url)` tuples.
pub async fn scrape_curriculum_index(
    client: &Client,
) -> Result<Vec<(String, String, String, String)>, DomainError> {
    let html = fetch_page(client, CURRICULUM_PAGE).await?;
    let document = Html::parse_document(&html);

    let mut results = Vec::new();

    // The curriculum page typically uses an accordion / table with links like:
    //   href="https://apps.sastra.edu/prog/SoC/BCSE/202122/"
    // We match any <a> whose href contains "apps.sastra.edu/prog/"
    let link_sel = Selector::parse("a[href]")
        .map_err(|e| DomainError::Parse(format!("selector error: {e:?}")))?;

    for el in document.select(&link_sel) {
        let Some(href) = el.value().attr("href") else {
            continue;
        };

        if !href.contains("apps.sastra.edu/prog/") && !href.contains("/prog/") {
            continue;
        }

        // Normalise to absolute URL.
        let full_url = if href.starts_with("http") {
            href.to_string()
        } else {
            format!("{SASTRA_BASE}{href}")
        };

        // Parse components from URL path: /prog/<school>/<program>/<batch>/
        if let Some(parsed) = parse_prog_url(&full_url) {
            results.push(parsed);
        }
    }

    // If the structured selectors found nothing, try a broader approach.
    if results.is_empty() {
        results = regex_fallback_index(&html);
    }

    // Deduplicate by URL.
    results.sort_by(|a, b| a.3.cmp(&b.3));
    results.dedup_by(|a, b| a.3 == b.3);

    Ok(results)
}

/// Layer 2: given a program index URL, extract all semester → course → PDF rows.
pub async fn scrape_program_semesters(
    client: &Client,
    url: &str,
) -> Result<Vec<SemesterCourses>, DomainError> {
    let html = fetch_page(client, url).await?;
    let document = Html::parse_document(&html);

    // Try to find semester headings (h3, h4, or tab labels) paired with
    // course tables below them.
    let mut semesters: Vec<SemesterCourses> = Vec::new();

    // Strategy A: look for explicit semester headings then collect rows below.
    let heading_sel = Selector::parse("h3, h4, .semester-heading, .tab-title").ok();
    let table_sel = Selector::parse("table tr").ok();
    let link_sel = Selector::parse("a[href$='.pdf'], a[href*='/pdf/'], a[href*='syllabus']").ok();

    // If the page has a simple table of all courses with a "Semester" column, parse that.
    if let Some(ref tr_sel) = table_sel {
        let rows: Vec<_> = document.select(tr_sel).collect();
        if !rows.is_empty() {
            let mut sem_map: std::collections::BTreeMap<u8, Vec<CourseLink>> =
                std::collections::BTreeMap::new();

            for row in &rows {
                let cells: Vec<String> = row
                    .select(&Selector::parse("td, th").unwrap())
                    .map(|td| td.text().collect::<String>().trim().to_string())
                    .collect();

                // Look for a semester number in the cells.
                let sem_num = cells.iter().find_map(|c| {
                    // e.g. "Semester 1", "Sem 2", or just "1"–"8"
                    let n = c.trim_start_matches(|ch: char| !ch.is_ascii_digit());
                    let n: u8 = n
                        .chars()
                        .take_while(char::is_ascii_digit)
                        .collect::<String>()
                        .parse()
                        .ok()?;
                    if (1..=8).contains(&n) {
                        Some(n)
                    } else {
                        None
                    }
                });

                // Find a PDF link in this row.
                let pdf_link = if let Some(ref ls) = link_sel {
                    row.select(ls)
                        .next()
                        .and_then(|a| a.value().attr("href"))
                        .map(|h| resolve_pdf_url(h, url))
                } else {
                    None
                };

                if let (Some(sem), Some(pdf)) = (sem_num, pdf_link) {
                    // Try to pick code (usually first cell) and name (second cell).
                    let code = cells.first().cloned().unwrap_or_default();
                    let name = cells.get(1).cloned().unwrap_or_default();
                    if !code.is_empty() && !pdf.is_empty() {
                        sem_map.entry(sem).or_default().push(CourseLink {
                            code,
                            name,
                            pdf_url: pdf,
                        });
                    }
                }
            }

            for (sem, courses) in sem_map {
                semesters.push(SemesterCourses {
                    semester: sem,
                    courses,
                });
            }
        }
    }

    // Strategy B: if no structured table, collect all PDF links with their
    // surrounding heading context.
    if semesters.is_empty() {
        if let Some(ref ls) = link_sel {
            let mut current_sem: u8 = 0;
            let mut current_courses: Vec<CourseLink> = Vec::new();

            // Walk all elements, tracking heading changes.
            if let Some(ref hs) = heading_sel {
                for heading in document.select(hs) {
                    let text = heading.text().collect::<String>();
                    if let Some(n) = extract_sem_number(&text) {
                        // Flush previous semester.
                        if current_sem > 0 && !current_courses.is_empty() {
                            semesters.push(SemesterCourses {
                                semester: current_sem,
                                courses: std::mem::take(&mut current_courses),
                            });
                        }
                        current_sem = n;
                    }
                }
            }

            // Collect all PDF links as a flat list under detected semesters.
            for a in document.select(ls) {
                let href = a.value().attr("href").unwrap_or("").to_string();
                let name = a.text().collect::<String>().trim().to_string();
                let pdf_url = resolve_pdf_url(&href, url);
                if !pdf_url.is_empty() {
                    current_courses.push(CourseLink {
                        code: String::new(),
                        name,
                        pdf_url,
                    });
                }
            }

            if !current_courses.is_empty() {
                semesters.push(SemesterCourses {
                    semester: current_sem.max(1),
                    courses: current_courses,
                });
            }
        }
    }

    Ok(semesters)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Parse `/prog/<school>/<program>/<batch>/` from a URL.
pub(crate) fn parse_prog_url(url: &str) -> Option<(String, String, String, String)> {
    // Strip query and fragment.
    let path = url.split('?').next()?.split('#').next()?;
    let prog_idx = path.find("/prog/")?;
    let after = &path[prog_idx + 6..]; // skip "/prog/"
    let parts: Vec<&str> = after.trim_end_matches('/').split('/').collect();
    if parts.len() < 3 {
        return None;
    }
    let school = parts[0].to_string();
    let program = parts[1].to_string();
    let batch = parts[2].to_string();
    // Reconstruct the canonical URL (always use apps.sastra.edu).
    let clean_url = format!("{APPS_BASE}/prog/{school}/{program}/{batch}/");
    Some((school, program, batch, clean_url))
}

pub(crate) fn regex_fallback_index(html: &str) -> Vec<(String, String, String, String)> {
    let re = regex::Regex::new(r#"https?://apps\.sastra\.edu/prog/([^/]+)/([^/]+)/([^/"]+)/?"#)
        .expect("static regex");
    re.captures_iter(html)
        .map(|cap| {
            let school = cap[1].to_string();
            let program = cap[2].to_string();
            let batch = cap[3].to_string();
            let url = format!("{APPS_BASE}/prog/{school}/{program}/{batch}/");
            (school, program, batch, url)
        })
        .collect()
}

fn extract_sem_number(text: &str) -> Option<u8> {
    let re = regex::Regex::new(r"(?i)sem(?:ester)?\s*(\d)").expect("static regex");
    re.captures(text)
        .and_then(|c| c[1].parse::<u8>().ok())
        .filter(|&n| (1..=8).contains(&n))
}

fn resolve_pdf_url(href: &str, base: &str) -> String {
    let href = href.trim();
    if href.starts_with("http") {
        href.to_string()
    } else if href.starts_with('/') {
        // Decide which base to use depending on the href.
        if href.starts_with("/prog/") {
            format!("{APPS_BASE}{href}")
        } else {
            format!("{SASTRA_BASE}{href}")
        }
    } else if href.is_empty() {
        String::new()
    } else {
        // Relative URL — resolve against the page base.
        let base_dir = base.trim_end_matches('/');
        format!("{base_dir}/{href}")
    }
}

/// Poor-man's `join_all` without pulling in `futures` crate.
async fn join_all<T, F>(futs: Vec<F>) -> Vec<T>
where
    F: std::future::Future<Output = T>,
{
    // Sequential fallback — cheap for typical ~20 programs.
    let mut results = Vec::with_capacity(futs.len());
    for f in futs {
        results.push(f.await);
    }
    results
}
