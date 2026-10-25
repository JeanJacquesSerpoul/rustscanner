//! Panneau des réglages de numérisation : essentiels en tête, options du
//! scanner (générées d'après SANE) repliées dessous, bouton « Numériser » en pied.

use leptos::prelude::*;
use rustscanner_shared::{Area, Constraint, OptKind, OptUnit, OptValue, ScanOption, names, page_sizes, tr, trf, value_label};

use crate::icons::{Icon, Spinner};
use crate::state::{DeviceState, Scan, View, use_state};

/// Écart toléré (mm) pour reconnaître un format de page.
const SIZE_TOLERANCE: f64 = 1.5;

fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < 0.005 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}

fn mode_icon(v: &str) -> &'static str {
    let v = v.to_lowercase();
    if v.contains("color") || v.contains("colour") {
        "palette"
    } else if v.contains("gray") || v.contains("grey") {
        "contrast"
    } else if v.contains("halftone") {
        "dpi"
    } else {
        "text"
    }
}

fn source_icon(v: &str) -> &'static str {
    let v = v.to_lowercase();
    if v.contains("duplex") {
        "duplex"
    } else if rustscanner_shared::is_feeder(&v) {
        "feeder"
    } else if v.contains("transparen") || v.contains("film") {
        "image"
    } else {
        "flatbed"
    }
}

fn resolution_hint(dpi: f64) -> &'static str {
    match dpi as u32 {
        0..=100 => tr!("Aperçu rapide, fichier très léger", "Quick preview, very small file"),
        101..=200 => tr!("Écran et e-mail", "Screen and e-mail"),
        201..=300 => tr!("Documents et reconnaissance de texte", "Documents and text recognition"),
        301..=600 => tr!("Photos et impression soignée", "Photos and fine printing"),
        _ => tr!("Archivage haute fidélité (lent)", "High-fidelity archiving (slow)"),
    }
}

fn human_bytes(b: f64) -> String {
    if b >= 1e9 {
        trf!("{:.1} Go", "{:.1} GB", b / 1e9)
    } else if b >= 1e6 {
        trf!("{:.0} Mo", "{:.0} MB", b / 1e6)
    } else {
        trf!("{:.0} ko", "{:.0} kB", b / 1e3)
    }
}

#[component]
pub fn Sidebar() -> impl IntoView {
    let state = use_state();
    let open = move || state.is_open();
    view! {
        <aside class="sidebar">
            <div class="sidebar-scroll">
                <Show when=open fallback=move || view! { <SidebarPlaceholder/> }>
                    <SourceSection/>
                    <ModeSection/>
                    <ResolutionSection/>
                    <AreaSection/>
                    <AdvancedSection/>
                </Show>
            </div>
            <ScanFooter/>
        </aside>
    }
}

#[component]
fn SidebarPlaceholder() -> impl IntoView {
    let state = use_state();
    view! {
        <div class="sidebar-placeholder">
            {move || match state.device.get() {
                DeviceState::Searching | DeviceState::Opening(_) => view! {
                    <div class="skeleton-stack">
                        <div class="skeleton h-title"></div>
                        <div class="skeleton h-seg"></div>
                        <div class="skeleton h-title"></div>
                        <div class="skeleton h-seg"></div>
                        <div class="skeleton h-title"></div>
                        <div class="skeleton h-chips"></div>
                    </div>
                }.into_any(),
                DeviceState::Failed(_, e) => view! {
                    <div class="notice error">
                        <Icon name="alert"/>
                        <div>
                            <strong>{tr!("Le scanner ne répond pas", "The scanner does not respond")}</strong>
                            <p class="small">{e}</p>
                        </div>
                    </div>
                }.into_any(),
                _ => view! {
                    <div class="notice">
                        <Icon name="info"/>
                        <p class="small">{tr!(
                            "Choisissez un scanner en haut de la fenêtre pour afficher ses réglages.",
                            "Pick a scanner at the top of the window to show its settings."
                        )}</p>
                    </div>
                }.into_any(),
            }}
        </div>
    }
}

#[component]
fn Section(#[prop(into)] title: String, #[prop(into)] icon: String, children: Children) -> impl IntoView {
    view! {
        <section class="panel-section">
            <h3 class="section-title"><Icon name=icon size=15/>{title}</h3>
            {children()}
        </section>
    }
}

