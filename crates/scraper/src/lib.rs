//! Scraper layer: network and parsing logic for SASTRA.
//! Depends only on `domain`.
//!
//! ## Modules
//! - Root (`lib.rs`) — authenticated portal scraper (`webstream.sastra.edu`)
//! - [`public`]      — unauthenticated public website scraper (`sastra.edu`)

pub mod public;
pub use public::SastraPublicSource;

use std::collections::HashMap;
use std::time::Duration;

use base64::{engine::general_purpose, Engine as _};
use domain::{
    AbsentHourRecord, AttendanceRecord, CaptchaChallenge, CourseHistoryItem, Credentials,
    DomainError, ExamScheduleItem, GpaSummary, GradeRecord, HostelDetails, InternalMark,
    PortalData, PortalSource, Session, StudentProfile,
};
use regex::Regex;
use reqwest::{
    header::{HeaderMap, HeaderValue, COOKIE, REFERER},
    redirect::Policy,
    Client,
};

pub const BASE: &str = "https://webstream.sastra.edu/sastrapwi";
pub const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
                     AppleWebKit/537.36 (KHTML, like Gecko) \
                     Chrome/135.0.0.0 Safari/537.36";

pub fn base_headers(session_id: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        "accept",
        HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
    );
    h.insert(
        "accept-language",
        HeaderValue::from_static("en-US,en;q=0.9"),
    );
    h.insert(
        "accept-encoding",
        HeaderValue::from_static("gzip, deflate, br"),
    );
    h.insert("connection", HeaderValue::from_static("keep-alive"));
    h.insert("host", HeaderValue::from_static("webstream.sastra.edu"));
    h.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
    h.insert("sec-fetch-mode", HeaderValue::from_static("navigate"));
    h.insert("sec-fetch-dest", HeaderValue::from_static("document"));
    h.insert("upgrade-insecure-requests", HeaderValue::from_static("1"));
    if let Ok(v) = HeaderValue::from_str(UA) {
        h.insert(reqwest::header::USER_AGENT, v);
    }
    if let Ok(v) = HeaderValue::from_str(&format!("JSESSIONID={session_id}")) {
        h.insert(COOKIE, v);
    }
    h.insert(
        REFERER,
        HeaderValue::from_static("https://webstream.sastra.edu/sastrapwi/"),
    );
    h
}

pub fn no_redirect_client() -> Client {
    Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap_or_else(|_| Client::new())
}

pub fn normal_client() -> Client {
    Client::builder()
        .redirect(Policy::limited(5))
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap_or_else(|_| Client::new())
}

pub fn extract_session(set_cookie: &str) -> Option<String> {
    let re = Regex::new(r"JSESSIONID=([^;]+)").ok()?;
    re.captures(set_cookie).map(|c| c[1].to_string())
}

pub async fn fetch_captcha(client: &Client) -> Result<CaptchaChallenge, DomainError> {
    let mut current_session: Option<String> = None;

    for _attempt in 1..=3 {
        let mut req = client
            .get(format!("{BASE}/stickyImg"))
            .header("user-agent", UA)
            .header(
                "accept",
                "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8",
            )
            .header("referer", format!("{BASE}/"));

        if let Some(ref sid) = current_session {
            req = req.header("cookie", format!("JSESSIONID={sid}"));
        }

        let res = req
            .send()
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?;

        if let Some(sc) = res
            .headers()
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
        {
            if let Some(sid) = extract_session(sc) {
                current_session = Some(sid);
            }
        }

        let ct = res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let bytes = res
            .bytes()
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?;

        if !bytes.is_empty()
            && (ct.contains("image")
                || bytes.starts_with(b"\x89PNG")
                || bytes.starts_with(b"\xFF\xD8"))
        {
            let session_id = current_session.ok_or_else(|| {
                DomainError::Captcha("Received captcha image but missing JSESSIONID".to_string())
            })?;
            let b64 = general_purpose::STANDARD.encode(&bytes);
            let final_ct = if ct.is_empty() { "image/png" } else { &ct };
            let data_uri = format!("data:{final_ct};base64,{b64}");
            return Ok(CaptchaChallenge {
                session_id,
                data_uri,
            });
        }
    }

    Err(DomainError::Captcha(
        "Failed to retrieve captcha image after 3 attempts. Portal may be down.".to_string(),
    ))
}

