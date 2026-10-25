//! Zone centrale : affichage de la page (zoom, transformations) ou réglage de
//! la zone de numérisation sur l'aperçu de la vitre.

use leptos::html;
use leptos::prelude::*;
use rustscanner_shared::{Area, page_sizes, tr, trf};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::PointerEvent;

use crate::api;
use crate::icons::{Icon, Spinner};
use crate::keys::zoom_step;
use crate::state::{Scan, View, Zoom, use_state};

/// Pixels CSS par pouce : à 100 %, la page s'affiche à sa taille réelle.
const CSS_DPI: f64 = 96.0;
const STAGE_PADDING: f64 = 48.0;

thread_local! {
    static STAGE: std::cell::Cell<(f64, f64)> = const { std::cell::Cell::new((800.0, 600.0)) };
}

/// Taille physique de la page affichée, en pixels CSS à 100 %.
fn natural_size(width: u32, height: u32, dpi: f64) -> (f64, f64) {
    let dpi = if dpi > 0.0 { dpi } else { 300.0 };
    (width as f64 / dpi * CSS_DPI, height as f64 / dpi * CSS_DPI)
}

/// Facteur qui fait tenir la page courante dans la zone d'affichage.
pub fn fit_scale() -> f64 {
    let state = use_state();
    let Some(p) = state.pages.with_untracked(|pages| {
        let id = state.current.get_untracked()?;
        pages.iter().find(|x| x.id == id).cloned()
    }) else {
        return 1.0;
    };
    let (w, h) = natural_size(p.width, p.height, p.dpi);
    let (sw, sh) = STAGE.with(|s| s.get());
    ((sw - STAGE_PADDING) / w).min((sh - STAGE_PADDING) / h).max(0.05)
}

fn fmt_mm(v: f64) -> String {
    format!("{}", v.round() as i64)
}

/// Nom du format standard le plus proche (« A4 », « US Letter »…).
fn size_name(w_mm: f64, h_mm: f64) -> Option<&'static str> {
    page_sizes().into_iter().find_map(|(_, label, w, h)| {
        let close = |a: f64, b: f64| (a - b).abs() < 4.0;
        ((close(w_mm, w) && close(h_mm, h)) || (close(w_mm, h) && close(h_mm, w))).then_some(label)
    })
}

#[component]
pub fn Viewer() -> impl IntoView {
    let state = use_state();
    let stage: NodeRef<html::Div> = NodeRef::new();
    let stage_size = RwSignal::new((800.0, 600.0));

    // taille de la zone d'affichage (pour « page entière » et le pourcentage affiché)
    Effect::new(move |_| {
        let Some(el) = stage.get() else { return };
        let update = Closure::<dyn FnMut(js_sys::Array)>::new(move |entries: js_sys::Array| {
            if let Some(entry) = entries.get(0).dyn_ref::<web_sys::ResizeObserverEntry>() {
                let r = entry.content_rect();
                STAGE.with(|s| s.set((r.width(), r.height())));
                stage_size.set((r.width(), r.height()));
            }
        });
        if let Ok(obs) = web_sys::ResizeObserver::new(update.as_ref().unchecked_ref()) {
            obs.observe(&el);
            on_cleanup(move || obs.disconnect());
        }
        update.forget();
    });

    let on_wheel = move |ev: web_sys::WheelEvent| {
        if ev.ctrl_key() && state.view.get_untracked() == View::Page {
            ev.prevent_default();
            zoom_step(&state, if ev.delta_y() < 0.0 { 1.1 } else { 1.0 / 1.1 });
        }
    };

    view! {
        <section class="viewer">
            <ViewerToolbar stage_size=stage_size/>
            <div class="viewer-stage" node_ref=stage on:wheel=on_wheel class:area-mode=move || state.view.get() == View::Area>
                {move || match state.view.get() {
                    View::Page => view! { <PageView stage_size=stage_size/> }.into_any(),
                    View::Area => view! { <AreaEditor stage_size=stage_size/> }.into_any(),
                }}
            </div>
            <StatusBar/>
        </section>
    }
}