/// Choix exclusif parmi les valeurs textuelles d'une option (boutons segmentés).
#[component]
fn ChoiceOption(name: &'static str, icon_for: fn(&str) -> &'static str) -> impl IntoView {
    let state = use_state();
    let values = move || match state.option(name).map(|o| o.constraint) {
        Some(Constraint::Strings { values }) => values,
        _ => Vec::new(),
    };
    let current = move || state.option(name).and_then(|o| o.value).and_then(|v| v.as_str().map(str::to_string));
    let settable = move || state.option(name).is_some_and(|o| o.settable);
    view! {
        {move || {
            let vals = values();
            if vals.len() <= 1 || vals.len() > 4 {
                // valeur unique : simple affichage ; trop de valeurs : liste déroulante
                if vals.len() <= 1 {
                    let v = current().unwrap_or_default();
                    return view! {
                        <div class="static-value"><Icon name=icon_for(&v)/>{value_label(name, &v)}</div>
                    }.into_any();
                }
                return view! {
                    <select class="select" disabled=move || !settable() on:change=move |ev| {
                        state.set_option(name.into(), Some(OptValue::Text(event_target_value(&ev))));
                    }>
                        {vals.into_iter().map(|v| {
                            let selected = current().as_deref() == Some(v.as_str());
                            view! { <option value=v.clone() selected=selected>{value_label(name, &v)}</option> }
                        }).collect_view()}
                    </select>
                }.into_any();
            }
            let cols = vals.len();
            view! {
                <div class="segmented big" style=format!("--cols:{cols}")>
                    {vals.into_iter().map(|v| {
                        let v2 = v.clone();
                        let v3 = v.clone();
                        view! {
                            <button
                                class:active=move || current().as_deref() == Some(v2.as_str())
                                disabled=move || !settable()
                                on:click=move |_| state.set_option(name.into(), Some(OptValue::Text(v3.clone())))
                                title=v.clone()
                            >
                                <Icon name=icon_for(&v) size=20/>
                                <span>{value_label(name, &v)}</span>
                            </button>
                        }
                    }).collect_view()}
                </div>
            }.into_any()
        }}
    }
}

#[component]
fn SourceSection() -> impl IntoView {
    let state = use_state();
    view! {
        <Show when=move || state.option(names::SOURCE).is_some()>
            <Section title=tr!("Source", "Source") icon="layers">
                <ChoiceOption name=names::SOURCE icon_for=source_icon/>
                <Show when=move || {
                    state.option(names::SOURCE)
                        .and_then(|o| o.value)
                        .and_then(|v| v.as_str().map(rustscanner_shared::is_feeder))
                        .unwrap_or(false)
                }>
                    <p class="hint">
                        <Icon name="info" size=14/>
                        {tr!(
                            "Toutes les feuilles du chargeur seront numérisées à la suite.",
                            "Every sheet in the feeder will be scanned in a row."
                        )}
                    </p>
                </Show>
            </Section>
        </Show>
    }
}

#[component]
fn ModeSection() -> impl IntoView {
    let state = use_state();
    view! {
        <Show when=move || state.option(names::MODE).is_some()>
            <Section title=tr!("Couleur", "Color") icon="palette">
                <ChoiceOption name=names::MODE icon_for=mode_icon/>
            </Section>
        </Show>
    }
}