pub async fn login(
    client: &Client,
    creds: &Credentials,
    captcha: &str,
    session_id: &str,
) -> Result<Session, DomainError> {
    let mut form = HashMap::new();
    form.insert("txtRegNumber", creds.reg_number.as_str());
    form.insert("txtPwd", creds.password.as_str());
    form.insert("txtPA", "1");
    form.insert("answer", captcha);

    let res = client
        .post(format!("{BASE}/"))
        .header("user-agent", UA)
        .header("accept", "text/html,application/xhtml+xml,*/*")
        .header("accept-encoding", "gzip, deflate, br")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("cookie", format!("JSESSIONID={session_id}"))
        .header("origin", "https://webstream.sastra.edu")
        .header("referer", "https://webstream.sastra.edu/sastrapwi/")
        .header("sec-fetch-site", "same-origin")
        .form(&form)
        .send()
        .await
        .map_err(|e| DomainError::Network(e.to_string()))?;

    let status = res.status();

    if status == 302 {
        let new_session = res
            .headers()
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .and_then(extract_session)
            .unwrap_or_else(|| session_id.to_string());

        return Ok(Session { id: new_session });
    }

    let body = res.text().await.unwrap_or_default();
    if body.contains("Invalid Verification Code") {
        return Err(DomainError::Auth("Wrong captcha. Try again.".to_string()));
    }
    if body.contains("Invalid Registration Number") {
        return Err(DomainError::Auth(
            "Wrong registration number or password.".to_string(),
        ));
    }

    Err(DomainError::Auth(format!(
        "Login failed with status {status}."
    )))
}

pub async fn scrape_profile(
    client: &Client,
    session_id: &str,
) -> Result<StudentProfile, DomainError> {
    let url = format!("{BASE}/usermanager/home.jsp");
    let res = client
        .get(&url)
        .headers(base_headers(session_id))
        .send()
        .await
        .map_err(|e| DomainError::Network(e.to_string()))?;

    let html = res.text().await.unwrap_or_default();
    let re = Regex::new(r"<li>\s*(.*?)\s*</li>").map_err(|e| DomainError::Parse(e.to_string()))?;
    let tag_re = Regex::new(r"<[^>]+>").map_err(|e| DomainError::Parse(e.to_string()))?;

    let mut fields = Vec::new();
    for cap in re.captures_iter(&html).take(5) {
        let raw = cap[1].to_string();
        let clean = tag_re.replace_all(&raw, "").trim().to_string();
        fields.push(clean);
    }

    Ok(StudentProfile {
        student_id: fields.first().cloned().unwrap_or_default(),
        name: fields.get(1).cloned().unwrap_or_default(),
        program: fields.get(2).cloned().unwrap_or_default(),
        institution: fields.get(3).cloned().unwrap_or_default(),
        semester: fields.get(4).cloned().unwrap_or_default(),
    })
}

pub async fn scrape_attendance(
    client: &Client,
    session_id: &str,
) -> Result<Vec<AttendanceRecord>, DomainError> {
    let url = format!("{BASE}/resource/StudentDetailsResources.jsp?resourceid=7");
    let res = client
        .get(&url)
        .headers(base_headers(session_id))
        .send()
        .await
        .map_err(|e| DomainError::Network(e.to_string()))?;

    let html = res.text().await.unwrap_or_default();
    let re = Regex::new(
        r#"<TD class="tablecontent01">\s*([^<]+)\s*</TD>\s*<TD class="tablecontent01">\s*([^<]+)\s*</TD>\s*<TD class="tablecontent02">\s*(\d+)\s*</TD>\s*<TD class="tablecontent02">\s*(\d+)\s*</TD>\s*<TD class="tablecontent02">\s*(\d+)\s*</TD>\s*<TD class="tablecontent03"[^>]*>\s*<font[^>]*>\s*(\d+)\s*</font>"#,
    ).map_err(|e| DomainError::Parse(e.to_string()))?;

    let mut list = Vec::new();
    for cap in re.captures_iter(&html) {
        list.push(AttendanceRecord {
            code: cap[1].trim().to_string(),
            description: cap[2].trim().to_string(),
            total: cap[3].trim().parse().unwrap_or(0),
            present: cap[4].trim().parse().unwrap_or(0),
            absent: cap[5].trim().parse().unwrap_or(0),
            percentage: cap[6].trim().parse().unwrap_or(0),
        });
    }

    Ok(list)
}

