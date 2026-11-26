# Student Portal Scraper Plan

## Goal

Build a reliable Rust scraper for SASTRA student data that can use multiple portal sources and return consistent, typed results to the app.

“100% working” can’t be guaranteed because portals may be unavailable or change their HTML. The implementation should detect failures clearly, avoid presenting failures as valid empty results, and use fallbacks when appropriate.

## Keep the data sources separate

The repository has two existing scraper domains:

- **Public SASTRA site** (`sastra.edu`): general information; keep its current scraper separate and unchanged.
- **Authenticated student portal** (`webstream.sastra.edu/sastrapwi`): student-specific information such as attendance, marks, and grades.

A **parent portal** may be added later as a separate authenticated source. Do not assume it uses the student portal’s credentials, session, endpoints, or HTML.

## Architecture

Organize `crates/scraper/src/` by source and endpoint family:

```text
src/
├── lib.rs
├── public/                       # existing public sastra.edu scraper
└── portal/
    ├── mod.rs                    # source/coordinator exports
    ├── attendance.rs             # shared attendance result + fallback coordination
    ├── student/
    │   ├── mod.rs
    │   ├── client.rs             # student portal HTTP setup and response validation
    │   ├── auth.rs               # captcha and login
    │   ├── resources.rs          # StudentDetailsResources.jsp?resourceid=...
    │   └── academy/
    │       ├── mod.rs
    │       └── attendance.rs     # studentHourWiseAttendance.jsp
    └── parent/                   # later; independent client/auth/parsers
```

Preserve the current crate API where practical, so existing sync code can continue to use `SastraPortalSource` while internals are reorganized.

Keep resource endpoints and `academy/*.jsp` endpoints as separate fetchers and parsers. They are distinct URLs with potentially different response structures. A coordinator should normalize their successful results into one shared attendance type. It should also record which source succeeded and why earlier sources failed for diagnostics.

## Attendance source strategy

Use the existing resource 7 scraper as the first candidate because it is already implemented and returns per-course totals, present, absent, and percentage:

```text
StudentDetailsResources.jsp?resourceid=7
```

Investigate the hourwise page as a separate candidate because it is marked as the most important attendance view:

```text
academy/studentHourWiseAttendance.jsp
```

Compare their real responses before deciding whether the academy page is:

- a more detailed replacement,
- complementary data that should be retained separately, or
- redundant and unnecessary.

If the parent portal has equivalent attendance data, add it as another independent source with its own authentication and parser. Only use it as a fallback when its output can be mapped accurately to the shared attendance model.

A **valid empty result** is not a source failure and should not automatically trigger fallback. Fallback should occur for unavailable endpoints, network errors, expired sessions/login pages, unexpected content, and parse failures.

## Current implementation to preserve and improve

The existing student portal scraper already includes:

- captcha retrieval and login,
- profile,
- resource 3: hostel,
- resource 4: course history,
- resource 7: attendance summary,
- resource 22: internal marks,
- resource 23: exam schedule,
- resource 25: absent hours,
- resource 28: grades and GPA.

The existing `fetch_portal_data()` currently converts many individual scraper errors into empty values using `.ok()`, `.unwrap_or_default()`, and similar fallbacks. This can make a failed request look like a successful empty response. Replace silent swallowing with explicit per-section outcomes or diagnostics while preserving successful data from other sections where possible.

The repository has no portal HTML fixtures for these resource pages or the academy attendance page. The supplied `source.html` and JavaScript identify endpoint URLs and some parameters, but they do not include the loaded endpoint responses.

## Request and response hardening

For every fetch:

1. Preserve the authenticated session cookie and required headers.
2. Check HTTP status before parsing.
3. Detect redirects to login and HTML login/error pages returned with status 200.
4. Validate content type or recognizable page structure.
5. Return typed network, auth/session, HTTP, and parse errors.
6. Do not silently turn invalid or failed responses into empty vectors.
7. Keep requests to state-changing endpoints out of routine sync.

Use bounded timeouts and retry only transient network/server failures. Avoid repeating requests for the same endpoint during one sync; parse one response into all useful values.

## Fixture-backed implementation workflow

For each read-only endpoint:

1. Capture its request URL, HTTP method, query/form parameters, and whether it changes state.
2. Save sanitized response HTML as a fixture. Remove names, IDs, grades, personal details, session IDs, cookies, and other private values while preserving the markup and representative table structure.
3. Record populated and empty-state examples when available.
4. Compare with existing endpoints to identify overlap and avoid redundant scraping.
5. Implement a focused parser returning typed domain values.
6. Add fixture-based tests for populated, empty, malformed, and login/error responses.
7. Wire the parser into sync only after its behavior is verified.
8. Update the checklist with module path, fixture path, test coverage, and overlap decision.

Prefer CSS selectors / structured HTML parsing for tables and forms. Use regex only for small, stable text patterns where the surrounding HTML structure is not better represented by selectors.

## Sidebar scope and priorities

From the annotated JavaScript:

1. **Hourwise attendance** — highest priority; compare with resource 7.
2. **Course registration** — read-only and described as the main registration data page.
3. **Hostel attendance** — important; compare with resource 3 only after examining the response.
4. **Course log details** — lower priority; requires selecting a course.
5. **Timetable** — read-only.
6. **Student leave application** — inspect existing application state only; do not submit.
7. **Grievance** — read and write; keep submission out of routine scraping.
8. **Fee dues** — inspect before implementing because payment actions may be present.
9. **Internal marks page** — annotated as not working; use resource 22 unless real response evidence shows unique data.
10. **ID card** — separate document/download flow.
11. **Change password and logout** — user/session actions; exclude from scraper sync.

Other sidebar pages should be classified as read-only, request/action, upload, payment, document, or unknown before implementation. Do not treat every sidebar page as a scraper requirement.

## Parent portal

Before implementation, establish:

- portal base URL and login flow,
- whether it uses separate credentials and sessions,
- exact equivalent attendance endpoint(s),
- response HTML for populated and empty states,
- whether its attendance fields map to the shared result.

Implement it under `portal/parent/` with its own client, auth flow, and parsers. It must not share cookies or assume student-portal behavior. Add it to attendance fallback only after fixture-backed mapping tests pass.

## Completion criteria

The scraper module is complete for a page when:

- the endpoint and parameters are confirmed,
- the response structure is represented by sanitized fixtures,
- the parser returns typed values and explicit errors,
- populated, empty, malformed, and unauthenticated responses are tested,
- sync preserves failures instead of masking them as empty data,
- overlap with other sources has been evaluated,
- fallback order and source diagnostics are documented.

## Next step for the implementing agent

Start by inspecting the current repo state and `docs/portal-scraper-checklist.md`. Then:

1. Finish and verify the `portal/student` module organization while preserving crate-root exports.
2. Add response validation and explicit scrape outcomes.
3. Request or locate sanitized fixtures for resource 7 and `studentHourWiseAttendance.jsp`.
4. Implement and compare those attendance parsers, then establish the normalized fallback coordinator.
5. Add other student pages and the parent portal only as their real endpoint responses become available.