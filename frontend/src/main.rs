//! Interface de RustScanner (Leptos, compilée en WebAssembly et affichée par Tauri).

mod api;
mod app;
mod dialogs;
mod header;
mod icons;
mod keys;
mod pages;
mod sidebar;
mod state;
mod toast;
mod viewer;

use rustscanner_shared::{AppInfo, Lang};

pub fn document() -> web_sys::Document {
    web_sys::window().and_then(|w| w.document()).expect("document")
}

pub fn set_document_lang(lang: Lang) {
    if let Some(root) = document().document_element() {
        let _ = root.set_attribute("lang", lang.code());
    }
}

/// Thème : "auto" suit le système, sinon force clair ou sombre.
pub fn apply_theme(theme: &str) {
    if let Some(root) = document().document_element() {
        match theme {
            "light" | "dark" => {
                let _ = root.set_attribute("data-theme", theme);
            }
            _ => {
                let _ = root.remove_attribute("data-theme");
            }
        }
    }
}

fn main() {
    console_error_panic_hook::set_once();
    // la langue (choisie par le cœur d'après les réglages ou la locale) doit être
    // connue avant le premier rendu
    wasm_bindgen_futures::spawn_local(async {
        let info = api::call::<AppInfo>("app_info", serde_json::json!({})).await.ok();
        let lang = match info.as_ref().map(|i| i.lang.as_str()) {
            Some("fr") => Lang::Fr,
            _ => Lang::En,
        };
        rustscanner_shared::set_lang(lang);
        set_document_lang(lang);
        if let Some(i) = &info {
            apply_theme(&i.settings.theme);
        }
        leptos::mount::mount_to_body(move || leptos::view! { <app::Root info=info.clone()/> });
    });
}