pub async fn scrape_marks(
    client: &Client,
    session_id: &str,
) -> Result<Vec<InternalMark>, DomainError> {
    let url = format!("{BASE}/resource/StudentDetailsResources.jsp?resourceid=22");
    let res = client
        .get(&url)
        .headers(base_headers(session_id))
        .send()
        .await
        .map_err(|e| DomainError::Network(e.to_string()))?;

    let html = res.text().await.unwrap_or_default();
    let re = Regex::new(
        r#"(?s)<TD\s+class="tablecontent02">\s*([^<]+)\s*</TD>.*?<TD\s+class="tablecontent02">\s*([^<]+)\s*</TD>.*?<TD\s+class="tablecontent01">\s*([^<]+)\s*</TD>.*?<TD\s+class="tablecontent02">\s*([^<]+)\s*</TD>.*?<TD\s+class="tablecontent02">\s*([^<]+)\s*</TD>"#,
    ).map_err(|e| DomainError::Parse(e.to_string()))?;

    let mut list = Vec::new();
    for cap in re.captures_iter(&html) {
        let code = cap[1].trim().to_string();
        if code.is_empty() || code.to_lowercase().contains("code") {
            continue;
        }
        list.push(InternalMark {
            code,
            description: cap[2].trim().to_string(),
            component: cap[3].trim().to_string(),
            marks: cap[4].trim().to_string(),
            max_marks: cap[5].trim().to_string(),
        });
    }

    Ok(list)
}

pub async fn scrape_grades(
    client: &Client,
    session_id: &str,
) -> Result<(Option<GpaSummary>, Vec<GradeRecord>), DomainError> {
    let url = format!("{BASE}/resource/StudentDetailsResources.jsp?resourceid=28");
    let res = client
        .get(&url)
        .headers(base_headers(session_id))
        .send()
        .await
        .map_err(|e| DomainError::Network(e.to_string()))?;

    let html = res.text().await.unwrap_or_default();

    let mut cgpa = None;
    if let Some(idx) = html.find("CGPA") {
        let window = &html[idx..(idx + 300).min(html.len())];
        if let Ok(re) = Regex::new(r"\d+\.\d+") {
            if let Some(m) = re.find(window) {
                cgpa = Some(m.as_str().to_string());
            }
        }
    }

    let mut sgpa_list = Vec::new();
    if let Ok(sgpa_re) = Regex::new(
        r#"<tr class="ui-widget-content">\s*<td><b>(\d+)</b></td>\s*<td><b>(\d+\.\d+)</b></td>"#,
    ) {
        for cap in sgpa_re.captures_iter(&html) {
            if let Ok(sem) = cap[1].parse::<u32>() {
                sgpa_list.push((sem, cap[2].to_string()));
            }
        }
    }

    let gpa = Some(GpaSummary { cgpa, sgpa_list });

    let marks_re = Regex::new(
        r"(?s)<tr[^>]*>\s*<td[^>]*>(\d+)</td>\s*<td[^>]*nowrap[^>]*>\s*([A-Za-z]+ / \d{4})\s*</td>\s*<td[^>]*>\s*([A-Z0-9]+)</td>\s*<td[^>]*>\s*([^<]+)</td>\s*<td[^>]*>\s*(\d+)</td>\s*<td[^>]*>\s*(\d+)</td>\s*<td[^>]*>\s*([A-Z+]+)</td>",
    ).map_err(|e| DomainError::Parse(e.to_string()))?;

    let mut grades = Vec::new();
    for cap in marks_re.captures_iter(&html) {
        grades.push(GradeRecord {
            semester: cap[1].trim().parse().unwrap_or(0),
            month_year: cap[2].trim().to_string(),
            code: cap[3].trim().to_string(),
            description: cap[4].trim().to_string(),
            cia: cap[5].trim().to_string(),
            credits: cap[6].trim().parse().unwrap_or(0),
            grade: cap[7].trim().to_string(),
        });
    }

    Ok((gpa, grades))
}