#[component]
fn ViewerToolbar(stage_size: RwSignal<(f64, f64)>) -> impl IntoView {
    let state = use_state();
    let has_page = move || state.current.get().is_some();
    let can_area = move || state.geometry().is_some();
    let zoom_label = move || {
        stage_size.track();
        if state.current.get().is_none() {
            return "—".to_string();
        }
        let scale = match state.zoom.get() {
            Zoom::Scale(s) => s,
            Zoom::Fit => fit_scale(),
            Zoom::Width => {
                let Some(p) = state.current_page() else { return "—".to_string() };
                let (w, _) = natural_size(p.width, p.height, p.dpi);
                (stage_size.get().0 - STAGE_PADDING) / w
            }
        };
        format!("{} %", (scale * 100.0).round())
    };
    let n_targets = move || {
        state.selected.track();
        state.current.track();
        state.targets().len()
    };
    view! {
        <div class="viewer-toolbar">
            <div class="tabs" role="tablist">
                <button role="tab" class:active=move || state.view.get() == View::Page on:click=move |_| state.view.set(View::Page)>
                    <Icon name="file" size=16/>{tr!("Page", "Page")}
                </button>
                <button
                    role="tab"
                    class:active=move || state.view.get() == View::Area
                    disabled=move || !can_area()
                    on:click=move |_| state.view.set(View::Area)
                >
                    <Icon name="area" size=16/>{tr!("Zone de numérisation", "Scan area")}
                </button>
            </div>
            <div class="spacer"></div>
            <Show
                when=move || state.view.get() == View::Page
                fallback=move || view! {
                    <div class="toolbar-group">
                        <button class="btn ghost" disabled=move || state.scan.with(|s| s.is_running()) || !state.is_open() on:click=move |_| state.start_scan(true)>
                            <Icon name="refresh"/>
                            <span class="label">{move || if state.preview.get().is_some() { tr!("Actualiser l'aperçu", "Refresh preview") } else { tr!("Lancer l'aperçu", "Run preview") }}</span>
                        </button>
                        <button class="btn ghost" on:click=move |_| {
                            if let Some((gw, gh, _)) = state.geometry() {
                                state.set_area(Area { x: 0.0, y: 0.0, w: gw, h: gh });
                            }
                        }>
                            <Icon name="fit"/>
                            <span class="label">{tr!("Toute la vitre", "Whole glass")}</span>
                        </button>
                    </div>
                }
            >
                <div class="toolbar-group">
                    <button class="btn icon-only ghost" disabled=move || !has_page() title=tr!("Pivoter à gauche (Ctrl+L)", "Rotate left (Ctrl+L)") on:click=move |_| state.transform("rotate-left")>
                        <Icon name="rotate-left"/>
                    </button>
                    <button class="btn icon-only ghost" disabled=move || !has_page() title=tr!("Pivoter à droite (Ctrl+R)", "Rotate right (Ctrl+R)") on:click=move |_| state.transform("rotate-right")>
                        <Icon name="rotate-right"/>
                    </button>
                    <button class="btn icon-only ghost" disabled=move || !has_page() title=tr!("Retourner (180°)", "Turn upside down (180°)") on:click=move |_| state.transform("rotate-180")>
                        <Icon name="rotate-180"/>
                    </button>
                    <button class="btn icon-only ghost" disabled=move || !has_page() title=tr!("Miroir horizontal", "Mirror horizontally") on:click=move |_| state.transform("flip-horizontal")>
                        <Icon name="flip-h"/>
                    </button>
                    <button class="btn icon-only ghost" disabled=move || !has_page() title=tr!("Miroir vertical", "Mirror vertically") on:click=move |_| state.transform("flip-vertical")>
                        <Icon name="flip-v"/>
                    </button>
                    <div class="divider-v"></div>
                    <button class="btn ghost danger-hover" disabled=move || !has_page() title=tr!("Supprimer (Suppr)", "Delete (Del)") on:click=move |_| state.delete_pages()>
                        <Icon name="trash"/>
                        <Show when=move || { n_targets() > 1 }>
                            <span class="badge">{n_targets}</span>
                        </Show>
                    </button>
                </div>
                <div class="divider-v"></div>
                <div class="toolbar-group zoom">
                    <button class="btn icon-only ghost" disabled=move || !has_page() title=tr!("Zoom arrière (−)", "Zoom out (−)") on:click=move |_| zoom_step(&state, 0.8)>
                        <Icon name="zoom-out"/>
                    </button>
                    <span class="zoom-value mono">{zoom_label}</span>
                    <button class="btn icon-only ghost" disabled=move || !has_page() title=tr!("Zoom avant (+)", "Zoom in (+)") on:click=move |_| zoom_step(&state, 1.25)>
                        <Icon name="zoom-in"/>
                    </button>
                    <button class="btn icon-only ghost" class:active=move || state.zoom.get() == Zoom::Fit disabled=move || !has_page() title=tr!("Page entière (0)", "Fit page (0)") on:click=move |_| state.zoom.set(Zoom::Fit)>
                        <Icon name="fit"/>
                    </button>
                    <button class="btn icon-only ghost" class:active=move || state.zoom.get() == Zoom::Width disabled=move || !has_page() title=tr!("Pleine largeur (W)", "Fit width (W)") on:click=move |_| state.zoom.set(Zoom::Width)>
                        <Icon name="fit-width"/>
                    </button>
                    <button class="btn ghost small" class:active=move || matches!(state.zoom.get(), Zoom::Scale(s) if (s - 1.0).abs() < 0.001) disabled=move || !has_page() title=tr!("Taille réelle", "Actual size") on:click=move |_| state.zoom.set(Zoom::Scale(1.0))>
                        "1:1"
                    </button>
                </div>
            </Show>
        </div>
    }
}

