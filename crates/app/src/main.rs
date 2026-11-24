#![allow(non_snake_case)]

use dioxus::prelude::*;
use storage::Storage;
use sync::{
    BannerItem, CalendarEntry, CaptchaChallenge, Credentials, CurriculumProgram, FacultyMember,
    PortalData, PublicData, SyncService,
};

fn main() {
    dioxus::launch(App);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    // Portal (authenticated)
    Profile,
    Attendance,
    Marks,
    Grades,
    Exams,
    Hostel,
    Absent,
    Courses,
    // Public website (no login required)
    Updates,
    Curriculum,
    Faculty,
    Calendar,
}

#[component]
fn App() -> Element {
    let storage = use_hook(Storage::new);
    let sync_service = use_hook(|| SyncService::new(storage.clone()));

    // Authentication state
    let mut reg_no = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut captcha_input = use_signal(String::new);
    let mut captcha_challenge = use_signal(|| None::<CaptchaChallenge>);
    let mut is_logged_in = use_signal(|| false);

    // Navigation: public pages are available before and after login.
    let mut active_tab = use_signal(|| Tab::Updates);

    // Application state
    let mut status_msg = use_signal(|| "Initializing application...".to_string());
    let mut is_loading = use_signal(|| false);
    let mut portal_data = use_signal(PortalData::default);
    let mut public_data = use_signal(PublicData::default);
    let mut public_loading = use_signal(|| false);
    let mut public_status = use_signal(|| "Loading public data...".to_string());

    // Drill-down state
    let mut selected_faculty: Signal<Option<FacultyMember>> = use_signal(|| None);

    let mut selected_program: Signal<Option<CurriculumProgram>> = use_signal(|| None);

    // ---------------------------------------------------------
    // INITIAL PUBLIC DATA LOAD
    // ---------------------------------------------------------

    let svc_for_public = sync_service.clone();

    use_effect(move || {
        let svc = svc_for_public.clone();

        spawn(async move {
            public_loading.set(true);
            public_status.set("Loading public information...".to_string());

            match svc.refresh_public().await {
                Ok(()) => {
                    public_data.set(svc.get_public_data());
                    public_status.set("Public information loaded.".to_string());
                }
                Err(err) => {
                    public_status.set(format!("Failed to load public information: {err}"));
                }
            }

            public_loading.set(false);
        });
    });

    // ---------------------------------------------------------
    // INITIAL CAPTCHA LOAD
    // ---------------------------------------------------------

    let service_for_load = sync_service.clone();

    use_effect(move || {
        let svc = service_for_load.clone();

        spawn(async move {
            is_loading.set(true);
            status_msg.set("Loading captcha...".to_string());

            match svc.fetch_captcha().await {
                Ok(ch) => {
                    if let Some(ocr_text) = svc.try_solve_captcha(&ch.data_uri).await {
                        captcha_input.set(ocr_text);
                        status_msg.set(
                            "Captcha automatically recognized. Verify before signing in."
                                .to_string(),
                        );
                    } else {
                        status_msg.set("Enter the captcha shown below.".to_string());
                    }

                    captcha_challenge.set(Some(ch));
                }

                Err(err) => {
                    status_msg.set(format!("Failed to load captcha: {err}"));
                }
            }

            is_loading.set(false);
        });
    });

    // ---------------------------------------------------------
    // RELOAD CAPTCHA
    // ---------------------------------------------------------

    let reload_captcha = {
        let svc = sync_service.clone();

        move |_| {
            if is_loading() {
                return;
            }

            let svc = svc.clone();

            spawn(async move {
                is_loading.set(true);
                status_msg.set("Refreshing captcha...".to_string());

                captcha_input.set(String::new());
                captcha_challenge.set(None);

                match svc.fetch_captcha().await {
                    Ok(ch) => {
                        if let Some(ocr_text) = svc.try_solve_captcha(&ch.data_uri).await {
                            captcha_input.set(ocr_text);
                            status_msg.set("Captcha automatically recognized.".to_string());
                        } else {
                            status_msg.set("Enter the new captcha.".to_string());
                        }

                        captcha_challenge.set(Some(ch));
                    }

                    Err(err) => {
                        status_msg.set(format!("Captcha refresh failed: {err}"));
                    }
                }

                is_loading.set(false);
            });
        }
    };

    // ---------------------------------------------------------
    // LOGIN
    // ---------------------------------------------------------

    let on_login = {
        let svc = sync_service.clone();

        move |_| {
            if is_loading() {
                return;
            }

            let svc = svc.clone();

            let reg = reg_no().trim().to_string();
            let pwd = password();
            let cap = captcha_input().trim().to_string();
            let challenge = captcha_challenge();

            if reg.is_empty() || pwd.is_empty() || cap.is_empty() {
                status_msg.set("Please fill in all login fields.".to_string());
                return;
            }

            let Some(challenge) = challenge else {
                status_msg.set("Captcha is unavailable. Please reload it.".to_string());
                return;
            };

            spawn(async move {
                is_loading.set(true);
                status_msg.set("Authenticating and synchronizing student data...".to_string());

                let creds = Credentials {
                    reg_number: reg,
                    password: pwd,
                };

                match svc
                    .login_and_sync(&creds, &cap, &challenge.session_id)
                    .await
                {
                    Ok(()) => {
                        // Read the synchronized private data.
                        portal_data.set(svc.storage().get_portal_data());

                        // Change authentication state.
                        is_logged_in.set(true);

                        // Open the private dashboard.
                        active_tab.set(Tab::Profile);

                        // Clear login form state.
                        password.set(String::new());
                        captcha_input.set(String::new());
                        captcha_challenge.set(None);

                        status_msg.set("Login successful! Welcome to your dashboard.".to_string());
                    }

                    Err(err) => {
                        status_msg.set(format!("Login failed: {err}"));

                        // Get a fresh captcha after unsuccessful login.
                        captcha_input.set(String::new());
                        captcha_challenge.set(None);

                        match svc.fetch_captcha().await {
                            Ok(new_ch) => {
                                captcha_challenge.set(Some(new_ch));
                            }
                            Err(captcha_err) => {
                                status_msg.set(format!(
                                    "Login failed: {err}. Captcha refresh failed: {captcha_err}"
                                ));
                            }
                        }
                    }
                }

                is_loading.set(false);
            });
        }
    };

    // ---------------------------------------------------------
    // LOGOUT
    // ---------------------------------------------------------

    let on_logout = {
        let storage = storage.clone();
        let svc = sync_service.clone();

        move |_| {
            // Clear the stored authentication session.
            storage.clear_session();

            // Clear private application state.
            is_logged_in.set(false);
            portal_data.set(PortalData::default());

            // Reset navigation.
            active_tab.set(Tab::Updates);

            // Reset private UI state.
            password.set(String::new());
            reg_no.set(String::new());
            captcha_input.set(String::new());
            captcha_challenge.set(None);

            selected_faculty.set(None);
            selected_program.set(None);

            status_msg.set("You have successfully logged out.".to_string());

            // Reload captcha for the login screen.
            let svc = svc.clone();

            spawn(async move {
                is_loading.set(true);

                match svc.fetch_captcha().await {
                    Ok(ch) => {
                        captcha_challenge.set(Some(ch));
                    }

                    Err(err) => {
                        status_msg.set(format!("Logged out, but captcha loading failed: {err}"));
                    }
                }

                is_loading.set(false);
            });
        }
    };

    // ---------------------------------------------------------
    // REFRESH PUBLIC DATA
    // ---------------------------------------------------------

    let refresh_public = {
        let svc = sync_service.clone();

        move |_| {
            if public_loading() {
                return;
            }

            let svc = svc.clone();

            spawn(async move {
                public_loading.set(true);
                public_status.set("Refreshing public information...".to_string());

                match svc.refresh_public().await {
                    Ok(()) => {
                        public_data.set(svc.get_public_data());

                        // Reset drill-down pages to avoid stale details.
                        selected_program.set(None);
                        selected_faculty.set(None);

                        public_status.set("Public information refreshed successfully.".to_string());
                    }

                    Err(err) => {
                        public_status.set(format!("Public refresh failed: {err}"));
                    }
                }

                public_loading.set(false);
            });
        }
    };

    // ---------------------------------------------------------
    // APPLICATION UI
    // ---------------------------------------------------------

    rsx! {
        div {
            style: "
                font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
                background: #f1f5f9;
                min-height: 100vh;
                padding: 12px;
                color: #1e293b;
                box-sizing: border-box;
            ",

            // =================================================
            // HEADER
            // =================================================

            div {
                style: "
                    background: #172f68;
                    color: white;
                    padding: 16px 20px;
                    border-radius: 10px;
                    display: flex;
                    justify-content: space-between;
                    align-items: center;
                    flex-wrap: wrap;
                    gap: 12px;
                    margin-bottom: 14px;
                ",

                div {
                    h1 {
                        style: "
                            font-size: 1.35rem;
                            margin: 0;
                            font-weight: 750;
                        ",
                        "NUTS"
                    }

                    p {
                        style: "
                            font-size: 0.78rem;
                            color: #bfdbfe;
                            margin: 4px 0 0 0;
                        ",
                        "SASTRA Student Portal"
                    }
                }

                div {
                    style: "
                        display: flex;
                        align-items: center;
                        gap: 10px;
                    ",

                    if public_loading() {
                        span {
                            style: "font-size: 0.8rem; color: #bfdbfe;",
                            "↻ Syncing..."
                        }
                    }

                    if is_logged_in() {
                        div {
                            style: "
                                background: #1e40af;
                                padding: 6px 10px;
                                border-radius: 6px;
                                font-size: 0.8rem;
                            ",
                            "Student"
                        }

                        button {
                            style: "
                                background: #dc2626;
                                color: white;
                                border: none;
                                padding: 8px 14px;
                                border-radius: 6px;
                                font-weight: 600;
                                cursor: pointer;
                            ",
                            onclick: on_logout,
                            "Logout"
                        }
                    }
                }
            }

            // =================================================
            // STATUS BAR
            // =================================================

            div {
                style: "
                    background: white;
                    border: 1px solid #e2e8f0;
                    padding: 9px 13px;
                    border-radius: 7px;
                    font-size: 0.82rem;
                    margin-bottom: 14px;
                    color: #475569;
                ",

                "{status_msg()}"
            }

            // =================================================
            // PUBLIC NAVIGATION
            // ALWAYS VISIBLE
            // =================================================

            div {
                style: "
                    background: white;
                    padding: 12px;
                    border-radius: 9px;
                    margin-bottom: 12px;
                    border: 1px solid #e2e8f0;
                ",

                div {
                    style: "
                        display: flex;
                        align-items: center;
                        justify-content: space-between;
                        flex-wrap: wrap;
                        gap: 8px;
                        margin-bottom: 10px;
                    ",

                    div {
                        style: "
                            font-size: 0.78rem;
                            font-weight: 700;
                            color: #64748b;
                            text-transform: uppercase;
                            letter-spacing: 0.06em;
                        ",
                        "Public Information"
                    }

                    button {
                        style: "
                            background: #eff6ff;
                            color: #1d4ed8;
                            border: 1px solid #bfdbfe;
                            padding: 6px 11px;
                            border-radius: 6px;
                            font-size: 0.78rem;
                            cursor: pointer;
                        ",
                        disabled: public_loading(),
                        onclick: refresh_public,

                        if public_loading() {
                            "Refreshing..."
                        } else {
                            "↻ Refresh"
                        }
                    }
                }

                div {
                    style: "
                        display: flex;
                        flex-wrap: wrap;
                        gap: 7px;
                    ",

                    TabButton {
                        title: "📢 Updates",
                        active: active_tab() == Tab::Updates,
                        onclick: move |_| {
                            active_tab.set(Tab::Updates);
                        }
                    }

                    TabButton {
                        title: "📚 Curriculum",
                        active: active_tab() == Tab::Curriculum,
                        onclick: move |_| {
                            active_tab.set(Tab::Curriculum);
                            selected_program.set(None);
                        }
                    }

                    TabButton {
                        title: "👨‍🏫 Faculty",
                        active: active_tab() == Tab::Faculty,
                        onclick: move |_| {
                            active_tab.set(Tab::Faculty);
                            selected_faculty.set(None);
                        }
                    }

                    TabButton {
                        title: "📅 Calendar",
                        active: active_tab() == Tab::Calendar,
                        onclick: move |_| {
                            active_tab.set(Tab::Calendar);
                        }
                    }
                }
            }

            // =================================================
            // PRIVATE NAVIGATION
            // ONLY VISIBLE AFTER LOGIN
            // =================================================

            if is_logged_in() {
                div {
                    style: "
                        background: white;
                        padding: 12px;
                        border-radius: 9px;
                        margin-bottom: 12px;
                        border: 1px solid #e2e8f0;
                    ",

                    div {
                        style: "
                            font-size: 0.78rem;
                            font-weight: 700;
                            color: #64748b;
                            text-transform: uppercase;
                            letter-spacing: 0.06em;
                            margin-bottom: 10px;
                        ",
                        "My Student Portal"
                    }

                    div {
                        style: "
                            display: flex;
                            flex-wrap: wrap;
                            gap: 7px;
                        ",

                        TabButton {
                            title: "Profile",
                            active: active_tab() == Tab::Profile,
                            onclick: move |_| active_tab.set(Tab::Profile)
                        }

                        TabButton {
                            title: "Attendance",
                            active: active_tab() == Tab::Attendance,
                            onclick: move |_| active_tab.set(Tab::Attendance)
                        }

                        TabButton {
                            title: "Marks",
                            active: active_tab() == Tab::Marks,
                            onclick: move |_| active_tab.set(Tab::Marks)
                        }

                        TabButton {
                            title: "Grades",
                            active: active_tab() == Tab::Grades,
                            onclick: move |_| active_tab.set(Tab::Grades)
                        }

                        TabButton {
                            title: "Exams",
                            active: active_tab() == Tab::Exams,
                            onclick: move |_| active_tab.set(Tab::Exams)
                        }

                        TabButton {
                            title: "Hostel",
                            active: active_tab() == Tab::Hostel,
                            onclick: move |_| active_tab.set(Tab::Hostel)
                        }

                        TabButton {
                            title: "Absent Hours",
                            active: active_tab() == Tab::Absent,
                            onclick: move |_| active_tab.set(Tab::Absent)
                        }

                        TabButton {
                            title: "Courses",
                            active: active_tab() == Tab::Courses,
                            onclick: move |_| active_tab.set(Tab::Courses)
                        }
                    }
                }
            }

            // =================================================
            // MAIN CONTENT AREA
            // =================================================

            if !is_logged_in() {
                // Login panel and public page are displayed
                // independently in a responsive layout.

                div {
                    style: "
                        display: grid;
                        grid-template-columns: repeat(auto-fit, minmax(min(100%, 320px), 1fr));
                        gap: 14px;
                        align-items: start;
                    ",

                    // LOGIN PANEL

                    div {
                        style: "
                            background: white;
                            padding: 18px;
                            border-radius: 9px;
                            border: 1px solid #e2e8f0;
                            box-shadow: 0 1px 3px rgba(0,0,0,0.04);
                        ",

                        h2 {
                            style: "
                                font-size: 1.1rem;
                                margin: 0 0 5px 0;
                                color: #1e3a8a;
                            ",
                            "Student Login"
                        }

                        p {
                            style: "
                                font-size: 0.8rem;
                                color: #64748b;
                                margin: 0 0 18px 0;
                            ",
                            "Sign in to access your personal academic records."
                        }

                        // Register number

                        div {
                            style: "margin-bottom: 13px;",

                            label {
                                style: "
                                    display: block;
                                    font-size: 0.82rem;
                                    font-weight: 600;
                                    margin-bottom: 5px;
                                ",
                                "Register Number"
                            }

                            input {
                                style: "
                                    width: 100%;
                                    box-sizing: border-box;
                                    padding: 10px;
                                    border: 1px solid #cbd5e1;
                                    border-radius: 6px;
                                    font-size: 0.9rem;
                                ",
                                r#type: "text",
                                placeholder: "Enter register number",
                                value: "{reg_no()}",
                                disabled: is_loading(),
                                oninput: move |e| reg_no.set(e.value()),
                            }
                        }

                        // Password

                        div {
                            style: "margin-bottom: 13px;",

                            label {
                                style: "
                                    display: block;
                                    font-size: 0.82rem;
                                    font-weight: 600;
                                    margin-bottom: 5px;
                                ",
                                "Password"
                            }

                            input {
                                style: "
                                    width: 100%;
                                    box-sizing: border-box;
                                    padding: 10px;
                                    border: 1px solid #cbd5e1;
                                    border-radius: 6px;
                                    font-size: 0.9rem;
                                ",
                                r#type: "password",
                                placeholder: "Enter portal password",
                                value: "{password()}",
                                disabled: is_loading(),
                                oninput: move |e| password.set(e.value()),
                            }
                        }

                        // CAPTCHA

                        div {
                            style: "margin-bottom: 13px;",

                            label {
                                style: "
                                    display: block;
                                    font-size: 0.82rem;
                                    font-weight: 600;
                                    margin-bottom: 7px;
                                ",
                                "Captcha Verification"
                            }

                            if let Some(ch) = captcha_challenge() {
                                div {
                                    style: "
                                        display: flex;
                                        align-items: center;
                                        gap: 10px;
                                        margin-bottom: 9px;
                                        flex-wrap: wrap;
                                    ",

                                    img {
                                        src: "{ch.data_uri}",
                                        alt: "Captcha",
                                        style: "
                                            height: 48px;
                                            max-width: 180px;
                                            object-fit: contain;
                                            border: 1px solid #cbd5e1;
                                            border-radius: 5px;
                                        ",
                                    }

                                    button {
                                        style: "
                                            background: #f1f5f9;
                                            border: 1px solid #cbd5e1;
                                            padding: 8px 11px;
                                            border-radius: 6px;
                                            cursor: pointer;
                                            font-size: 0.8rem;
                                        ",
                                        disabled: is_loading(),
                                        onclick: reload_captcha,
                                        "↻ Reload"
                                    }
                                }
                            } else {
                                p {
                                    style: "
                                        font-size: 0.8rem;
                                        color: #64748b;
                                    ",
                                    "Captcha image is loading..."
                                }
                            }

                            input {
                                style: "
                                    width: 100%;
                                    box-sizing: border-box;
                                    padding: 10px;
                                    border: 1px solid #cbd5e1;
                                    border-radius: 6px;
                                    font-size: 0.9rem;
                                ",
                                r#type: "text",
                                placeholder: "Enter captcha",
                                value: "{captcha_input()}",
                                disabled: is_loading(),
                                oninput: move |e| captcha_input.set(e.value()),
                            }
                        }

                        // LOGIN BUTTON

                        button {
                            style: "
                                width: 100%;
                                background: #2563eb;
                                color: white;
                                padding: 12px;
                                border: none;
                                border-radius: 6px;
                                font-size: 0.92rem;
                                font-weight: 700;
                                cursor: pointer;
                                margin-top: 5px;
                                opacity: if is_loading() { 0.65 } else { 1.0 };
                            ",
                            disabled: is_loading() || captcha_challenge().is_none(),
                            onclick: on_login,

                            if is_loading() {
                                "Please wait..."
                            } else {
                                "Sign In"
                            }
                        }

                        p {
                            style: "
                                font-size: 0.72rem;
                                color: #94a3b8;
                                margin: 12px 0 0 0;
                                text-align: center;
                            ",
                            "Your credentials are used to authenticate with the student portal."
                        }
                    }

                    // PUBLIC CONTENT

                    div {
                        style: "
                            min-width: 0;
                            background: white;
                            padding: 16px;
                            border-radius: 9px;
                            border: 1px solid #e2e8f0;
                            box-shadow: 0 1px 3px rgba(0,0,0,0.04);
                        ",

                        match active_tab() {
                            Tab::Updates => rsx! {
                                UpdatesView {
                                    academic_updates: public_data().academic_updates,
                                    news_events: public_data().news_events,
                                    loading: public_loading(),
                                }
                            },

                            Tab::Curriculum => {
                                if let Some(prog) = selected_program() {
                                    rsx! {
                                        ProgramDetailView {
                                            program: prog,
                                            on_back: move |_| {
                                                selected_program.set(None);
                                            },
                                        }
                                    }
                                } else {
                                    rsx! {
                                        CurriculumView {
                                            list: public_data().curriculum,
                                            loading: public_loading(),
                                            on_select: move |p| {
                                                selected_program.set(Some(p));
                                            },
                                        }
                                    }
                                }
                            }

                            Tab::Faculty => {
                                if let Some(fac) = selected_faculty() {
                                    rsx! {
                                        FacultyDetailView {
                                            member: fac,
                                            on_back: move |_| {
                                                selected_faculty.set(None);
                                            },
                                        }
                                    }
                                } else {
                                    rsx! {
                                        FacultyView {
                                            list: public_data().faculty,
                                            loading: public_loading(),
                                            on_select: move |f| {
                                                selected_faculty.set(Some(f));
                                            },
                                        }
                                    }
                                }
                            }

                            Tab::Calendar => rsx! {
                                CalendarView {
                                    list: public_data().calendar,
                                    loading: public_loading(),
                                }
                            },

                            _ => rsx! {
                                div {
                                    p {
                                        "Please sign in to access this section."
                                    }
                                }
                            },
                        }
                    }
                }
            } else {
                // AUTHENTICATED DASHBOARD

                div {
                    style: "
                        background: white;
                        padding: 18px;
                        border-radius: 9px;
                        border: 1px solid #e2e8f0;
                        box-shadow: 0 1px 3px rgba(0,0,0,0.04);
                        min-width: 0;
                    ",

                    match active_tab() {
                        // PRIVATE PAGES

                        Tab::Profile => rsx! {
                            ProfileView {
                                profile: portal_data().profile
                            }
                        },

                        Tab::Attendance => rsx! {
                            AttendanceView {
                                list: portal_data().attendance
                            }
                        },

                        Tab::Marks => rsx! {
                            MarksView {
                                list: portal_data().marks
                            }
                        },

                        Tab::Grades => rsx! {
                            GradesView {
                                gpa: portal_data().gpa,
                                list: portal_data().grades
                            }
                        },

                        Tab::Exams => rsx! {
                            ExamsView {
                                list: portal_data().exams
                            }
                        },

                        Tab::Hostel => rsx! {
                            HostelView {
                                hostel: portal_data().hostel
                            }
                        },

                        Tab::Absent => rsx! {
                            AbsentHoursView {
                                list: portal_data().absent_hours
                            }
                        },

                        Tab::Courses => rsx! {
                            CoursesView {
                                list: portal_data().course_history
                            }
                        },

                        // PUBLIC PAGES

                        Tab::Updates => rsx! {
                            UpdatesView {
                                academic_updates: public_data().academic_updates,
                                news_events: public_data().news_events,
                                loading: public_loading(),
                            }
                        },

                        Tab::Curriculum => {
                            if let Some(prog) = selected_program() {
                                rsx! {
                                    ProgramDetailView {
                                        program: prog,
                                        on_back: move |_| {
                                            selected_program.set(None);
                                        },
                                    }
                                }
                            } else {
                                rsx! {
                                    CurriculumView {
                                        list: public_data().curriculum,
                                        loading: public_loading(),
                                        on_select: move |p| {
                                            selected_program.set(Some(p));
                                        },
                                    }
                                }
                            }
                        }

                        Tab::Faculty => {
                            if let Some(fac) = selected_faculty() {
                                rsx! {
                                    FacultyDetailView {
                                        member: fac,
                                        on_back: move |_| {
                                            selected_faculty.set(None);
                                        },
                                    }
                                }
                            } else {
                                rsx! {
                                    FacultyView {
                                        list: public_data().faculty,
                                        loading: public_loading(),
                                        on_select: move |f| {
                                            selected_faculty.set(Some(f));
                                        },
                                    }
                                }
                            }
                        }

                        Tab::Calendar => rsx! {
                            CalendarView {
                                list: public_data().calendar,
                                loading: public_loading(),
                            }
                        },
                    }
                }
            }

            // =================================================
            // PUBLIC DATA STATUS
            // =================================================

            div {
                style: "
                    font-size: 0.72rem;
                    color: #94a3b8;
                    text-align: center;
                    padding: 14px 0 4px 0;
                ",
                "{public_status()}"
            }
        }
    }
}
// ─────────────────────────────────────────────────────────────────────────────
// Shared helpers
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn TabButton(title: &'static str, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    let bg = if active { "#1e3a8a" } else { "#e2e8f0" };
    let fg = if active { "white" } else { "#1e293b" };
    rsx! {
        button {
            style: "background: {bg}; color: {fg}; border: none; padding: 8px 12px; \
                    border-radius: 6px; font-size: 0.85rem; font-weight: 600; cursor: pointer;",
            onclick: move |e| onclick.call(e),
            "{title}"
        }
    }
}