pub async fn scrape_exam_schedule(
    client: &Client,
    session_id: &str,
) -> Result<Vec<ExamScheduleItem>, DomainError> {
    let url = format!("{BASE}/resource/StudentDetailsResources.jsp?resourceid=23");
    let res = client
        .get(&url)
        .headers(base_headers(session_id))
        .send()
        .await
        .map_err(|e| DomainError::Network(e.to_string()))?;

    let html = res.text().await.unwrap_or_default();
    let re = Regex::new(
        r#"(?i)<tr>\s*<td class="tablecontent02"[^>]*>([^<]+)</td>\s*<td class="tablecontent02"[^>]*>([^<]+)</td>\s*<td class="tablecontent01"[^>]*>([^<]+)</td>\s*<td class="tablecontent01"[^>]*>([^<]+)</td>\s*<td class="tablecontent01"[^>]*>([^<]*)</td>"#,
    ).map_err(|e| DomainError::Parse(e.to_string()))?;

    let mut list = Vec::new();
    for cap in re.captures_iter(&html) {
        list.push(ExamScheduleItem {
            date: cap[1].trim().to_string(),
            time: cap[2].trim().to_string(),
            code: cap[3].trim().to_string(),
            description: cap[4].trim().to_string(),
            status: cap[5].trim().to_string(),
        });
    }

    Ok(list)
}