#[component]
fn ResolutionSection() -> impl IntoView {
    let state = use_state();
    let opt = move || state.option(names::RESOLUTION).or_else(|| state.option("x-resolution"));
    let current = move || state.resolution();
    // valeurs proposées en pastilles
    let chips = move || -> Vec<f64> {
        let common = [75.0, 100.0, 150.0, 200.0, 300.0, 600.0, 1200.0];
        match opt().map(|o| o.constraint) {
            Some(Constraint::Numbers { values }) => {
                if values.len() <= 6 {
                    values
                } else {
                    let picked: Vec<f64> = values.iter().cloned().filter(|v| [75.0, 150.0, 300.0, 600.0, 1200.0].contains(v)).collect();
                    if picked.len() >= 3 { picked } else { values.into_iter().take(6).collect() }
                }
            }
            Some(Constraint::Range { min, max, .. }) => {
                common.iter().cloned().filter(|v| *v >= min && *v <= max && [75.0, 150.0, 300.0, 600.0, 1200.0].contains(v)).collect()
            }
            _ => Vec::new(),
        }
    };
    let all_values = move || match opt().map(|o| o.constraint) {
        Some(Constraint::Numbers { values }) => values,
        _ => Vec::new(),
    };
    let range = move || match opt().map(|o| o.constraint) {
        Some(Constraint::Range { min, max, step }) => Some((min, max, step)),
        _ => None,
    };
    let set = move |v: f64| {
        if let Some(o) = opt() {
            state.set_option(o.name, Some(OptValue::Number(v)));
        }
    };
    let dims = move || {
        let (_, _, area) = state.geometry()?;
        let dpi = current();
        let w = (area.w / 25.4 * dpi).round();
        let h = (area.h / 25.4 * dpi).round();
        let channels = match state.option(names::MODE).and_then(|o| o.value).and_then(|v| v.as_str().map(|s| s.to_lowercase())) {
            Some(m) if m.contains("color") => 3.0,
            Some(m) if m.contains("lineart") || m.contains("binary") => 0.125,
            _ => 1.0,
        };
        Some(trf!(
            "{} × {} px · {} non compressé",
            "{} × {} px · {} uncompressed",
            w,
            h,
            human_bytes(w * h * channels)
        ))
    };
    view! {
        <Show when=move || opt().is_some()>
            <Section title=tr!("Résolution (dpi)", "Resolution (dpi)") icon="dpi">
                <div class="chips">
                    {move || chips().into_iter().map(|v| view! {
                        <button
                            class="chip"
                            class:active=move || (current() - v).abs() < 0.5
                            on:click=move |_| set(v)
                        >{fmt_num(v)}</button>
                    }).collect_view()}
                    <Show when=move || { let c = chips(); !c.iter().any(|v| (current() - v).abs() < 0.5) }>
                        <span class="chip active custom">{move || fmt_num(current())}</span>
                    </Show>
                </div>
                {move || {
                    let values = all_values();
                    if values.len() > chips().len() {
                        Some(view! {
                            <select class="select compact" on:change=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<f64>() { set(v) }
                            }>
                                {values.into_iter().map(|v| view! {
                                    <option value=v.to_string() selected=(current() - v).abs() < 0.5>{format!("{} dpi", fmt_num(v))}</option>
                                }).collect_view()}
                            </select>
                        }.into_any())
                    } else if let Some((min, max, step)) = range() {
                        let max = max.min(2400.0);
                        let step = if step > 0.0 { step } else { 1.0 };
                        Some(view! {
                            <div class="range-row">
                                <input
                                    type="range"
                                    min=min
                                    max=max
                                    step=step
                                    prop:value=current
                                    on:change=move |ev| {
                                        if let Ok(v) = event_target_value(&ev).parse::<f64>() { set(v) }
                                    }
                                />
                            </div>
                        }.into_any())
                    } else {
                        None
                    }
                }}
                <p class="hint"><Icon name="sparkles" size=14/>{move || resolution_hint(current())}</p>
                <Show when=move || dims().is_some()>
                    <p class="meta">{dims}</p>
                </Show>
            </Section>
        </Show>
    }
}