#[component]
fn InfoRow(label: &'static str, value: String) -> Element {
    rsx! {
        div {
            style: "display: flex; justify-content: space-between; padding: 8px 0; border-bottom: 1px solid #f1f5f9;",
            span { style: "color: #64748b; font-weight: 500;", "{label}" }
            span { style: "font-weight: 600;", "{value}" }
        }
    }
}

fn card_style() -> &'static str {
    "background: white; padding: 16px; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.1); margin-bottom: 12px;"
}

fn section_title_style() -> &'static str {
    "font-size: 1.1rem; margin-top: 0; margin-bottom: 12px; color: #1e3a8a; font-weight: 700;"
}

fn loading_spinner() -> Element {
    rsx! {
        div {
            style: "text-align: center; padding: 32px; color: #64748b;",
            "🔄 Loading…"
        }
    }
}

fn empty_state(msg: &'static str) -> Element {
    rsx! {
        p { style: "color: #94a3b8; font-style: italic;", "{msg}" }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PUBLIC: Updates (homepage banners)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn UpdatesView(
    academic_updates: Vec<BannerItem>,
    news_events: Vec<BannerItem>,
    loading: bool,
) -> Element {
    rsx! {
        div {
            if loading && academic_updates.is_empty() && news_events.is_empty() {
                { loading_spinner() }
            } else {
                // Academic Updates
                div { style: "{card_style()}",
                    h2 { style: "{section_title_style()}", "📢 Academic Updates" }
                    if academic_updates.is_empty() {
                        { empty_state("No academic updates available.") }
                    } else {
                        div {
                            for item in &academic_updates {
                                BannerCard { item: item.clone() }
                            }
                        }
                    }
                }
                // News & Events
                div { style: "{card_style()}",
                    h2 { style: "{section_title_style()}", "📰 News & Events" }
                    if news_events.is_empty() {
                        { empty_state("No news & events available.") }
                    } else {
                        div {
                            for item in &news_events {
                                BannerCard { item: item.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BannerCard(item: BannerItem) -> Element {
    rsx! {
        div {
            style: "padding: 10px 0; border-bottom: 1px solid #f1f5f9; display: flex; justify-content: space-between; align-items: flex-start;",
            div {
                style: "flex: 1;",
                p { style: "margin: 0; font-size: 0.9rem; color: #1e293b;", "{item.title}" }
            }
            if let Some(url) = &item.url {
                a {
                    href: "{url}",
                    target: "_blank",
                    style: "font-size: 0.75rem; color: #2563eb; margin-left: 12px; white-space: nowrap;",
                    "View →"
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PUBLIC: Curriculum
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn CurriculumView(
    list: Vec<CurriculumProgram>,
    loading: bool,
    on_select: EventHandler<CurriculumProgram>,
) -> Element {
    rsx! {
        div { style: "{card_style()}",
            h2 { style: "{section_title_style()}", "📚 Curriculum & Syllabus" }
            if loading && list.is_empty() {
                { loading_spinner() }
            } else if list.is_empty() {
                { empty_state("Curriculum data not loaded yet. Tap ↺ Refresh above.") }
            } else {
                div {
                    // Group by school
                    {
                        let mut schools: Vec<String> = list.iter().map(|p| p.school.clone()).collect();
                        schools.dedup();
                        rsx! {
                            for school in schools {
                                div { style: "margin-bottom: 20px;",
                                    h3 { style: "font-size: 0.95rem; color: #475569; margin-bottom: 8px; \
                                                 text-transform: uppercase; letter-spacing: 0.05em;",
                                        "{school}"
                                    }
                                    div { style: "display: flex; flex-wrap: wrap; gap: 8px;",
                                        for prog in list.iter().filter(|p| p.school == school) {
                                            {
                                                let prog_clone = prog.clone();
                                                rsx! {
                                                    button {
                                                        style: "background: #eff6ff; border: 1px solid #bfdbfe; \
                                                                padding: 10px 14px; border-radius: 8px; cursor: pointer; \
                                                                text-align: left; min-width: 120px;",
                                                        onclick: move |_| on_select.call(prog_clone.clone()),
                                                        div { style: "font-weight: 700; color: #1e3a8a; font-size: 0.95rem;", "{prog.program}" }
                                                        div { style: "font-size: 0.75rem; color: #64748b; margin-top: 2px;", "{prog.batch}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ProgramDetailView(program: CurriculumProgram, on_back: EventHandler<MouseEvent>) -> Element {
    rsx! {
        div { style: "{card_style()}",
            // Back button
            div { style: "margin-bottom: 16px; display: flex; align-items: center; gap: 10px;",
                button {
                    style: "background: #e2e8f0; border: none; padding: 6px 12px; border-radius: 6px; cursor: pointer;",
                    onclick: on_back,
                    "← Back"
                }
                h2 { style: "font-size: 1.05rem; margin: 0; color: #1e3a8a;",
                    "{program.school} — {program.program} ({program.batch})"
                }
            }

            if program.semesters.is_empty() {
                div {
                    style: "text-align: center; padding: 24px; color: #64748b;",
                    p { "No course details scraped yet." }
                    a {
                        href: "{program.index_url}",
                        target: "_blank",
                        style: "color: #2563eb; font-size: 0.9rem;",
                        "Open full curriculum page →"
                    }
                }
            } else {
                for sem in &program.semesters {
                    div { style: "margin-bottom: 16px;",
                        h3 { style: "font-size: 0.9rem; font-weight: 700; color: #475569; \
                                     border-bottom: 2px solid #e2e8f0; padding-bottom: 6px; margin-bottom: 8px;",
                            "Semester {sem.semester}"
                        }
                        for course in &sem.courses {
                            div {
                                style: "display: flex; justify-content: space-between; align-items: center; \
                                        padding: 8px 0; border-bottom: 1px solid #f8fafc;",
                                div {
                                    div { style: "font-weight: 600; font-size: 0.85rem;", "{course.code}" }
                                    div { style: "font-size: 0.8rem; color: #64748b;", "{course.name}" }
                                }
                                a {
                                    href: "{course.pdf_url}",
                                    target: "_blank",
                                    style: "font-size: 0.75rem; background: #2563eb; color: white; \
                                            padding: 4px 10px; border-radius: 4px; text-decoration: none; white-space: nowrap;",
                                    "📄 PDF"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PUBLIC: Faculty
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn FacultyView(
    list: Vec<FacultyMember>,
    loading: bool,
    on_select: EventHandler<FacultyMember>,
) -> Element {
    let mut search = use_signal(String::new);

    rsx! {
        div { style: "{card_style()}",
            h2 { style: "{section_title_style()}", "👨‍🏫 Faculty Directory" }

            if loading && list.is_empty() {
                { loading_spinner() }
            } else if list.is_empty() {
                { empty_state("Faculty data not loaded yet. Tap ↺ Refresh above.") }
            } else {
                // Search bar
                input {
                    style: "width: 100%; box-sizing: border-box; padding: 8px 12px; \
                            border: 1px solid #cbd5e1; border-radius: 6px; margin-bottom: 14px; font-size: 0.9rem;",
                    r#type: "text",
                    placeholder: "Search by name, department…",
                    value: "{search()}",
                    oninput: move |e| search.set(e.value()),
                }

                // Faculty count
                {
                    let query = search().to_lowercase();
                    let filtered: Vec<&FacultyMember> = list.iter()
                        .filter(|f| {
                            if query.is_empty() { return true; }
                            f.name.to_lowercase().contains(&query)
                                || f.department.to_lowercase().contains(&query)
                                || f.designation.to_lowercase().contains(&query)
                        })
                        .collect();

                    rsx! {
                        p { style: "font-size: 0.75rem; color: #94a3b8; margin-bottom: 10px;",
                            "Showing {filtered.len()} of {list.len()} faculty"
                        }
                        div {
                            for member in filtered {
                                {
                                    let m = member.clone();
                                    rsx! {
                                        div {
                                            style: "display: flex; align-items: center; gap: 12px; \
                                                    padding: 10px; border-bottom: 1px solid #f1f5f9; \
                                                    cursor: pointer; border-radius: 6px; \
                                                    transition: background 0.1s;",
                                            onclick: move |_| on_select.call(m.clone()),
                                            // Photo avatar
                                            div {
                                                style: "width: 44px; height: 44px; border-radius: 50%; \
                                                        background: #dbeafe; display: flex; align-items: center; \
                                                        justify-content: center; font-size: 1.2rem; flex-shrink: 0;",
                                                if let Some(photo) = &member.photo_url {
                                                    img {
                                                        src: "{photo}",
                                                        style: "width: 44px; height: 44px; border-radius: 50%; object-fit: cover;",
                                                        alt: "photo",
                                                    }
                                                } else {
                                                    "👤"
                                                }
                                            }
                                            div { style: "flex: 1;",
                                                div { style: "font-weight: 600; font-size: 0.9rem;", "{member.name}" }
                                                div { style: "font-size: 0.8rem; color: #64748b;", "{member.designation}" }
                                                div { style: "font-size: 0.75rem; color: #94a3b8;", "{member.department}" }
                                            }
                                            span { style: "color: #94a3b8; font-size: 1rem;", "›" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn FacultyDetailView(member: FacultyMember, on_back: EventHandler<MouseEvent>) -> Element {
    rsx! {
        div { style: "{card_style()}",
            // Back
            div { style: "margin-bottom: 16px;",
                button {
                    style: "background: #e2e8f0; border: none; padding: 6px 12px; border-radius: 6px; cursor: pointer;",
                    onclick: on_back,
                    "← Back to Faculty"
                }
            }

            // Header card
            div {
                style: "display: flex; gap: 16px; align-items: flex-start; margin-bottom: 20px; \
                        padding-bottom: 20px; border-bottom: 1px solid #e2e8f0;",
                div {
                    style: "width: 72px; height: 72px; border-radius: 50%; background: #dbeafe; \
                            display: flex; align-items: center; justify-content: center; font-size: 2rem; flex-shrink: 0;",
                    if let Some(photo) = &member.photo_url {
                        img {
                            src: "{photo}",
                            style: "width: 72px; height: 72px; border-radius: 50%; object-fit: cover;",
                            alt: "photo",
                        }
                    } else {
                        "👤"
                    }
                }
                div {
                    h2 { style: "margin: 0 0 4px 0; font-size: 1.1rem; color: #1e3a8a;", "{member.name}" }
                    p { style: "margin: 0 0 2px 0; font-size: 0.9rem; color: #475569;", "{member.designation}" }
                    p { style: "margin: 0; font-size: 0.85rem; color: #94a3b8;", "{member.department}" }
                    if let Some(email) = &member.email {
                        a {
                            href: "mailto:{email}",
                            style: "font-size: 0.85rem; color: #2563eb; display: block; margin-top: 4px;",
                            "✉ {email}"
                        }
                    }
                }
            }

            if let Some(detail) = &member.detail {
                // Qualifications
                if !detail.qualifications.is_empty() {
                    div { style: "margin-bottom: 16px;",
                        h3 { style: "font-size: 0.9rem; font-weight: 700; color: #475569; margin-bottom: 6px;", "🎓 Qualifications" }
                        p { style: "font-size: 0.85rem; color: #334155; margin: 0;", "{detail.qualifications}" }
                    }
                }
                // Experience
                if !detail.experience.is_empty() {
                    div { style: "margin-bottom: 16px;",
                        h3 { style: "font-size: 0.9rem; font-weight: 700; color: #475569; margin-bottom: 6px;", "💼 Experience" }
                        p { style: "font-size: 0.85rem; color: #334155; margin: 0;", "{detail.experience}" }
                    }
                }
                // Research areas
                if !detail.research_areas.is_empty() {
                    div { style: "margin-bottom: 16px;",
                        h3 { style: "font-size: 0.9rem; font-weight: 700; color: #475569; margin-bottom: 6px;", "🔬 Research Areas" }
                        div { style: "display: flex; flex-wrap: wrap; gap: 6px;",
                            for area in &detail.research_areas {
                                span {
                                    style: "background: #eff6ff; color: #1e40af; padding: 3px 10px; \
                                            border-radius: 999px; font-size: 0.8rem;",
                                    "{area}"
                                }
                            }
                        }
                    }
                }
                // Publications
                if !detail.publications.is_empty() {
                    div { style: "margin-bottom: 16px;",
                        h3 { style: "font-size: 0.9rem; font-weight: 700; color: #475569; margin-bottom: 6px;",
                            "📄 Publications ({detail.publications.len()})"
                        }
                        for (i, pub_) in detail.publications.iter().enumerate() {
                            div {
                                style: "font-size: 0.8rem; color: #334155; padding: 6px 0; \
                                        border-bottom: 1px solid #f8fafc;",
                                "{i + 1}. {pub_}"
                            }
                        }
                    }
                }
            } else {
                div {
                    style: "text-align: center; padding: 24px; color: #64748b;",
                    p { "Detailed profile not yet loaded." }
                    a {
                        href: "{member.profile_url}",
                        target: "_blank",
                        style: "color: #2563eb; font-size: 0.9rem;",
                        "View full profile →"
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PUBLIC: Calendar
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn CalendarView(list: Vec<CalendarEntry>, loading: bool) -> Element {
    rsx! {
        div { style: "{card_style()}",
            h2 { style: "{section_title_style()}", "📅 Academic Calendar" }
            if loading && list.is_empty() {
                { loading_spinner() }
            } else if list.is_empty() {
                { empty_state("Calendar data not loaded yet. Tap ↺ Refresh above.") }
            } else {
                div {
                    for entry in &list {
                        div {
                            style: "display: flex; justify-content: space-between; align-items: flex-start; \
                                    padding: 10px 0; border-bottom: 1px solid #f1f5f9;",
                            div { style: "flex: 1; padding-right: 12px;",
                                div { style: "font-size: 0.9rem; font-weight: 600; color: #1e293b;", "{entry.event}" }
                                if !entry.date_range.is_empty() {
                                    div { style: "font-size: 0.8rem; color: #64748b; margin-top: 2px;", "📅 {entry.date_range}" }
                                }
                            }
                            if let Some(pdf_url) = &entry.pdf_url {
                                a {
                                    href: "{pdf_url}",
                                    target: "_blank",
                                    style: "font-size: 0.75rem; background: #2563eb; color: white; \
                                            padding: 4px 10px; border-radius: 4px; text-decoration: none; white-space: nowrap;",
                                    "📄 PDF"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PORTAL: existing authenticated views (unchanged)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn ProfileView(profile: Option<sync::StudentProfile>) -> Element {
    if let Some(p) = profile {
        rsx! {
            div {
                h2 { style: "font-size: 1.1rem; margin-top: 0;", "Student Profile" }
                InfoRow { label: "Student ID", value: p.student_id }
                InfoRow { label: "Name", value: p.name }
                InfoRow { label: "Program", value: p.program }
                InfoRow { label: "Institution", value: p.institution }
                InfoRow { label: "Semester", value: p.semester }
            }
        }
    } else {
        rsx! { p { "No profile information available." } }
    }
}

#[component]
fn AttendanceView(list: Vec<sync::AttendanceRecord>) -> Element {
    rsx! {
        div {
            h2 { style: "font-size: 1.1rem; margin-top: 0;", "Attendance (Course-wise)" }
            if list.is_empty() {
                p { "No attendance records found." }
            } else {
                div {
                    style: "overflow-x: auto;",
                    table {
                        style: "width: 100%; border-collapse: collapse; font-size: 0.85rem;",
                        thead {
                            tr {
                                style: "background: #f8fafc; text-align: left;",
                                th { style: "padding: 8px; border-bottom: 2px solid #e2e8f0;", "Course" }
                                th { style: "padding: 8px; border-bottom: 2px solid #e2e8f0; text-align: center;", "Attended" }
                                th { style: "padding: 8px; border-bottom: 2px solid #e2e8f0; text-align: center;", "%" }
                            }
                        }
                        tbody {
                            for item in list {
                                tr {
                                    style: "border-bottom: 1px solid #f1f5f9;",
                                    td { style: "padding: 8px;",
                                        div { style: "font-weight: 600;", "{item.code}" }
                                        div { style: "color: #64748b; font-size: 0.75rem;", "{item.description}" }
                                    }
                                    td { style: "padding: 8px; text-align: center;", "{item.present} / {item.total}" }
                                    td {
                                        style: "padding: 8px; text-align: center; font-weight: bold; \
                                                color: if item.percentage < 75 { \"#dc2626\" } else { \"#16a34a\" };",
                                        "{item.percentage}%"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn MarksView(list: Vec<sync::InternalMark>) -> Element {
    rsx! {
        div {
            h2 { style: "font-size: 1.1rem; margin-top: 0;", "Internal Marks" }
            if list.is_empty() {
                p { "No marks recorded." }
            } else {
                div {
                    style: "overflow-x: auto;",
                    table {
                        style: "width: 100%; border-collapse: collapse; font-size: 0.85rem;",
                        thead {
                            tr {
                                style: "background: #f8fafc; text-align: left;",
                                th { style: "padding: 8px;", "Course" }
                                th { style: "padding: 8px;", "Component" }
                                th { style: "padding: 8px; text-align: right;", "Marks" }
                            }
                        }
                        tbody {
                            for item in list {
                                tr {
                                    style: "border-bottom: 1px solid #f1f5f9;",
                                    td { style: "padding: 8px;",
                                        div { style: "font-weight: 600;", "{item.code}" }
                                        div { style: "color: #64748b; font-size: 0.75rem;", "{item.description}" }
                                    }
                                    td { style: "padding: 8px;", "{item.component}" }
                                    td { style: "padding: 8px; text-align: right; font-weight: 600;", "{item.marks} / {item.max_marks}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn GradesView(gpa: Option<sync::GpaSummary>, list: Vec<sync::GradeRecord>) -> Element {
    rsx! {
        div {
            h2 { style: "font-size: 1.1rem; margin-top: 0;", "Grades & GPA" }
            if let Some(summary) = gpa {
                div {
                    style: "background: #eff6ff; padding: 12px; border-radius: 6px; margin-bottom: 16px;",
                    if let Some(cgpa) = summary.cgpa {
                        div { style: "font-size: 1.1rem; font-weight: bold; color: #1e40af;", "CGPA: {cgpa}" }
                    }
                    if !summary.sgpa_list.is_empty() {
                        div {
                            style: "display: flex; gap: 12px; margin-top: 8px; flex-wrap: wrap;",
                            for (sem, sgpa) in summary.sgpa_list {
                                span { style: "font-size: 0.85rem; color: #1e3a8a;", "Sem {sem}: {sgpa}" }
                            }
                        }
                    }
                }
            }
            if list.is_empty() {
                p { "No grades recorded." }
            } else {
                div {
                    style: "overflow-x: auto;",
                    table {
                        style: "width: 100%; border-collapse: collapse; font-size: 0.85rem;",
                        thead {
                            tr {
                                style: "background: #f8fafc; text-align: left;",
                                th { style: "padding: 8px;", "Sem" }
                                th { style: "padding: 8px;", "Course" }
                                th { style: "padding: 8px; text-align: center;", "Grade" }
                            }
                        }
                        tbody {
                            for item in list {
                                tr {
                                    style: "border-bottom: 1px solid #f1f5f9;",
                                    td { style: "padding: 8px;", "{item.semester}" }
                                    td { style: "padding: 8px;",
                                        div { style: "font-weight: 600;", "{item.code}" }
                                        div { style: "color: #64748b; font-size: 0.75rem;", "{item.description}" }
                                    }
                                    td { style: "padding: 8px; text-align: center; font-weight: bold;", "{item.grade}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ExamsView(list: Vec<sync::ExamScheduleItem>) -> Element {
    rsx! {
        div {
            h2 { style: "font-size: 1.1rem; margin-top: 0;", "Exam Schedule" }
            if list.is_empty() {
                p { "No upcoming exams scheduled." }
            } else {
                div {
                    style: "overflow-x: auto;",
                    table {
                        style: "width: 100%; border-collapse: collapse; font-size: 0.85rem;",
                        thead {
                            tr {
                                style: "background: #f8fafc; text-align: left;",
                                th { style: "padding: 8px;", "Date / Time" }
                                th { style: "padding: 8px;", "Course" }
                                th { style: "padding: 8px;", "Status" }
                            }
                        }
                        tbody {
                            for item in list {
                                tr {
                                    style: "border-bottom: 1px solid #f1f5f9;",
                                    td { style: "padding: 8px;", "{item.date} {item.time}" }
                                    td { style: "padding: 8px;",
                                        div { style: "font-weight: 600;", "{item.code}" }
                                        div { style: "color: #64748b; font-size: 0.75rem;", "{item.description}" }
                                    }
                                    td { style: "padding: 8px;", "{item.status}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HostelView(hostel: Option<sync::HostelDetails>) -> Element {
    if let Some(h) = hostel {
        rsx! {
            div {
                h2 { style: "font-size: 1.1rem; margin-top: 0;", "Hostel Details" }
                InfoRow { label: "Hostel Name", value: h.hostel_name }
                InfoRow { label: "Room No", value: h.room_no }
                InfoRow { label: "Allocation Date", value: h.allocation_date }
                InfoRow { label: "Mess Name", value: h.mess_name }
            }
        }
    } else {
        rsx! { p { "No hostel allocation found." } }
    }
}

#[component]
fn AbsentHoursView(list: Vec<sync::AbsentHourRecord>) -> Element {
    rsx! {
        div {
            h2 { style: "font-size: 1.1rem; margin-top: 0;", "Absent Hours" }
            if list.is_empty() {
                p { "No absent hours recorded." }
            } else {
                div {
                    style: "overflow-x: auto;",
                    table {
                        style: "width: 100%; border-collapse: collapse; font-size: 0.85rem;",
                        thead {
                            tr {
                                style: "background: #f8fafc; text-align: left;",
                                th { style: "padding: 8px;", "Date" }
                                th { style: "padding: 8px;", "Course" }
                                th { style: "padding: 8px; text-align: center;", "Hour" }
                            }
                        }
                        tbody {
                            for item in list {
                                tr {
                                    style: "border-bottom: 1px solid #f1f5f9;",
                                    td { style: "padding: 8px;", "{item.date}" }
                                    td { style: "padding: 8px;",
                                        div { style: "font-weight: 600;", "{item.code}" }
                                        div { style: "color: #64748b; font-size: 0.75rem;", "{item.description}" }
                                    }
                                    td { style: "padding: 8px; text-align: center;", "{item.hour}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn CoursesView(list: Vec<sync::CourseHistoryItem>) -> Element {
    rsx! {
        div {
            h2 { style: "font-size: 1.1rem; margin-top: 0;", "Course History" }
            if list.is_empty() {
                p { "No course history found." }
            } else {
                div {
                    style: "overflow-x: auto;",
                    table {
                        style: "width: 100%; border-collapse: collapse; font-size: 0.85rem;",
                        thead {
                            tr {
                                style: "background: #f8fafc; text-align: left;",
                                th { style: "padding: 8px;", "Course" }
                                th { style: "padding: 8px; text-align: center;", "Sem" }
                                th { style: "padding: 8px; text-align: center;", "Credits" }
                            }
                        }
                        tbody {
                            for item in list {
                                tr {
                                    style: "border-bottom: 1px solid #f1f5f9;",
                                    td { style: "padding: 8px;",
                                        div { style: "font-weight: 600;", "{item.code}" }
                                        div { style: "color: #64748b; font-size: 0.75rem;", "{item.description}" }
                                    }
                                    td { style: "padding: 8px; text-align: center;", "{item.semester}" }
                                    td { style: "padding: 8px; text-align: center;", "{item.credits}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
