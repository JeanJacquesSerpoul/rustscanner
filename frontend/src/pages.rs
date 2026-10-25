//! Colonne des pages : vignettes, sélection multiple, glisser-déposer.

use leptos::prelude::*;
use rustscanner_shared::{PageInfo, tr, trf};
use web_sys::DragEvent;

use crate::api;
use crate::icons::Icon;
use crate::state::{Scan, use_state};

/// Cible du dépôt : avant la page indiquée, ou à la fin (`None`).
#[derive(Clone, Copy, PartialEq, Debug)]
struct DropAt(Option<u64>);

#[component]
pub fn PagesRail() -> impl IntoView {
    let state = use_state();
    let dragging = RwSignal::new(Vec::<u64>::new());
    let drop_at = RwSignal::new(None::<DropAt>);
    let count = move || state.pages.with(|p| p.len());
    let selected = move || state.selected.with(|s| s.len());
    let scanning = move || matches!(state.scan.get(), Scan::Running { preview: false, .. });
    let indexed = move || state.pages.get().into_iter().enumerate().collect::<Vec<_>>();

    let on_drop_end = move |ev: DragEvent| {
        ev.prevent_default();
        let ids = dragging.get_untracked();
        if let Some(DropAt(before)) = drop_at.get_untracked()
            && !ids.is_empty()
        {
            state.drop_pages(ids, before);
        }
        dragging.set(Vec::new());
        drop_at.set(None);
    };

    view! {
        <aside class="pages-rail">
            <div class="rail-head">
                <span class="rail-title">{tr!("Pages", "Pages")}</span>
                <span class="badge">{count}</span>
                <span class="grow"></span>
                <button class="btn icon-only ghost tiny" title=tr!("Tout sélectionner (Ctrl+A)", "Select all (Ctrl+A)")
                    disabled=move || count() == 0 on:click=move |_| state.select_all()>
                    <Icon name="select-all" size=16/>
                </button>
            </div>
            <div
                class="rail-list"
                on:dragover=move |ev: DragEvent| {
                    if !dragging.with_untracked(|d| d.is_empty()) {
                        ev.prevent_default();
                    }
                }
                on:drop=on_drop_end
            >
                <Show when=move || count() == 0 && !scanning()>
                    <div class="rail-empty">
                        <Icon name="layers" size=28/>
                        <p>{tr!("Les pages numérisées apparaîtront ici.", "Scanned pages will show up here.")}</p>
                    </div>
                </Show>
                <For
                    each=indexed
                    key=|(i, p)| (p.id, p.rev, *i)
                    children=move |(i, p)| view! { <Thumb index=i page=p dragging=dragging drop_at=drop_at/> }
                />
                <Show when=scanning>
                    <div class="thumb placeholder">
                        <div class="thumb-image scanning" style="aspect-ratio: 210 / 297">
                            <div class="scanning-beam moving"></div>
                        </div>
                        <div class="thumb-caption">
                            <span>{move || match state.scan.get() {
                                Scan::Running { fraction, .. } if fraction >= 0.0 => format!("{} %", (fraction * 100.0).round()),
                                _ => "…".to_string(),
                            }}</span>
                        </div>
                    </div>
                </Show>
                <div
                    class="drop-end"
                    class:active=move || drop_at.get() == Some(DropAt(None))
                    on:dragover=move |ev: DragEvent| {
                        if !dragging.with_untracked(|d| d.is_empty()) {
                            ev.prevent_default();
                            drop_at.set(Some(DropAt(None)));
                        }
                    }
                ></div>
            </div>
            <Show when=move || { (selected() > 1) || (count() > 1) }>
                <div class="rail-foot">
                    <span class="muted small">{move || {
                        let n = selected();
                        if n > 1 { trf!("{n} sélectionnées", "{n} selected") } else { tr!("Glissez pour trier", "Drag to reorder").to_string() }
                    }}</span>
                    <span class="grow"></span>
                    <button class="btn icon-only ghost tiny" title=tr!("Monter (Alt+↑)", "Move up (Alt+↑)") on:click=move |_| state.move_pages(-1)>
                        <Icon name="arrow-up" size=16/>
                    </button>
                    <button class="btn icon-only ghost tiny" title=tr!("Descendre (Alt+↓)", "Move down (Alt+↓)") on:click=move |_| state.move_pages(1)>
                        <Icon name="arrow-down" size=16/>
                    </button>
                </div>
            </Show>
        </aside>
    }
}