#[component]
fn AreaSection() -> impl IntoView {
    let state = use_state();
    let geometry = move || state.geometry();
    // formats qui tiennent sur la vitre
    let sizes = move || {
        let Some((gw, gh, _)) = geometry() else { return Vec::new() };
        page_sizes()
            .into_iter()
            .filter(|(_, _, w, h)| *w <= gw + SIZE_TOLERANCE && *h <= gh + SIZE_TOLERANCE)
            .collect::<Vec<_>>()
    };
    // format reconnu et orientation (true : paysage)
    let matched = move || -> Option<(&'static str, bool)> {
        let (gw, gh, a) = geometry()?;
        if a.x < SIZE_TOLERANCE && a.y < SIZE_TOLERANCE && (a.w - gw).abs() < SIZE_TOLERANCE && (a.h - gh).abs() < SIZE_TOLERANCE {
            return Some(("full", false));
        }
        sizes().into_iter().find_map(|(key, _, w, h)| {
            if (a.w - w).abs() < SIZE_TOLERANCE && (a.h - h).abs() < SIZE_TOLERANCE {
                Some((key, false))
            } else if (a.w - h).abs() < SIZE_TOLERANCE && (a.h - w).abs() < SIZE_TOLERANCE {
                Some((key, true))
            } else {
                None
            }
        })
    };
    let apply = move |key: String, landscape: bool| {
        let Some((gw, gh, _)) = geometry() else { return };
        if key == "full" {
            state.set_area(Area { x: 0.0, y: 0.0, w: gw, h: gh });
            return;
        }
        if let Some((_, _, w, h)) = page_sizes().into_iter().find(|(k, ..)| *k == key) {
            let (w, h) = if landscape { (h, w) } else { (w, h) };
            state.set_area(Area { x: 0.0, y: 0.0, w: w.min(gw), h: h.min(gh) });
        }
    };
    let rotate_fits = move || {
        let Some((key, landscape)) = matched() else { return false };
        let Some((gw, gh, _)) = geometry() else { return false };
        page_sizes().into_iter().find(|(k, ..)| *k == key).is_some_and(|(_, _, w, h)| {
            let (w, h) = if landscape { (w, h) } else { (h, w) };
            w <= gw + SIZE_TOLERANCE && h <= gh + SIZE_TOLERANCE
        })
    };
    view! {
        <Show when=move || geometry().is_some()>
            <Section title=tr!("Format et zone", "Size and area") icon="page-size">
                <div class="row gap">
                    <select class="select grow" on:change=move |ev| {
                        let v = event_target_value(&ev);
                        if v != "custom" { apply(v, false) }
                    }>
                        <option value="full" selected=move || matched().map(|m| m.0) == Some("full")>
                            {tr!("Toute la vitre", "Whole glass")}
                        </option>
                        {move || sizes().into_iter().map(|(key, label, w, h)| view! {
                            <option value=key selected=move || matched().map(|m| m.0) == Some(key)>
                                {format!("{label} — {} × {} mm", fmt_num(w), fmt_num(h))}
                            </option>
                        }).collect_view()}
                        <option value="custom" selected=move || matched().is_none()>
                            {tr!("Personnalisée", "Custom")}
                        </option>
                    </select>
                    <button
                        class="btn icon-only"
                        title=tr!("Portrait / paysage", "Portrait / landscape")
                        disabled=move || !rotate_fits() || matched().map(|m| m.0) == Some("full")
                        on:click=move |_| {
                            if let Some((key, landscape)) = matched() { apply(key.into(), !landscape) }
                        }
                    >
                        <Icon name="swap"/>
                    </button>
                </div>
                <p class="meta">
                    {move || geometry().map(|(_, _, a)| trf!(
                        "{} × {} mm, à {} × {} mm du coin",
                        "{} × {} mm, {} × {} mm from the corner",
                        fmt_num(a.w), fmt_num(a.h), fmt_num(a.x), fmt_num(a.y)
                    ))}
                </p>
                <button
                    class="btn subtle full"
                    on:click=move |_| state.view.set(View::Area)
                    class:active=move || state.view.get() == View::Area
                >
                    <Icon name="area"/>
                    {tr!("Ajuster la zone sur l'aperçu", "Adjust the area on the preview")}
                </button>
            </Section>
        </Show>
    }
}

#[component]
fn AdvancedSection() -> impl IntoView {
    let state = use_state();
    let expanded = RwSignal::new(false);
    let show_advanced = move || state.settings.with(|s| s.show_advanced);
    let visible = move || -> Vec<ScanOption> {
        let adv = show_advanced();
        state.options.with(|opts| {
            opts.iter()
                .filter(|o| o.active && !o.array && !names::is_essential(&o.name) && !o.title.is_empty())
                .filter(|o| adv || !o.advanced)
                .cloned()
                .collect()
        })
    };
    let count = move || visible().len();
    view! {
        <section class="panel-section collapsible" class:expanded=move || expanded.get()>
            <button class="section-toggle" on:click=move |_| expanded.update(|e| *e = !*e)>
                <Icon name="sliders" size=15/>
                <span>{tr!("Réglages du scanner", "Scanner settings")}</span>
                <span class="badge">{count}</span>
                <span class="grow"></span>
                <span class="chev"><Icon name="chevron-down" size=16/></span>
            </button>
            <Show when=move || expanded.get()>
                <div class="advanced">
                    {move || {
                        let opts = visible();
                        if opts.is_empty() {
                            return view! { <p class="muted small">{tr!("Aucun autre réglage pour ce scanner.", "No other setting for this scanner.")}</p> }.into_any();
                        }
                        let mut groups: Vec<(String, Vec<ScanOption>)> = Vec::new();
                        for o in opts {
                            match groups.last_mut() {
                                Some((g, list)) if *g == o.group => list.push(o),
                                _ => groups.push((o.group.clone(), vec![o])),
                            }
                        }
                        groups.into_iter().map(|(g, list)| view! {
                            <div class="opt-group">
                                <Show when={let g = g.clone(); move || !g.is_empty()}>
                                    <div class="opt-group-title">{g.clone()}</div>
                                </Show>
                                {list.into_iter().map(|o| view! { <OptionControl opt=o/> }).collect_view()}
                            </div>
                        }).collect_view().into_any()
                    }}
                    <label class="switch-row small">
                        <input type="checkbox" prop:checked=show_advanced on:change=move |ev| {
                            let mut s = state.settings.get_untracked();
                            s.show_advanced = event_target_checked(&ev);
                            state.save_settings(s);
                        }/>
                        <span class="switch"></span>
                        <span>{tr!("Afficher les options avancées", "Show advanced options")}</span>
                    </label>
                    <button class="btn subtle full" on:click=move |_| state.reset_options()>
                        <Icon name="undo"/>
                        {tr!("Rétablir les réglages d'usine", "Restore factory settings")}
                    </button>
                </div>
            </Show>
        </section>
    }
}

