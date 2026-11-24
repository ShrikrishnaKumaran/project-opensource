//! Unit tests for the public SASTRA website scrapers.
//!
//! Each test uses a local HTML fixture from the `fixtures/` directory at the
//! workspace root, so no network access is required.

#[cfg(test)]
mod tests {
    use html_scraper::Html;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn fixture(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()   // crates/
            .unwrap()
            .parent()   // workspace root
            .unwrap()
            .join("fixtures")
            .join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Cannot read fixture {name}: {e}"))
    }

    // ── Banner tests ──────────────────────────────────────────────────────────

    #[test]
    fn test_banner_selectors_find_academic_updates() {
        let html = fixture("sastra_home.html");
        let document = Html::parse_document(&html);

        let items = crate::public::banner::extract_banner_pub(
            &document,
            &["#academicupdates a", "marquee:first-of-type a"],
        );

        assert!(!items.is_empty(), "Should find academic update items");
        assert!(
            items.iter().any(|i| i.title.contains("Results")),
            "Should contain the Results item"
        );
    }

    #[test]
    fn test_banner_selectors_find_news_events() {
        let html = fixture("sastra_home.html");
        let document = Html::parse_document(&html);

        let items = crate::public::banner::extract_banner_pub(
            &document,
            &["#newsevents a", "marquee:last-of-type a"],
        );

        assert!(!items.is_empty(), "Should find news & events items");
        assert!(
            items.iter().any(|i| i.title.contains("TECHFEST")),
            "Should contain TECHFEST event"
        );
    }

    #[test]
    fn test_banner_url_resolution() {
        // Relative URL should become absolute.
        let resolved = crate::public::banner::resolve_url_pub("/news/item.html");
        assert!(resolved.starts_with("https://www.sastra.edu"));

        // Already absolute URL stays unchanged.
        let abs = "https://www.sastra.edu/news/item.html";
        assert_eq!(crate::public::banner::resolve_url_pub(abs), abs);
    }

    // ── Curriculum tests ──────────────────────────────────────────────────────

    #[test]
    fn test_curriculum_index_parses_prog_links() {
        let html = fixture("sastra_curriculum.html");
        let results = crate::public::curriculum::regex_fallback_index_pub(&html);

        assert_eq!(results.len(), 3, "Should find 3 program links");
        assert!(results.iter().any(|(_, prog, _, _)| prog == "BCSE"));
        assert!(results.iter().any(|(school, _, _, _)| school == "SoC"));
    }

    #[test]
    fn test_prog_url_parsing() {
        let url = "https://apps.sastra.edu/prog/SoC/BCSE/202122/";
        let parsed = crate::public::curriculum::parse_prog_url_pub(url);
        assert!(parsed.is_some());
        let (school, prog, batch, _) = parsed.unwrap();
        assert_eq!(school, "SoC");
        assert_eq!(prog, "BCSE");
        assert_eq!(batch, "202122");
    }

    #[test]
    fn test_prog_url_parsing_invalid() {
        let url = "https://www.sastra.edu/academics/curriculum.html";
        assert!(crate::public::curriculum::parse_prog_url_pub(url).is_none());
    }

    // ── Faculty tests ─────────────────────────────────────────────────────────

    #[test]
    fn test_faculty_card_extraction() {
        let html = fixture("sastra_faculty_soc.html");
        let document = Html::parse_document(&html);

        let members = crate::public::faculty::extract_faculty_from_document_pub(
            &document,
            "https://www.sastra.edu/staffprofiles/schools/soc.php",
        );

        assert!(!members.is_empty(), "Should find faculty members");
        assert_eq!(members.len(), 3, "Should find exactly 3 faculty");
        assert!(members.iter().any(|f| f.name.contains("Rajesh")));
        assert!(members.iter().any(|f| f.email.is_some()));
        assert!(members.iter().any(|f| f.photo_url.is_some()));
    }

    #[test]
    fn test_faculty_designation_extracted() {
        let html = fixture("sastra_faculty_soc.html");
        let document = Html::parse_document(&html);
        let members = crate::public::faculty::extract_faculty_from_document_pub(
            &document,
            "https://www.sastra.edu/staffprofiles/schools/soc.php",
        );
        let head = members.iter().find(|f| f.name.contains("Rajesh")).unwrap();
        assert!(head.designation.contains("Professor"));
    }

    // ── Calendar tests ────────────────────────────────────────────────────────

    #[test]
    fn test_calendar_table_parsing() {
        let html = fixture("sastra_calendar.html");
        let document = Html::parse_document(&html);
        let entries = crate::public::calendar::parse_calendar_table_pub(&document);

        assert!(!entries.is_empty(), "Should find calendar entries");
        assert!(
            entries.iter().any(|e| e.event.contains("Odd Semester")),
            "Should contain semester commencement"
        );
        assert!(
            entries.iter().any(|e| e.event.contains("End Semester")),
            "Should contain exam entry"
        );
    }

    #[test]
    fn test_calendar_pdf_link_extraction() {
        let html = fixture("sastra_calendar.html");
        let document = Html::parse_document(&html);
        let pdfs = crate::public::calendar::collect_calendar_pdfs_pub(&document);

        assert!(!pdfs.is_empty(), "Should find at least one calendar PDF link");
        assert!(
            pdfs[0].1.contains(".pdf"),
            "URL should end with .pdf"
        );
    }

    #[test]
    fn test_calendar_sno_rows_skipped() {
        let html = fixture("sastra_calendar.html");
        let document = Html::parse_document(&html);
        let entries = crate::public::calendar::parse_calendar_table_pub(&document);

        // The header row (S.No / Event / Date) must be skipped.
        assert!(
            entries.iter().all(|e| e.event != "Event"),
            "Header row should be filtered out"
        );
        assert!(
            entries.iter().all(|e| e.event != "S.No"),
            "S.No header row should be filtered out"
        );
    }
}
