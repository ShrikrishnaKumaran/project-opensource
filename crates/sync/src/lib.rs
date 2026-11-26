//! Sync coordinator: wires concrete `scraper::SastraPortalSource` and
//! `scraper::SastraPublicSource` into `storage`.
//! Depends on `domain`, `scraper`, `storage`, and `captcha`.

pub use captcha::OcrDebugResult;
pub use domain::*;
use scraper::{SastraPortalSource, SastraPublicSource};
use storage::Storage;

#[derive(Clone)]
pub struct SyncService {
    storage: Storage,
    source: SastraPortalSource,
    public_source: SastraPublicSource,
    ocr_endpoint: String,
}

impl Default for SyncService {
    fn default() -> Self {
        Self::new(Storage::new())
    }
}

impl SyncService {
    pub fn new(storage: Storage) -> Self {
        Self {
            storage,
            source: SastraPortalSource::new(),
            public_source: SastraPublicSource::new(),
            ocr_endpoint: "http://localhost:8000/solve".to_string(),
        }
    }

    pub fn with_source(storage: Storage, source: SastraPortalSource) -> Self {
        Self {
            storage,
            source,
            public_source: SastraPublicSource::new(),
            ocr_endpoint: "http://localhost:8000/solve".to_string(),
        }
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    // ── Authenticated portal ──────────────────────────────────────────────────

    pub async fn fetch_captcha(&self) -> Result<CaptchaChallenge, String> {
        self.source.fetch_captcha().await.map_err(|e| e.to_string())
    }

    pub async fn try_solve_captcha(&self, data_uri: &str) -> Option<String> {
        captcha::try_solve_ocr(data_uri, &self.ocr_endpoint).await
    }

    pub async fn try_solve_captcha_debug(&self, data_uri: &str) -> OcrDebugResult {
        captcha::try_solve_ocr_debug(data_uri, &self.ocr_endpoint).await
    }

    pub async fn login_and_sync(
        &self,
        creds: &Credentials,
        captcha_code: &str,
        session_id: &str,
    ) -> Result<(), String> {
        let session = self
            .source
            .login(creds, captcha_code, session_id)
            .await
            .map_err(|e| e.to_string())?;

        self.storage.set_session(session.clone());

        let data = self
            .source
            .fetch_portal_data(&session)
            .await
            .map_err(|e| e.to_string())?;

        self.storage.set_portal_data(data);

        Ok(())
    }

    pub async fn refresh(&self) -> Result<(), String> {
        let session = self
            .storage
            .get_session()
            .ok_or_else(|| "No active session".to_string())?;

        let data = self
            .source
            .fetch_portal_data(&session)
            .await
            .map_err(|e| e.to_string())?;

        self.storage.set_portal_data(data);

        Ok(())
    }

    // ── Public website data ───────────────────────────────────────────────────

    /// Scrape all public SASTRA website data and store it.
    ///
    /// This is **network-heavy** (dozens of pages). Call it once on startup
    /// and then periodically in a background task.
    pub async fn refresh_public(&self) -> Result<(), String> {
        let data = self
            .public_source
            .fetch_public_data()
            .await
            .map_err(|e| e.to_string())?;

        self.storage.set_public_data(data);
        Ok(())
    }

    /// Return the last cached public data snapshot (empty on first run before
    /// [`refresh_public`] completes).
    pub fn get_public_data(&self) -> PublicData {
        self.storage.get_public_data()
    }
}
