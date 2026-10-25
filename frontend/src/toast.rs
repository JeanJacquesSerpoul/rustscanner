//! Notifications éphémères (bas de la fenêtre).

use leptos::prelude::*;
use serde_json::json;

use crate::api;
use crate::icons::Icon;
use crate::state::{ToastAction, ToastKind, use_state};

#[component]
pub fn Toasts() -> impl IntoView {
    let state = use_state();
    view! {
        <div class="toasts" role="status" aria-live="polite">
            <For each=move || state.toasts.get() key=|t| t.id let:t>
                {
                    let id = t.id;
                    let (cls, icon) = match t.kind {
                        ToastKind::Info => ("toast", "info"),
                        ToastKind::Success => ("toast success", "check"),
                        ToastKind::Error => ("toast error", "alert"),
                    };
                    view! {
                        <div class=cls>
                            <span class="toast-icon"><Icon name=icon size=18/></span>
                            <span class="toast-text selectable">{t.text.clone()}</span>
                            {t.actions.iter().cloned().map(|(label, action)| view! {
                                <button class="toast-action" on:click=move |_| {
                                    state.dismiss(id);
                                    match &action {
                                        ToastAction::Undo => state.undo_delete(),
                                        ToastAction::Open(p) => api::fire("open_path", json!({ "path": p })),
                                        ToastAction::Reveal(p) => api::fire("reveal_path", json!({ "path": p })),
                                        ToastAction::Retry => state.start_scan(false),
                                    }
                                }>{label}</button>
                            }).collect_view()}
                            <button class="toast-close" on:click=move |_| state.dismiss(id)>
                                <Icon name="x" size=14/>
                            </button>
                        </div>
                    }
                }
            </For>
        </div>
    }
}