/// Contrôle générique d'une option SANE.
#[component]
fn OptionControl(opt: ScanOption) -> impl IntoView {
    let state = use_state();
    let name = opt.name.clone();
    let disabled = !opt.settable;
    let unit = opt.unit;
    let applying = {
        let name = name.clone();
        move || state.applying.get().as_deref() == Some(name.as_str())
    };
    let title = opt.title.clone();
    let desc = opt.desc.clone();
    let set = {
        let name = name.clone();
        move |v: Option<OptValue>| state.set_option(name.clone(), v)
    };
    let auto_button = opt.automatic.then(|| {
        let set = set.clone();
        view! {
            <button class="btn tiny ghost" title=tr!("Valeur automatique", "Automatic value") on:click=move |_| set(None)>
                "Auto"
            </button>
        }
    });
    let control = match (opt.kind, &opt.constraint) {
        (OptKind::Bool, _) => {
            let checked = opt.value.as_ref().and_then(OptValue::as_bool).unwrap_or(false);
            let set = set.clone();
            return view! {
                <label class="switch-row opt" title=desc>
                    <input type="checkbox" checked=checked disabled=disabled on:change=move |ev| {
                        set(Some(OptValue::Bool(event_target_checked(&ev))));
                    }/>
                    <span class="switch"></span>
                    <span class="opt-title">{title}</span>
                    <Show when=applying.clone()><Spinner size=12/></Show>
                </label>
            }.into_any();
        }
        (OptKind::Button, _) => {
            let set = set.clone();
            return view! {
                <div class="opt" title=desc>
                    <button class="btn subtle full" disabled=disabled on:click=move |_| set(Some(OptValue::Bool(true)))>{title}</button>
                </div>
            }.into_any();
        }
        (OptKind::Str, Constraint::Strings { values }) => {
            let current = opt.value.as_ref().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
            let set = set.clone();
            let n = name.clone();
            view! {
                <select class="select compact" disabled=disabled on:change=move |ev| set(Some(OptValue::Text(event_target_value(&ev))))>
                    {values.iter().map(|v| view! {
                        <option value=v.clone() selected=*v == current>{value_label(&n, v)}</option>
                    }).collect_view()}
                </select>
            }.into_any()
        }
        (OptKind::Str, _) => {
            let current = opt.value.as_ref().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
            let set = set.clone();
            view! {
                <input class="input compact" type="text" value=current disabled=disabled on:change=move |ev| set(Some(OptValue::Text(event_target_value(&ev))))/>
            }.into_any()
        }
        (_, Constraint::Numbers { values }) => {
            let current = opt.value.as_ref().and_then(OptValue::as_f64).unwrap_or(0.0);
            let set = set.clone();
            view! {
                <select class="select compact" disabled=disabled on:change=move |ev| {
                    if let Ok(v) = event_target_value(&ev).parse::<f64>() { set(Some(OptValue::Number(v))) }
                }>
                    {values.iter().map(|v| view! {
                        <option value=v.to_string() selected=(current - v).abs() < 1e-6>{format!("{}{}", fmt_num(*v), unit.suffix())}</option>
                    }).collect_view()}
                </select>
            }.into_any()
        }
        (_, Constraint::Range { min, max, step }) if min < max => {
            let current = opt.value.as_ref().and_then(OptValue::as_f64).unwrap_or(*min);
            let live = RwSignal::new(current);
            let step = if *step > 0.0 { *step } else if opt.kind == OptKind::Int { 1.0 } else { (max - min) / 100.0 };
            let (min, max) = (*min, *max);
            let set_slider = set.clone();
            view! {
                <div class="range-row">
                    <input
                        type="range" min=min max=max step=step prop:value=move || live.get() disabled=disabled
                        on:input=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<f64>() { live.set(v) } }
                        on:change=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<f64>() { set_slider(Some(OptValue::Number(v))) } }
                    />
                    <span class="range-value">{move || format!("{}{}", fmt_num(live.get()), if unit == OptUnit::None { "" } else { unit.suffix() })}</span>
                </div>
            }.into_any()
        }
        _ => {
            let current = opt.value.as_ref().and_then(OptValue::as_f64).unwrap_or(0.0);
            let set = set.clone();
            view! {
                <input class="input compact" type="number" value=current disabled=disabled on:change=move |ev| {
                    if let Ok(v) = event_target_value(&ev).parse::<f64>() { set(Some(OptValue::Number(v))) }
                }/>
            }.into_any()
        }
    };
    view! {
        <div class="opt" title=desc>
            <div class="opt-head">
                <span class="opt-title">{title}</span>
                <Show when=applying><Spinner size=12/></Show>
                {auto_button}
            </div>
            {control}
        </div>
    }
    .into_any()
}

