# Student portal scraper checklist

This inventory is derived from the sidebar handlers in the supplied `source.html` and pasted JavaScript. It covers the authenticated `webstream.sastra.edu/sastrapwi` student portal only. The public `sastra.edu` site has a separate scraper under `crates/scraper/src/public/`.

## Rules for avoiding duplicate scraping

- Treat an endpoint and its response as the unit of fetching. Parse one response into all useful domain values before adding another request.
- `getStudentDetails(22)` and `getInternalMarkDetails()` are distinct URLs; compare their response fields before deciding whether one can replace the other.
- `getHourwiseAttendance()` is a distinct academy page from resource ID 7. Do not assume their records overlap until fixtures establish it.
- Request pages and POST/action handlers are not equivalent to read-only data pages. Do not call mutation handlers during routine sync.
- Save a sanitized HTML fixture per distinct read-only response shape, then test parsing offline. Never commit credentials, session cookies, or personal student data.

## Existing resource scrapers

| Portal function / endpoint | Data | Status |
| --- | --- | --- |
| `getStudentDetails(3)` / `StudentDetailsResources.jsp?resourceid=3` | Hostel details | Implemented in `scrape_hostel`; parser tests/fixture not located |
| `getStudentDetails(4)` / resource 4 | Course history | Implemented in `scrape_course_history`; parser tests/fixture not located |
| `getStudentDetails(7)` / resource 7 | Attendance summary | Implemented in `scrape_attendance`; parser tests/fixture not located |
| `getStudentDetails(22)` / resource 22 | Internal marks | Implemented in `scrape_marks`; distinct from academy internal mark page |
| `getStudentDetails(23)` / resource 23 | Exam schedule | Implemented in `scrape_exam_schedule` |
| `getStudentDetails(25)` / resource 25 | Absent hours | Implemented in `scrape_absent_hours` |
| `getStudentDetails(28)` / resource 28 | Grades/GPA | Implemented in `scrape_grades` |
| `getStudentDetails(1)` / `dashboard.jsp` | Dashboard | Fetch/parse coverage not found |

## Annotated priority subset

The supplied annotated JavaScript gives these priorities and constraints:

| Handler | User annotation | Checklist treatment |
| --- | --- | --- |
| `funTimeTable` | Read-only | Candidate for scraper after obtaining its response HTML |
| `funGrievance` | Read and write; request sending needed | Separate read parsing from submission; never submit during sync |
| `funChangePassword` | Input required | User action only; exclude from scraper |
| `getStudentDetails(arg)` | Resource dispatcher; IDs 58 and 70 add parameters | Inventory each relevant resource ID and parameters before fetch; avoid treating dispatcher as one page |
| `getStudentFeeDues` | Annotation says unsure | Inspect endpoint and form behavior before classifying |
| `getCourseRegistration` | Read-only; main registration data | High-priority read-only candidate |
| `getHourwiseAttendance` | Most important attendance view | Highest priority; compare actual response to resource 7 before duplicating data |
| `getCourseLogDetails` | Requires course selection; low priority | Defer until selector/form flow is understood |
| `getInternalMarkDetails` | Not working; use `getStudentDetails(22)` | Mark as covered by resource 22 for current scope; do not implement duplicate page absent evidence of unique data |
| `getStudentDetails1(arg,arg1)` | Supporting function | Not an independent data page; trace callers only |
| `funLogout` | Logout | Session action; exclude |
| `funSaveRequest(arg)` | POST with required remarks and student ID | Mutation; exclude from read-only scraping |
| `funDownloadIDCard` | New feature, ID card | Document/download flow; inspect separately if requested |
| `funHostelAttendance` | Important | High-priority candidate; compare with resource 3 only if fields overlap |
| `funStudentLeaveApplication` | Many fields; read previous data for now | Read existing application state only; do not submit or mutate |

## Remaining sidebar pages

Each row needs endpoint/parameter confirmation, sanitized fixture, parser, domain mapping, and offline tests before it is marked complete. Rows with inputs or side effects should stay out of read-only sync until their behavior is understood.

| Handler | Endpoint / behavior | Initial classification |
| --- | --- | --- |
| `funTimeTable` | `academy/frmStudentTimetable.jsp` | Read-only candidate |
| `funGrievance` | `academy/StudentsGrievances.jsp` | Read plus request submission; inspect forms first |
| `funChangePassword` | `ChangePassword.jsp?hdnPageAction=0` opened in new window | Sensitive action; exclude from scraping |
| `getStudentFeeDues` | `accounts/FeedueIndex.jsp` | Read-only candidate; determine payment actions/data overlap |
| `getCourseRegistration` | `StudentCourseRegistrationIndex.jsp` | Read-only candidate; potential course data overlap |
| `getHourwiseAttendance` | `studentHourWiseAttendance.jsp` | Read-only candidate; compare with resource 7 |
| `getCourseLogDetails` | `StudentCourseLogDetails.jsp` | Read-only candidate; may require course selection |
| `getInternalMarkDetails` | `studentInternalMarkDetails.jsp` | Read-only candidate; compare with resource 22 |
| `funDownloadIDCard` | Opens `academy/report/PrintVirtualIDCard.jsp` | Document/download; inspect separately |
| `funHostelAttendance` | `HostelAttendance.jsp` | Read-only candidate; compare with resource 3 only if same data |
| `funStudentLeaveApplication` | `HostelStudentLeaveApplication.jsp` | Application/action page |
| `funLogout` | Navigates to `Logout.jsp` | Session action; exclude |

## Per-page completion checklist

- [ ] Capture the authenticated request URL, method, required parameters, and whether it changes server state.
- [ ] Capture a sanitized fixture with representative empty and populated states where available.
- [ ] Identify whether another endpoint already provides the same facts; reuse parsed response data when possible.
- [ ] Add a focused parser module with typed domain output and explicit parse errors for malformed required structure.
- [ ] Add fixture-backed tests for populated, empty, and malformed responses.
- [ ] Wire only read-only, useful data into sync; keep request, upload, payment, password, and acknowledgement actions out.
- [ ] Record the module, fixture, tests, and overlap decision in this checklist.

## Current priority

Start with `getHourwiseAttendance()` after obtaining its actual response HTML. It is a high-value read-only page and can be compared directly with resource 7 to establish whether a single fetch can supply both attendance views. Do not write selectors or regexes based only on the parent page's JavaScript.
