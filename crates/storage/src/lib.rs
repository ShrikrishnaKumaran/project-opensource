//! Storage layer: holds portal state and session.
//! Depends only on `domain`.

pub use domain::*;
use std::sync::{Arc, RwLock};

#[derive(Clone, Default)]
pub struct Storage {
    session: Arc<RwLock<Option<Session>>>,
    data: Arc<RwLock<PortalData>>,
    /// Cached public website data (no auth required).
    public_data: Arc<RwLock<PublicData>>,
}

impl Storage {
    pub fn new() -> Self {
        Self {
            session: Arc::new(RwLock::new(None)),
            data: Arc::new(RwLock::new(PortalData {
                profile: None,
                attendance: Vec::new(),
                marks: Vec::new(),
                grades: Vec::new(),
                gpa: None,
                exams: Vec::new(),
                hostel: None,
                absent_hours: Vec::new(),
                course_history: Vec::new(),
            })),
            public_data: Arc::new(RwLock::new(PublicData::default())),
        }
    }

    // ── Authenticated portal ──────────────────────────────────────────────────

    pub fn set_session(&self, session: Session) {
        if let Ok(mut lock) = self.session.write() {
            *lock = Some(session);
        }
    }

    pub fn get_session(&self) -> Option<Session> {
        self.session.read().ok().and_then(|lock| lock.clone())
    }

    pub fn clear_session(&self) {
        if let Ok(mut lock) = self.session.write() {
            *lock = None;
        }
    }

    pub fn set_portal_data(&self, data: PortalData) {
        if let Ok(mut lock) = self.data.write() {
            *lock = data;
        }
    }

    pub fn get_portal_data(&self) -> PortalData {
        self.data.read().ok().map_or_else(
            || PortalData {
                profile: None,
                attendance: Vec::new(),
                marks: Vec::new(),
                grades: Vec::new(),
                gpa: None,
                exams: Vec::new(),
                hostel: None,
                absent_hours: Vec::new(),
                course_history: Vec::new(),
            },
            |lock| lock.clone(),
        )
    }

    // ── Public website data ───────────────────────────────────────────────────

    /// Store a freshly scraped [`PublicData`] snapshot.
    pub fn set_public_data(&self, data: PublicData) {
        if let Ok(mut lock) = self.public_data.write() {
            *lock = data;
        }
    }

    /// Read the last cached [`PublicData`] snapshot (may be default/empty on first run).
    pub fn get_public_data(&self) -> PublicData {
        self.public_data
            .read()
            .ok()
            .map_or_else(PublicData::default, |lock| lock.clone())
    }
}