#[component]
fn PageView(stage_size: RwSignal<(f64, f64)>) -> impl IntoView {
    let state = use_state();
    let page = move || state.current_page();
    let first_scan = move || matches!(state.scan.get(), Scan::Running { preview: false, done: 0, .. }) && !state.has_pages();
    view! {
        {move || {
            if first_scan() {
                return view! { <ScanningPlaceholder/> }.into_any();
            }
            let Some(p) = page() else {
                return view! { <EmptyDocument/> }.into_any();
            };
            let (w, h) = natural_size(p.width, p.height, p.dpi);
            let src = api::image_url("view", p.id, p.rev);
            let style = move || {
                let (sw, sh) = stage_size.get();
                let scale = match state.zoom.get() {
                    Zoom::Fit => ((sw - STAGE_PADDING) / w).min((sh - STAGE_PADDING) / h),
                    Zoom::Width => (sw - STAGE_PADDING) / w,
                    Zoom::Scale(s) => s,
                };
                let scale = scale.max(0.02);
                format!("width:{:.0}px;height:{:.0}px", w * scale, h * scale)
            };
            let (img, loaded) = crate::icons::image_loaded();
            view! {
                <div class="page-canvas">
                    <div class="page-sheet" class:loaded=move || loaded.get() class:bilevel=p.bilevel style=style>
                        <img node_ref=img src=src alt="" draggable="false" on:load=move |_| loaded.set(true)/>
                        <Show when=move || state.busy.get().is_some()>
                            <div class="sheet-busy"><Spinner size=28/></div>
                        </Show>
                    </div>
                </div>
            }.into_any()
        }}
    }
}

