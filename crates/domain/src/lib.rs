//! Domain types and traits (no I/O, no heavy dependencies).
//!
//! Two top-level data models:
//! - [`PortalData`]  — authenticated student portal (webstream.sastra.edu)
//! - [`PublicData`]  — public SASTRA website (sastra.edu), no login required

#[derive(Debug)]
pub enum DomainError {
    Network(String),
    Auth(String),
    Parse(String),
    Captcha(String),
    Other(String),
}

impl std::fmt::Display for DomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(msg) => write!(f, "Network error: {msg}"),
            Self::Auth(msg) => write!(f, "Auth error: {msg}"),
            Self::Parse(msg) => write!(f, "Parse error: {msg}"),
            Self::Captcha(msg) => write!(f, "Captcha error: {msg}"),
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for DomainError {}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Credentials {
    pub reg_number: String,
    pub password: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Session {
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CaptchaChallenge {
    pub session_id: String,
    pub data_uri: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct StudentProfile {
    pub student_id: String,
    pub name: String,
    pub program: String,
    pub institution: String,
    pub semester: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct AttendanceRecord {
    pub code: String,
    pub description: String,
    pub total: u32,
    pub present: u32,
    pub absent: u32,
    pub percentage: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct InternalMark {
    pub code: String,
    pub description: String,
    pub component: String,
    pub marks: String,
    pub max_marks: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GradeRecord {
    pub semester: u32,
    pub month_year: String,
    pub code: String,
    pub description: String,
    pub cia: String,
    pub credits: u32,
    pub grade: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GpaSummary {
    pub cgpa: Option<String>,
    pub sgpa_list: Vec<(u32, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ExamScheduleItem {
    pub date: String,
    pub time: String,
    pub code: String,
    pub description: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct HostelDetails {
    pub hostel_name: String,
    pub room_no: String,
    pub allocation_date: String,
    pub mess_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct AbsentHourRecord {
    pub date: String,
    pub code: String,
    pub description: String,
    pub hour: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CourseHistoryItem {
    pub code: String,
    pub description: String,
    pub semester: String,
    pub credits: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PortalData {
    pub profile: Option<StudentProfile>,
    pub attendance: Vec<AttendanceRecord>,
    pub marks: Vec<InternalMark>,
    pub grades: Vec<GradeRecord>,
    pub gpa: Option<GpaSummary>,
    pub exams: Vec<ExamScheduleItem>,
    pub hostel: Option<HostelDetails>,
    pub absent_hours: Vec<AbsentHourRecord>,
    pub course_history: Vec<CourseHistoryItem>,
}

/// Abstract contract for reading data from a portal source.
pub trait PortalSource: Send + Sync {
    fn fetch_captcha(
        &self,
    ) -> impl std::future::Future<Output = Result<CaptchaChallenge, DomainError>> + Send;
    fn login(
        &self,
        creds: &Credentials,
        captcha_code: &str,
        session_id: &str,
    ) -> impl std::future::Future<Output = Result<Session, DomainError>> + Send;
    fn fetch_portal_data(
        &self,
        session: &Session,
    ) -> impl std::future::Future<Output = Result<PortalData, DomainError>> + Send;
}

// ─────────────────────────────────────────────────────────────────────────────
// Public website domain types (sastra.edu — no login required)
// ─────────────────────────────────────────────────────────────────────────────

/// A single item from a running banner (academic updates / news & events).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct BannerItem {
    /// Display text of the banner item.
    pub title: String,
    /// Optional hyperlink attached to the item.
    pub url: Option<String>,
}

/// A single course entry with a link to its PDF syllabus.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CourseLink {
    /// Course code, e.g. "CSE301".
    pub code: String,
    /// Human-readable course name.
    pub name: String,
    /// Direct URL to the course PDF.
    pub pdf_url: String,
}

/// All courses for one semester of a program.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SemesterCourses {
    /// Semester number (1–8).
    pub semester: u8,
    pub courses: Vec<CourseLink>,
}

/// Full curriculum tree for one school + program + batch.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CurriculumProgram {
    /// School name, e.g. "School of Computing".
    pub school: String,
    /// Program code, e.g. "BCSE".
    pub program: String,
    /// Regulation / batch year, e.g. "2021-22".
    pub batch: String,
    /// URL of the program index page on apps.sastra.edu.
    pub index_url: String,
    /// All eight semesters (populated after full scrape).
    pub semesters: Vec<SemesterCourses>,
}

/// Detailed information about a faculty member (fetched lazily on demand).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FacultyDetail {
    pub qualifications: String,
    pub experience: String,
    pub research_areas: Vec<String>,
    pub publications: Vec<String>,
}

/// A faculty member as listed on a school's staff profile page.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FacultyMember {
    pub name: String,
    pub designation: String,
    pub department: String,
    pub email: Option<String>,
    /// Absolute URL to the faculty member's photo, if available.
    pub photo_url: Option<String>,
    /// Absolute URL to the individual profile page (used for lazy detail fetch).
    pub profile_url: String,
    /// Populated only after [`scrape_faculty_detail`] is called for this member.
    pub detail: Option<FacultyDetail>,
}

/// One entry in the academic calendar.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CalendarEntry {
    pub event: String,
    pub date_range: String,
    /// Link to a downloadable PDF, if the page provides one.
    pub pdf_url: Option<String>,
}

/// All public data fetched from the main SASTRA website.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PublicData {
    /// Items from the "Academic Updates" running banner.
    pub academic_updates: Vec<BannerItem>,
    /// Items from the "News & Events" running banner.
    pub news_events: Vec<BannerItem>,
    /// Curriculum tree (school → program → semester → course PDF).
    pub curriculum: Vec<CurriculumProgram>,
    /// Faculty directory (all schools combined; detail is lazy).
    pub faculty: Vec<FacultyMember>,
    /// Academic calendar entries.
    pub calendar: Vec<CalendarEntry>,
}

/// Abstract contract for reading data from the public SASTRA website.
pub trait PublicSource: Send + Sync {
    fn fetch_public_data(
        &self,
    ) -> impl std::future::Future<Output = Result<PublicData, DomainError>> + Send;
}