#[component]
fn ScanFooter() -> impl IntoView {
    let state = use_state();
    let open = move || state.is_open();
    let feeder = move || {
        state
            .option(names::SOURCE)
            .and_then(|o| o.value)
            .and_then(|v| v.as_str().map(rustscanner_shared::is_feeder))
            .unwrap_or(false)
    };
    view! {
        <div class="sidebar-foot">
            {move || match state.scan.get() {
                Scan::Running { page, fraction, preview, done, cancelling } => {
                    let pct = if fraction >= 0.0 { Some((fraction * 100.0).round() as u32) } else { None };
                    let text = if cancelling {
                        tr!("Annulation…", "Cancelling…").to_string()
                    } else if preview {
                        tr!("Aperçu en cours…", "Previewing…").to_string()
                    } else {
                        trf!("Page {page} en cours…", "Scanning page {page}…")
                    };
                    view! {
                        <div class="scan-progress" class:indeterminate=pct.is_none()>
                            <div class="scan-progress-bar" style=format!("width:{}%", pct.unwrap_or(30))></div>
                            <div class="scan-progress-text">
                                <Spinner size=14/>
                                <span>{text}</span>
                                <span class="grow"></span>
                                <span class="mono">{pct.map(|p| format!("{p} %")).unwrap_or_default()}</span>
                            </div>
                        </div>
                        <div class="row gap">
                            <Show when=move || { done != 0 }>
                                <span class="muted small">{trf!("{done} page(s) prête(s)", "{done} page(s) ready")}</span>
                            </Show>
                            <span class="grow"></span>
                            <button class="btn danger-ghost" disabled=cancelling on:click=move |_| state.cancel_scan()>
                                <Icon name="stop"/>
                                {tr!("Annuler", "Cancel")}
                                <kbd>{tr!("Échap", "Esc")}</kbd>
                            </button>
                        </div>
                    }.into_any()
                }
                Scan::Idle => view! {
                    <button class="btn scan-button" disabled=move || !open() on:click=move |_| state.start_scan(false)>
                        <Icon name="scan" size=22/>
                        <span class="scan-button-text">
                            <span>{move || if feeder() { tr!("Numériser le chargeur", "Scan the feeder") } else { tr!("Numériser", "Scan") }}</span>
                            <kbd>"Ctrl ⏎"</kbd>
                        </span>
                    </button>
                    <button
                        class="btn subtle full"
                        disabled=move || !open()
                        title=tr!("Numérisation rapide de toute la vitre pour choisir la zone (Ctrl+Maj+Entrée)", "Quick scan of the whole glass to choose the area (Ctrl+Shift+Enter)")
                        on:click=move |_| state.start_scan(true)
                    >
                        <Icon name="eye"/>
                        {tr!("Aperçu", "Preview")}
                    </button>
                }.into_any(),
            }}
        </div>
    }
}