#[component]
fn EmptyDocument() -> impl IntoView {
    let state = use_state();
    view! {
        <div class="empty-state">
            <div class="empty-illustration" aria-hidden="true">
                <div class="ill-sheet"><span></span><span></span><span></span></div>
                <div class="ill-beam"></div>
                <div class="ill-base"></div>
            </div>
            <h2>{tr!("Prêt à numériser", "Ready to scan")}</h2>
            <p class="muted">
                {move || if state.is_open() {
                    tr!(
                        "Placez votre document sur la vitre (ou dans le chargeur), puis lancez la numérisation.",
                        "Put your document on the glass (or in the feeder), then start scanning."
                    )
                } else {
                    tr!("Commencez par choisir un scanner en haut de la fenêtre.", "Start by choosing a scanner at the top of the window.")
                }}
            </p>
            <div class="row gap center">
                <button class="btn primary large" disabled=move || !state.is_open() on:click=move |_| state.start_scan(false)>
                    <Icon name="scan"/>{tr!("Numériser", "Scan")}
                </button>
                <button class="btn large" disabled=move || !state.is_open() on:click=move |_| state.start_scan(true)>
                    <Icon name="eye"/>{tr!("Aperçu", "Preview")}
                </button>
            </div>
            <p class="shortcut-hint">
                <kbd>"Ctrl"</kbd>"+"<kbd>{tr!("Entrée", "Enter")}</kbd>
                <span>{tr!("pour numériser", "to scan")}</span>
                <span class="dot">"·"</span>
                <kbd>"F1"</kbd>
                <span>{tr!("tous les raccourcis", "all shortcuts")}</span>
            </p>
        </div>
    }
}