#[component]
fn Thumb(index: usize, page: PageInfo, dragging: RwSignal<Vec<u64>>, drop_at: RwSignal<Option<DropAt>>) -> impl IntoView {
    let state = use_state();
    let id = page.id;
    let is_current = move || state.current.get() == Some(id);
    let is_selected = move || state.selected.with(|s| s.contains(&id));
    let is_dragged = move || dragging.with(|d| d.contains(&id));
    let src = api::image_url("thumb", id, page.rev);
    let aspect = format!("aspect-ratio: {} / {}", page.width.max(1), page.height.max(1));
    let (img, loaded) = crate::icons::image_loaded();

    let on_click = move |ev: web_sys::MouseEvent| {
        state.click_page(id, ev.ctrl_key() || ev.meta_key(), ev.shift_key());
    };
    let on_drag_start = move |ev: DragEvent| {
        let ids = if state.selected.with_untracked(|s| s.contains(&id)) { state.targets() } else { vec![id] };
        if let Some(dt) = ev.data_transfer() {
            let _ = dt.set_data("text/plain", &format!("{ids:?}"));
            dt.set_effect_allowed("move");
        }
        dragging.set(ids);
    };
    let on_drag_over = move |ev: DragEvent| {
        if dragging.with_untracked(|d| d.is_empty()) {
            return;
        }
        ev.prevent_default();
        // moitié haute : avant cette page ; moitié basse : avant la suivante
        use wasm_bindgen::JsCast;
        let Some(el) = ev.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) else { return };
        let r = el.get_bounding_client_rect();
        let lower = (ev.client_y() as f64) > r.top() + r.height() / 2.0;
        let target = if lower {
            state.pages.with_untracked(|p| {
                let pos = p.iter().position(|x| x.id == id).unwrap_or(0);
                p.get(pos + 1).map(|x| x.id)
            })
        } else {
            Some(id)
        };
        drop_at.set(Some(DropAt(target)));
    };
    let on_drag_end = move |_| {
        dragging.set(Vec::new());
        drop_at.set(None);
    };
    view! {
        <div
            class="thumb"
            class:current=is_current
            class:selected=is_selected
            class:dragged=is_dragged
            class:drop-before=move || drop_at.get() == Some(DropAt(Some(id)))
            draggable="true"
            on:click=on_click
            on:dragstart=on_drag_start
            on:dragover=on_drag_over
            on:dragend=on_drag_end
            on:dblclick=move |_| state.zoom.set(crate::state::Zoom::Fit)
            title=tr!("Ctrl+clic : sélection multiple · Maj+clic : plage", "Ctrl+click: multi-select · Shift+click: range")
        >
            <div class="thumb-image" class:loaded=move || loaded.get() style=aspect>
                <img node_ref=img src=src alt="" draggable="false" on:load=move |_| loaded.set(true)/>
                <div class="thumb-actions" on:click=|ev| ev.stop_propagation()>
                    <button class="btn icon-only tiny" title=tr!("Pivoter à droite", "Rotate right") on:click=move |_| {
                        state.click_page(id, false, false);
                        state.transform("rotate-right");
                    }>
                        <Icon name="rotate-right" size=14/>
                    </button>
                    <button class="btn icon-only tiny danger-hover" title=tr!("Supprimer", "Delete") on:click=move |_| {
                        state.click_page(id, false, false);
                        state.delete_pages();
                    }>
                        <Icon name="trash" size=14/>
                    </button>
                </div>
                <Show when=is_selected>
                    <span class="thumb-check"><Icon name="check" size=12/></span>
                </Show>
            </div>
            <div class="thumb-caption">
                <span class="thumb-number">{index + 1}</span>
            </div>
        </div>
    }
}