pub async fn scrape_hostel(
    client: &Client,
    session_id: &str,
) -> Result<Option<HostelDetails>, DomainError> {
    let url = format!("{BASE}/resource/StudentDetailsResources.jsp?resourceid=3");
    let res = client
        .get(&url)
        .headers(base_headers(session_id))
        .send()
        .await
        .map_err(|e| DomainError::Network(e.to_string()))?;

    let html = res.text().await.unwrap_or_default();
    let re = Regex::new(r#"(?i)<td[^>]*class="tablecontent02"[^>]*>([\s\S]*?)</td>"#)
        .map_err(|e| DomainError::Parse(e.to_string()))?;
    let tag_re = Regex::new(r"<[^>]+>").map_err(|e| DomainError::Parse(e.to_string()))?;

    let mut fields = Vec::new();
    for cap in re.captures_iter(&html).take(4) {
        let val = tag_re.replace_all(cap[1].trim(), "").trim().to_string();
        fields.push(val);
    }

    if fields.is_empty() {
        return Ok(None);
    }

    Ok(Some(HostelDetails {
        hostel_name: fields.first().cloned().unwrap_or_default(),
        room_no: fields.get(1).cloned().unwrap_or_default(),
        allocation_date: fields.get(2).cloned().unwrap_or_default(),
        mess_name: fields.get(3).cloned().unwrap_or_default(),
    }))
}

pub async fn scrape_absent_hours(
    client: &Client,
    session_id: &str,
) -> Result<Vec<AbsentHourRecord>, DomainError> {
    let url = format!("{BASE}/resource/StudentDetailsResources.jsp?resourceid=25");
    let res = client
        .get(&url)
        .headers(base_headers(session_id))
        .send()
        .await
        .map_err(|e| DomainError::Network(e.to_string()))?;

    let html = res.text().await.unwrap_or_default();
    let re = Regex::new(
        r"(?s)<tr>\s*(?:<td[^>]*>[^<]*</td>\s*){1}<td[^>]*>\s*([^<]+)\s*</td>\s*<td[^>]*>\s*([^<]+)\s*</td>\s*<td[^>]*>\s*([^<]+)\s*</td>\s*<td[^>]*>\s*([^<]+)\s*</td>",
    ).map_err(|e| DomainError::Parse(e.to_string()))?;

    let mut list = Vec::new();
    for cap in re.captures_iter(&html) {
        let date = cap[1].trim().to_string();
        let code = cap[2].trim().to_string();
        let description = cap[3].trim().to_string();
        let hour = cap[4].trim().to_string();
        if date.is_empty() || hour.is_empty() {
            continue;
        }
        list.push(AbsentHourRecord {
            date,
            code,
            description,
            hour,
        });
    }

    Ok(list)
}

pub async fn scrape_course_history(
    client: &Client,
    session_id: &str,
) -> Result<Vec<CourseHistoryItem>, DomainError> {
    let url = format!("{BASE}/resource/StudentDetailsResources.jsp?resourceid=4");
    let res = client
        .get(&url)
        .headers(base_headers(session_id))
        .send()
        .await
        .map_err(|e| DomainError::Network(e.to_string()))?;

    let html = res.text().await.unwrap_or_default();
    let row_re =
        Regex::new(r"(?s)<tr>(.*?)</tr>").map_err(|e| DomainError::Parse(e.to_string()))?;
    let td_re =
        Regex::new(r"(?s)<td[^>]*>(.*?)</td>").map_err(|e| DomainError::Parse(e.to_string()))?;
    let tag_re = Regex::new(r"<[^>]+>").map_err(|e| DomainError::Parse(e.to_string()))?;

    let mut list = Vec::new();
    for row in row_re.captures_iter(&html) {
        let cells: Vec<String> = td_re
            .captures_iter(&row[1])
            .map(|c| tag_re.replace_all(c[1].trim(), "").trim().to_string())
            .collect();
        if cells.len() == 5 && !cells[0].is_empty() && cells[0] != "Course Code" {
            list.push(CourseHistoryItem {
                code: cells[0].clone(),
                description: cells[1].clone(),
                semester: cells[2].clone(),
                credits: cells[3].clone(),
            });
        }
    }

    Ok(list)
}

/// Concrete `PortalSource` implementing the domain trait for SASTRA.
#[derive(Clone)]
pub struct SastraPortalSource {
    no_redirect: Client,
    normal: Client,
}

impl Default for SastraPortalSource {
    fn default() -> Self {
        Self::new()
    }
}

impl SastraPortalSource {
    pub fn new() -> Self {
        Self {
            no_redirect: no_redirect_client(),
            normal: normal_client(),
        }
    }

    pub async fn fetch_captcha(&self) -> Result<CaptchaChallenge, DomainError> {
        fetch_captcha(&self.no_redirect).await
    }

    pub async fn login(
        &self,
        creds: &Credentials,
        captcha_code: &str,
        session_id: &str,
    ) -> Result<Session, DomainError> {
        login(&self.no_redirect, creds, captcha_code, session_id).await
    }

    pub async fn fetch_portal_data(&self, session: &Session) -> Result<PortalData, DomainError> {
        let profile = scrape_profile(&self.normal, &session.id).await.ok();
        let attendance = scrape_attendance(&self.normal, &session.id)
            .await
            .unwrap_or_default();
        let marks = scrape_marks(&self.normal, &session.id)
            .await
            .unwrap_or_default();
        let (gpa, grades) = scrape_grades(&self.normal, &session.id)
            .await
            .unwrap_or((None, Vec::new()));
        let exams = scrape_exam_schedule(&self.normal, &session.id)
            .await
            .unwrap_or_default();
        let hostel = scrape_hostel(&self.normal, &session.id)
            .await
            .ok()
            .flatten();
        let absent_hours = scrape_absent_hours(&self.normal, &session.id)
            .await
            .unwrap_or_default();
        let course_history = scrape_course_history(&self.normal, &session.id)
            .await
            .unwrap_or_default();

        Ok(PortalData {
            profile,
            attendance,
            marks,
            grades,
            gpa,
            exams,
            hostel,
            absent_hours,
            course_history,
        })
    }
}

impl PortalSource for SastraPortalSource {
    async fn fetch_captcha(&self) -> Result<CaptchaChallenge, DomainError> {
        Self::fetch_captcha(self).await
    }

    async fn login(
        &self,
        creds: &Credentials,
        captcha_code: &str,
        session_id: &str,
    ) -> Result<Session, DomainError> {
        Self::login(self, creds, captcha_code, session_id).await
    }

    async fn fetch_portal_data(&self, session: &Session) -> Result<PortalData, DomainError> {
        Self::fetch_portal_data(self, session).await
    }
}