#[component]
fn ScanningPlaceholder() -> impl IntoView {
    let state = use_state();
    let pct = move || match state.scan.get() {
        Scan::Running { fraction, .. } if fraction >= 0.0 => Some((fraction * 100.0).round()),
        _ => None,
    };
    view! {
        <div class="empty-state">
            <div class="scanning-sheet" aria-hidden="true">
                <div class="scanning-fill" style=move || format!("height:{}%", pct().unwrap_or(0.0))></div>
                <div class="scanning-beam" style=move || format!("top:{}%", pct().unwrap_or(0.0))></div>
            </div>
            <h2>{tr!("Numérisation en cours…", "Scanning…")}</h2>
            <p class="muted">{move || pct().map(|p| format!("{p} %")).unwrap_or_else(|| tr!("Préparation du scanner", "Warming up the scanner").to_string())}</p>
        </div>
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Drag {
    Move,
    Create,
    /// Bords modifiés : gauche, haut, droite, bas.
    Resize(bool, bool, bool, bool),
}

#[component]
fn AreaEditor(stage_size: RwSignal<(f64, f64)>) -> impl IntoView {
    let state = use_state();
    let glass: NodeRef<html::Div> = NodeRef::new();
    let draft = RwSignal::new(None::<Area>);
    let drag = StoredValue::new(None::<(Drag, f64, f64, Area)>);

    let geometry = move || state.geometry();
    let area = move || draft.get().or_else(|| geometry().map(|g| g.2));
    let glass_style = move || {
        let Some((gw, gh, _)) = geometry() else { return String::new() };
        let (sw, sh) = stage_size.get();
        let scale = ((sw - STAGE_PADDING) / gw).min((sh - STAGE_PADDING - 20.0) / gh).max(0.1);
        format!("width:{:.0}px;height:{:.0}px", gw * scale, gh * scale)
    };
    let to_mm = move |ev: &PointerEvent| -> Option<(f64, f64)> {
        let el = glass.get_untracked()?;
        let (gw, gh, _) = state.geometry()?;
        let r = el.get_bounding_client_rect();
        let x = ((ev.client_x() as f64 - r.left()) / r.width() * gw).clamp(0.0, gw);
        let y = ((ev.client_y() as f64 - r.top()) / r.height() * gh).clamp(0.0, gh);
        Some((x, y))
    };
    let begin = move |ev: PointerEvent, kind: Option<Drag>| {
        if ev.button() != 0 {
            return;
        }
        ev.prevent_default();
        ev.stop_propagation();
        let Some((x, y)) = to_mm(&ev) else { return };
        let Some(a) = area() else { return };
        let kind = kind.unwrap_or(Drag::Create);
        drag.set_value(Some((kind, x, y, a)));
        if kind == Drag::Create {
            draft.set(Some(Area { x, y, w: 0.0, h: 0.0 }));
        }
        if let Some(el) = glass.get_untracked() {
            let _ = el.set_pointer_capture(ev.pointer_id());
        }
    };
    let on_move = move |ev: PointerEvent| {
        let Some((kind, x0, y0, a0)) = drag.get_value() else { return };
        let Some((x, y)) = to_mm(&ev) else { return };
        let Some((gw, gh, _)) = state.geometry() else { return };
        let (dx, dy) = (x - x0, y - y0);
        let next = match kind {
            Drag::Create => Area {
                x: x0.min(x),
                y: y0.min(y),
                w: (x - x0).abs(),
                h: (y - y0).abs(),
            },
            Drag::Move => Area {
                x: (a0.x + dx).clamp(0.0, gw - a0.w),
                y: (a0.y + dy).clamp(0.0, gh - a0.h),
                ..a0
            },
            Drag::Resize(l, t, r, b) => {
                let (mut x1, mut y1, mut x2, mut y2) = (a0.x, a0.y, a0.x + a0.w, a0.y + a0.h);
                if l { x1 = (x1 + dx).clamp(0.0, x2 - 5.0) }
                if t { y1 = (y1 + dy).clamp(0.0, y2 - 5.0) }
                if r { x2 = (x2 + dx).clamp(x1 + 5.0, gw) }
                if b { y2 = (y2 + dy).clamp(y1 + 5.0, gh) }
                Area { x: x1, y: y1, w: x2 - x1, h: y2 - y1 }
            }
        };
        draft.set(Some(next));
    };
    let end = move |_ev: PointerEvent| {
        if drag.get_value().is_none() {
            return;
        }
        drag.set_value(None);
        if let Some(a) = draft.get_untracked() {
            // un simple clic sans tracé ne réduit pas la zone à rien
            if a.w >= 3.0 && a.h >= 3.0 {
                state.set_area(a);
            }
        }
        draft.set(None);
    };

    let pct = move |v: f64, total: f64| if total > 0.0 { v / total * 100.0 } else { 0.0 };
    let sel_style = move || {
        let (Some(a), Some((gw, gh, _))) = (area(), geometry()) else { return String::new() };
        format!(
            "left:{:.3}%;top:{:.3}%;width:{:.3}%;height:{:.3}%",
            pct(a.x, gw),
            pct(a.y, gh),
            pct(a.w, gw),
            pct(a.h, gh)
        )
    };
    let sel_label = move || {
        let a = area()?;
        let name = size_name(a.w, a.h).map(|n| format!(" · {n}")).unwrap_or_default();
        Some(format!("{} × {} mm{name}", fmt_mm(a.w), fmt_mm(a.h)))
    };
    let preview_url = move || state.preview.get().map(|p| api::image_url("preview", 0, p.rev));
    let previewing = move || matches!(state.scan.get(), Scan::Running { preview: true, .. });

    let handle = move |cls: &'static str, kind: Drag| {
        view! { <div class=format!("handle {cls}") on:pointerdown=move |ev| begin(ev, Some(kind))></div> }
    };

    view! {
        <Show
            when=move || geometry().is_some()
            fallback=|| view! {
                <div class="empty-state">
                    <h2>{tr!("Zone non réglable", "Area not adjustable")}</h2>
                    <p class="muted">{tr!("Ce scanner ne permet pas de choisir la zone de numérisation.", "This scanner does not allow choosing the scan area.")}</p>
                </div>
            }
        >
            <div class="area-wrap">
                <div
                    class="glass"
                    class:has-preview=move || preview_url().is_some()
                    node_ref=glass
                    style=glass_style
                    on:pointerdown=move |ev| begin(ev, None)
                    on:pointermove=on_move
                    on:pointerup=end
                    on:pointercancel=end
                >
                    {move || preview_url().map(|src| view! { <img class="glass-preview" src=src alt="" draggable="false"/> })}
                    <div class="selection" style=sel_style on:pointerdown=move |ev| {
                        // zone (presque) pleine vitre : un glisser trace une nouvelle zone
                        let full = geometry().zip(area()).is_some_and(|((gw, gh, _), a)| a.w >= gw * 0.95 && a.h >= gh * 0.95);
                        begin(ev, Some(if full { Drag::Create } else { Drag::Move }))
                    }>
                        <span class="selection-label">{sel_label}</span>
                        {handle("nw", Drag::Resize(true, true, false, false))}
                        {handle("n", Drag::Resize(false, true, false, false))}
                        {handle("ne", Drag::Resize(false, true, true, false))}
                        {handle("e", Drag::Resize(false, false, true, false))}
                        {handle("se", Drag::Resize(false, false, true, true))}
                        {handle("s", Drag::Resize(false, false, false, true))}
                        {handle("sw", Drag::Resize(true, false, false, true))}
                        {handle("w", Drag::Resize(true, false, false, false))}
                    </div>
                    <Show when=previewing>
                        <div class="glass-scanning"><div class="scanning-beam moving"></div></div>
                    </Show>
                    <Show when=move || preview_url().is_none() && !previewing()>
                        <div class="glass-hint" on:pointerdown=|ev| ev.stop_propagation()>
                            <Icon name="eye" size=28/>
                            <p>{tr!(
                                "Lancez un aperçu pour voir le document et tracer précisément la zone à numériser.",
                                "Run a preview to see the document and draw the exact area to scan."
                            )}</p>
                            <button class="btn primary" disabled=move || !state.is_open() on:click=move |_| state.start_scan(true)>
                                <Icon name="eye"/>{tr!("Lancer l'aperçu", "Run preview")}
                            </button>
                        </div>
                    </Show>
                </div>
                <p class="area-caption muted small">
                    <Icon name="info" size=14/>
                    {move || geometry().map(|(gw, gh, _)| trf!(
                        "Vitre de {} × {} mm — tracez un cadre ou déplacez ses poignées.",
                        "Glass of {} × {} mm — draw a frame or drag its handles.",
                        fmt_mm(gw), fmt_mm(gh)
                    ))}
                </p>
            </div>
        </Show>
    }
}

#[component]
fn StatusBar() -> impl IntoView {
    let state = use_state();
    let info = move || {
        let p = state.current_page()?;
        let (w, h) = p.size_mm();
        let name = size_name(w, h).map(|n| format!("{n} · ")).unwrap_or_default();
        Some(format!(
            "{name}{} × {} mm · {} × {} px · {} dpi",
            fmt_mm(w),
            fmt_mm(h),
            p.width,
            p.height,
            p.dpi.round()
        ))
    };
    let position = move || {
        let i = state.current_index()?;
        let n = state.pages.with(|p| p.len());
        Some(trf!("Page {} sur {}", "Page {} of {}", i + 1, n))
    };
    view! {
        <footer class="statusbar">
            <Show when=move || state.view.get() == View::Page && state.current.get().is_some()>
                <button class="btn icon-only ghost tiny" title=tr!("Page précédente", "Previous page") on:click=move |_| state.go(-1)
                    disabled=move || state.current_index() == Some(0)>
                    <Icon name="chevron-left" size=16/>
                </button>
                <span class="status-strong">{position}</span>
                <button class="btn icon-only ghost tiny" title=tr!("Page suivante", "Next page") on:click=move |_| state.go(1)
                    disabled=move || state.current_index().map(|i| i + 1) == Some(state.pages.with(|p| p.len()))>
                    <Icon name="chevron-right" size=16/>
                </button>
                <span class="status-sep"></span>
                <span class="muted">{info}</span>
            </Show>
            <span class="grow"></span>
            <Show when=move || state.dirty.get() && state.has_pages()>
                <span class="unsaved"><span class="dot-warn"></span>{tr!("Non enregistré", "Not saved")}</span>
            </Show>
        </footer>
    }
}
