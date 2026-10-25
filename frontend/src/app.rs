//! Racine de l'application : état global, événements du cœur et disposition.

use leptos::prelude::*;
use rustscanner_shared::{AppInfo, ScanEvent, tr};

use crate::api;
use crate::dialogs::Dialogs;
use crate::header::Header;
use crate::icons::Icon;
use crate::pages::PagesRail;
use crate::sidebar::Sidebar;
use crate::state::{AppState, Dialog, use_state};
use crate::toast::Toasts;
use crate::viewer::Viewer;

#[component]
pub fn Root(info: Option<AppInfo>) -> impl IntoView {
    let state = AppState::new();
    provide_context(state);

    api::listen::<ScanEvent>("scan", move |ev| state.on_scan_event(ev));
    api::listen::<usize>("close-requested", move |n| state.dialog.set(Some(Dialog::Close(n))));
    api::listen::<serde_json::Value>("export-progress", move |v| {
        let done = v["done"].as_u64().unwrap_or(0) as usize;
        let total = v["total"].as_u64().unwrap_or(0) as usize;
        state.export_progress.set(Some((done, total)));
    });
    crate::keys::install(state);

    // titre de la fenêtre : nombre de pages et état d'enregistrement
    Effect::new(move |_| {
        let n = state.pages.with(|p| p.len());
        let dirty = state.dirty.get();
        state.lang.track();
        let title = match n {
            0 => "RustScanner".to_string(),
            1 => format!("{}{} — RustScanner", if dirty { "• " } else { "" }, tr!("1 page", "1 page")),
            n => format!("{}{n} pages — RustScanner", if dirty { "• " } else { "" }),
        };
        api::set_title(&title);
    });

    match info {
        Some(info) => state.boot(info),
        None => state.toast(
            crate::state::ToastKind::Error,
            tr!("Impossible de joindre le cœur de l'application", "Cannot reach the application core"),
            Vec::new(),
        ),
    }

    // changement de langue : toute l'interface est reconstruite (l'état est conservé)
    view! {
        {move || {
            state.lang.track();
            view! { <App/> }
        }}
    }
}

#[component]
fn App() -> impl IntoView {
    let state = use_state();
    let no_sane = move || state.info.with(|i| i.as_ref().is_some_and(|i| !i.sane));
    view! {
        <div class="app">
            <Header/>
            <Show when=no_sane fallback=|| view! {
                <main class="workspace">
                    <Sidebar/>
                    <Viewer/>
                    <PagesRail/>
                </main>
            }>
                <NoSane/>
            </Show>
            <Dialogs/>
            <Toasts/>
        </div>
    }
}

#[component]
fn NoSane() -> impl IntoView {
    view! {
        <main class="workspace single">
            <div class="empty-state big">
                <div class="empty-icon warn"><Icon name="alert" size=40/></div>
                <h2>{tr!("SANE n'est pas installé", "SANE is not installed")}</h2>
                <p>
                    {tr!(
                        "RustScanner s'appuie sur SANE pour piloter les scanners. Installez-le puis relancez l'application :",
                        "RustScanner relies on SANE to drive scanners. Install it, then restart the application:"
                    )}
                </p>
                <pre class="code selectable">"sudo apt install sane-utils sane-airscan"</pre>
                <p class="muted small">
                    {tr!(
                        "Les scanners réseau récents (eSCL / AirScan) sont pris en charge par sane-airscan.",
                        "Recent network scanners (eSCL / AirScan) are supported by sane-airscan."
                    )}
                </p>
            </div>
        </main>
    }
}
