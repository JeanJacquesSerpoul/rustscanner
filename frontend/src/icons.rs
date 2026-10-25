//! Icônes vectorielles (traits 24×24, couleur héritée du texte).

use leptos::prelude::*;

fn paths(name: &str) -> &'static str {
    match name {
        "scan" => {
            r#"<path d="M4 15.5h16a1 1 0 0 1 1 1V19a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1v-2.5a1 1 0 0 1 1-1z"/><path d="M6.5 15.5V5.8a1.8 1.8 0 0 1 1.8-1.8h7.4a1.8 1.8 0 0 1 1.8 1.8v9.7"/><path d="M2.5 12h19"/><path d="M17 17.8h1"/>"#
        }
        "eye" => r#"<path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>"#,
        "save" => r#"<path d="M12 3v12"/><path d="m7 10 5 5 5-5"/><path d="M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2"/>"#,
        "print" => {
            r#"<path d="M6.5 9V3.5h11V9"/><rect x="3" y="9" width="18" height="8" rx="2"/><path d="M6.5 14h11v6.5h-11z"/><path d="M17 12h.01"/>"#
        }
        "settings" => {
            r#"<path d="M12 15.2a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4z"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.6 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.6-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/>"#
        }
        "rotate-left" => r#"<path d="M3 4v5h5"/><path d="M3.5 9A9 9 0 1 1 5 17.2"/>"#,
        "rotate-right" => r#"<path d="M21 4v5h-5"/><path d="M20.5 9A9 9 0 1 0 19 17.2"/>"#,
        "rotate-180" => r#"<path d="M3 12a9 9 0 0 1 15.5-6.2L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-15.5 6.2L3 16"/><path d="M3 21v-5h5"/>"#,
        "flip-h" => r#"<path d="M12 3v18" stroke-dasharray="2 2.4"/><path d="M8.5 7 3 17h5.5z"/><path d="M15.5 7 21 17h-5.5z"/>"#,
        "flip-v" => r#"<path d="M3 12h18" stroke-dasharray="2 2.4"/><path d="M7 8.5 17 3v5.5z"/><path d="M7 15.5 17 21v-5.5z"/>"#,
        "trash" => {
            r#"<path d="M3.5 6h17"/><path d="M8.5 6V4.5A1.5 1.5 0 0 1 10 3h4a1.5 1.5 0 0 1 1.5 1.5V6"/><path d="M18.5 6l-.8 13.1a2 2 0 0 1-2 1.9H8.3a2 2 0 0 1-2-1.9L5.5 6"/><path d="M10 10.5v6M14 10.5v6"/>"#
        }
        "undo" => r#"<path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11"/>"#,
        "zoom-in" => r#"<circle cx="11" cy="11" r="7"/><path d="m20.5 20.5-4.5-4.5M11 8v6M8 11h6"/>"#,
        "zoom-out" => r#"<circle cx="11" cy="11" r="7"/><path d="m20.5 20.5-4.5-4.5M8 11h6"/>"#,
        "fit" => r#"<path d="M8 3H5a2 2 0 0 0-2 2v3M21 8V5a2 2 0 0 0-2-2h-3M3 16v3a2 2 0 0 0 2 2h3M16 21h3a2 2 0 0 0 2-2v-3"/>"#,
        "fit-width" => r#"<path d="M3 5v14M21 5v14"/><path d="m7 9-3 3 3 3M17 9l3 3-3 3M4 12h16"/>"#,
        "chevron-down" => r#"<path d="m6 9 6 6 6-6"/>"#,
        "chevron-left" => r#"<path d="m15 18-6-6 6-6"/>"#,
        "chevron-right" => r#"<path d="m9 18 6-6-6-6"/>"#,
        "chevron-up" => r#"<path d="m18 15-6-6-6 6"/>"#,
        "refresh" => r#"<path d="M20 12a8 8 0 1 1-2.6-5.9"/><path d="M20 4v5h-5"/>"#,
        "check" => r#"<path d="M20 6 9 17l-5-5"/>"#,
        "x" => r#"<path d="M18 6 6 18M6 6l12 12"/>"#,
        "plus" => r#"<path d="M12 5v14M5 12h14"/>"#,
        "file-plus" => {
            r#"<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/><path d="M12 11.5v6M9 14.5h6"/>"#
        }
        "file" => r#"<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/><path d="M9 13h6M9 17h4"/>"#,
        "image" => r#"<rect x="3" y="3" width="18" height="18" rx="2.5"/><circle cx="9" cy="9" r="1.8"/><path d="m21 15-4.5-4.5L6 21"/>"#,
        "layers" => r#"<path d="m12 3 9 5-9 5-9-5z"/><path d="m3 13 9 5 9-5"/>"#,
        "wifi" => r#"<path d="M5 12.5a10 10 0 0 1 14 0M8.5 16a5 5 0 0 1 7 0M2 9a14.5 14.5 0 0 1 20 0"/><path d="M12 19.5h.01"/>"#,
        "usb" => {
            r#"<circle cx="12" cy="19" r="2"/><path d="M12 17V4M9 7l3-3 3 3"/><path d="M12 14l-4-2.5V9M12 12.5l4-2.5V8"/><rect x="15" y="6" width="2" height="2"/><circle cx="8" cy="8.2" r="1"/>"#
        }
        "camera" => r#"<path d="M4 7h3l2-2.5h6L17 7h3a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V8a1 1 0 0 1 1-1z"/><circle cx="12" cy="13" r="3.8"/>"#,
        "cube" => r#"<path d="m12 2.5 8.5 4.75v9.5L12 21.5l-8.5-4.75v-9.5z"/><path d="m3.5 7.25 8.5 4.75 8.5-4.75M12 12v9.5"/>"#,
        "info" => r#"<circle cx="12" cy="12" r="9"/><path d="M12 11v5.5M12 7.5h.01"/>"#,
        "alert" => r#"<path d="M10.3 3.9 2.4 17.5A2 2 0 0 0 4.1 20.5h15.8a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"/><path d="M12 9v4.5M12 17h.01"/>"#,
        "sun" => r#"<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>"#,
        "moon" => r#"<path d="M20.5 14.5A8.5 8.5 0 1 1 9.5 3.5a7 7 0 0 0 11 11z"/>"#,
        "monitor" => r#"<rect x="2.5" y="3.5" width="19" height="13" rx="2"/><path d="M8 20.5h8M12 16.5v4"/>"#,
        "keyboard" => {
            r#"<rect x="2" y="5.5" width="20" height="13" rx="2"/><path d="M6 9.5h.01M10 9.5h.01M14 9.5h.01M18 9.5h.01M6 13h.01M18 13h.01M10 13h4M7 15.5h10"/>"#
        }
        "folder" => r#"<path d="M3 7a2 2 0 0 1 2-2h4.2l2 2H19a2 2 0 0 1 2 2v8.5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>"#,
        "external" => r#"<path d="M14 4h6v6"/><path d="M20 4 10 14"/><path d="M19 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h5"/>"#,
        "crop" => r#"<path d="M6 2v14a2 2 0 0 0 2 2h14"/><path d="M18 22V8a2 2 0 0 0-2-2H2"/>"#,
        "area" => {
            r#"<path d="M4 8V5a1 1 0 0 1 1-1h3M16 4h3a1 1 0 0 1 1 1v3M20 16v3a1 1 0 0 1-1 1h-3M8 20H5a1 1 0 0 1-1-1v-3"/><rect x="8" y="8" width="8" height="8" rx="1"/>"#
        }
        "sliders" => {
            r#"<path d="M4 6h9M17 6h3M4 12h3M11 12h9M4 18h11M19 18h1"/><circle cx="15" cy="6" r="2"/><circle cx="9" cy="12" r="2"/><circle cx="17" cy="18" r="2"/>"#
        }
        "palette" => {
            r#"<path d="M12 21a9 9 0 1 1 9-9c0 2.5-2 3.5-4 3.5h-1.6a1.5 1.5 0 0 0-1.1 2.5 1.5 1.5 0 0 1-1.1 3z"/><circle cx="7.5" cy="11" r="1.2" class="fill"/><circle cx="10.5" cy="7" r="1.2" class="fill"/><circle cx="15" cy="7.5" r="1.2" class="fill"/>"#
        }
        "contrast" => r#"<circle cx="12" cy="12" r="9"/><path class="fill" d="M12 3a9 9 0 0 1 0 18z"/>"#,
        "text" => r#"<path d="M5 6V4.5h14V6M12 4.5v15M9 19.5h6"/>"#,
        "feeder" => {
            r#"<path d="M6 3h12v9H6z"/><path d="M9 7h6"/><path d="M3 14h18v6a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z"/><path d="M12 12v4M10 14.5l2 2 2-2"/>"#
        }
        "flatbed" => r#"<rect x="3" y="9" width="18" height="10" rx="1.5"/><path d="M3 13h18"/><path d="M6 9 8 5h8l2 4"/>"#,
        "duplex" => r#"<path d="M7 3h8l3 3v9H7z"/><path d="M4 7v13h11"/><path d="M10 9h5M10 12h5"/>"#,
        "globe" => r#"<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.5 2.6 3.7 5.6 3.7 9s-1.2 6.4-3.7 9c-2.5-2.6-3.7-5.6-3.7-9S9.5 5.6 12 3z"/>"#,
        "arrow-up" => r#"<path d="M12 19V5M6 11l6-6 6 6"/>"#,
        "arrow-down" => r#"<path d="M12 5v14M6 13l6 6 6-6"/>"#,
        "select-all" => {
            r#"<rect x="3" y="3" width="18" height="18" rx="2.5" stroke-dasharray="3 2.6"/><path d="m8 12 3 3 5-6"/>"#
        }
        "stop" => r#"<rect x="6" y="6" width="12" height="12" rx="2"/>"#,
        "page-size" => r#"<path d="M6 3h12v18H6z"/><path d="M9 7h6"/><path d="m4 9-2 2 2 2M20 9l2 2-2 2"/>"#,
        "dpi" => r#"<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18M3 15h18M9 3v18M15 3v18"/>"#,
        "swap" => r#"<path d="M7 4v13M3.5 13.5 7 17l3.5-3.5"/><path d="M17 20V7M13.5 10.5 17 7l3.5 3.5"/>"#,
        "sparkles" => {
            r#"<path d="M12 3.5 13.6 8.4 18.5 10l-4.9 1.6L12 16.5l-1.6-4.9L5.5 10l4.9-1.6z"/><path d="M19 15.5 19.7 17.3 21.5 18l-1.8.7-.7 1.8-.7-1.8-1.8-.7 1.8-.7z"/>"#
        }
        _ => r#"<circle cx="12" cy="12" r="8"/>"#,
    }
}

/// Icône `name` ; `size` en pixels.
#[component]
pub fn Icon(#[prop(into)] name: String, #[prop(default = 18)] size: u32) -> impl IntoView {
    view! {
        <svg
            class="icon"
            width=size
            height=size
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
            inner_html=paths(&name)
        ></svg>
    }
}

#[component]
pub fn Spinner(#[prop(default = 16)] size: u32) -> impl IntoView {
    view! { <span class="spinner" style=format!("width:{size}px;height:{size}px")></span> }
}

/// Suivi du chargement d'une image : l'événement `load` peut survenir avant
/// que le gestionnaire soit posé (image en cache), d'où la vérification de
/// `complete` une fois l'élément monté.
pub fn image_loaded() -> (NodeRef<leptos::html::Img>, RwSignal<bool>) {
    let img = NodeRef::<leptos::html::Img>::new();
    let loaded = RwSignal::new(false);
    Effect::new(move |_| {
        if let Some(el) = img.get()
            && el.complete()
            && el.natural_width() > 0
        {
            loaded.set(true);
        }
    });
    (img, loaded)
}
