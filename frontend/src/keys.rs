//! Raccourcis clavier.

use leptos::prelude::*;
use rustscanner_shared::tr;
use web_sys::KeyboardEvent;

use crate::state::{AppState, Dialog, View, Zoom};

/// Raccourcis présentés dans l'aide (touches, description).
pub fn shortcuts() -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    vec![
        (
            tr!("Numérisation", "Scanning"),
            vec![
                (tr!("Ctrl + Entrée", "Ctrl + Enter"), tr!("Numériser", "Scan")),
                (tr!("Ctrl + Maj + Entrée", "Ctrl + Shift + Enter"), tr!("Aperçu de la vitre", "Preview the glass")),
                (tr!("Échap", "Esc"), tr!("Annuler la numérisation", "Cancel the scan")),
            ],
        ),
        (
            tr!("Document", "Document"),
            vec![
                ("Ctrl + S", tr!("Enregistrer", "Save")),
                ("Ctrl + P", tr!("Imprimer", "Print")),
                ("Ctrl + N", tr!("Nouveau document", "New document")),
                ("Ctrl + ,", tr!("Paramètres", "Settings")),
            ],
        ),
        (
            tr!("Pages", "Pages"),
            vec![
                ("← → ↑ ↓", tr!("Page précédente / suivante", "Previous / next page")),
                ("Ctrl + A", tr!("Tout sélectionner", "Select all")),
                ("Ctrl + L / Ctrl + R", tr!("Pivoter à gauche / à droite", "Rotate left / right")),
                ("Alt + ↑ / Alt + ↓", tr!("Déplacer la page", "Move the page")),
                (tr!("Suppr", "Del"), tr!("Supprimer", "Delete")),
                ("Ctrl + Z", tr!("Annuler la suppression", "Undo delete")),
            ],
        ),
        (
            tr!("Affichage", "View"),
            vec![
                ("+ / −", tr!("Zoom avant / arrière", "Zoom in / out")),
                ("0", tr!("Page entière", "Fit page")),
                ("W", tr!("Pleine largeur", "Fit width")),
                ("Tab", tr!("Page / zone de numérisation", "Page / scan area")),
                ("F1", tr!("Raccourcis clavier", "Keyboard shortcuts")),
            ],
        ),
    ]
}

fn in_field(ev: &KeyboardEvent) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        .is_some_and(|el| {
            let tag = el.tag_name().to_lowercase();
            matches!(tag.as_str(), "input" | "textarea" | "select") || el.has_attribute("contenteditable")
        })
}

pub fn zoom_step(state: &AppState, factor: f64) {
    let current = match state.zoom.get_untracked() {
        Zoom::Scale(s) => s,
        _ => crate::viewer::fit_scale(),
    };
    state.zoom.set(Zoom::Scale((current * factor).clamp(0.1, 4.0)));
}

pub fn install(state: AppState) {
    let handle = window_event_listener(leptos::ev::keydown, move |ev: KeyboardEvent| {
        let ctrl = ev.ctrl_key() || ev.meta_key();
        let key = ev.key();
        let dialog = state.dialog.get_untracked();
        if dialog.is_some() {
            if key == "Escape" {
                state.dialog.set(None);
                ev.prevent_default();
            }
            return;
        }
        if in_field(&ev) && !(ctrl && key == "Enter") && key != "Escape" {
            return;
        }
        let scanning = state.scan.get_untracked().is_running();
        let handled = match (ctrl, ev.shift_key(), ev.alt_key(), key.as_str()) {
            (true, false, _, "Enter") => {
                state.start_scan(false);
                true
            }
            (true, true, _, "Enter") => {
                state.start_scan(true);
                true
            }
            (_, _, _, "Escape") if scanning => {
                state.cancel_scan();
                true
            }
            (true, _, _, "s" | "S") => {
                if state.has_pages() {
                    state.dialog.set(Some(Dialog::Save));
                }
                true
            }
            (true, _, _, "p" | "P") => {
                if state.has_pages() {
                    state.dialog.set(Some(Dialog::Print));
                }
                true
            }
            (true, _, _, "n" | "N") => {
                state.new_document(false);
                true
            }
            (true, _, _, ",") => {
                state.dialog.set(Some(Dialog::Settings));
                true
            }
            (true, _, _, "a" | "A") => {
                state.select_all();
                true
            }
            (true, _, _, "z" | "Z") => {
                state.undo_delete();
                true
            }
            (true, _, _, "l" | "L") => {
                state.transform("rotate-left");
                true
            }
            (true, _, _, "r" | "R") => {
                state.transform("rotate-right");
                true
            }
            (false, _, true, "ArrowUp" | "ArrowLeft") => {
                state.move_pages(-1);
                true
            }
            (false, _, true, "ArrowDown" | "ArrowRight") => {
                state.move_pages(1);
                true
            }
            (false, _, false, "ArrowLeft" | "ArrowUp" | "PageUp") => {
                state.go(-1);
                true
            }
            (false, _, false, "ArrowRight" | "ArrowDown" | "PageDown") => {
                state.go(1);
                true
            }
            (false, _, false, "Home") => {
                state.go(-10_000);
                true
            }
            (false, _, false, "End") => {
                state.go(10_000);
                true
            }
            (false, _, false, "Delete" | "Backspace") => {
                state.delete_pages();
                true
            }
            (false, _, false, "+" | "=") => {
                zoom_step(&state, 1.25);
                true
            }
            (false, _, false, "-" | "_") => {
                zoom_step(&state, 0.8);
                true
            }
            (false, _, false, "0") => {
                state.zoom.set(Zoom::Fit);
                true
            }
            (false, _, false, "w" | "W") => {
                state.zoom.set(Zoom::Width);
                true
            }
            (false, false, false, "Tab") => {
                state.view.update(|v| *v = if *v == View::Page { View::Area } else { View::Page });
                true
            }
            (_, _, _, "F1") | (false, _, false, "?") => {
                state.dialog.set(Some(Dialog::Shortcuts));
                true
            }
            _ => false,
        };
        if handled {
            ev.prevent_default();
        }
    });
    // écouteur conservé pendant toute la vie de l'application
    std::mem::forget(handle);
}
