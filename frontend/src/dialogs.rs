//! Boîtes de dialogue : enregistrement, impression, paramètres, raccourcis,
//! confirmation de fermeture.

use leptos::prelude::*;
use leptos::task::spawn_local;
use rustscanner_shared::{ExportFormat, ExportRequest, ExportResult, Printer, Settings, expand_pattern, tr, trf};
use serde_json::json;

use crate::api;
use crate::icons::{Icon, Spinner};
use crate::state::{Dialog, ToastAction, ToastKind, use_state};

#[component]
pub fn Dialogs() -> impl IntoView {
    let state = use_state();
    view! {
        {move || state.dialog.get().map(|d| match d {
            Dialog::Save => view! { <SaveDialog/> }.into_any(),
            Dialog::Print => view! { <PrintDialog/> }.into_any(),
            Dialog::Settings => view! { <SettingsDialog/> }.into_any(),
            Dialog::Shortcuts => view! { <ShortcutsDialog/> }.into_any(),
            Dialog::Close(n) => view! { <CloseDialog unsaved=n/> }.into_any(),
            Dialog::NewDocument => view! { <NewDocumentDialog/> }.into_any(),
        })}
    }
}

#[component]
fn Modal(
    #[prop(into)] title: String,
    #[prop(into)] icon: String,
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    let state = use_state();
    view! {
        <div class="modal-backdrop" on:mousedown=move |ev| {
            // clic sur le fond uniquement
            use wasm_bindgen::JsCast;
            let on_backdrop = ev.target().zip(ev.current_target()).is_some_and(|(t, c)| {
                t.dyn_ref::<web_sys::Node>().map(|n| n.is_same_node(c.dyn_ref::<web_sys::Node>())).unwrap_or(false)
            });
            if on_backdrop { state.dialog.set(None) }
        }>
            <div class=format!("modal {class}") role="dialog" aria-modal="true">
                <div class="modal-head">
                    <span class="modal-icon"><Icon name=icon size=20/></span>
                    <h2>{title}</h2>
                    <span class="grow"></span>
                    <button class="btn icon-only ghost" title=tr!("Fermer (Échap)", "Close (Esc)") on:click=move |_| state.dialog.set(None)>
                        <Icon name="x"/>
                    </button>
                </div>
                {children()}
            </div>
        </div>
    }
}

fn format_icon(f: ExportFormat) -> &'static str {
    match f {
        ExportFormat::Pdf => "file",
        _ => "image",
    }
}

fn human_size(b: u64) -> String {
    let b = b as f64;
    if b >= 1e6 {
        trf!("{:.1} Mo", "{:.1} MB", b / 1e6)
    } else {
        trf!("{:.0} ko", "{:.0} kB", b / 1e3)
    }
}

fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

fn with_extension(path: &str, ext: &str) -> String {
    let name_start = path.rfind(['/', '\\']).map(|i| i + 1).unwrap_or(0);
    match path[name_start..].rfind('.') {
        Some(dot) if dot > 0 => format!("{}.{ext}", &path[..name_start + dot]),
        _ => format!("{path}.{ext}"),
    }
}

#[component]
fn SaveDialog() -> impl IntoView {
    let state = use_state();
    let settings = state.settings.get_untracked();
    let format = RwSignal::new(settings.format);
    let quality = RwSignal::new(settings.quality.clamp(10, 100));
    let path = RwSignal::new(String::new());
    let only_selected = RwSignal::new(false);
    let saving = RwSignal::new(None::<(usize, usize)>);
    let total = state.pages.with_untracked(|p| p.len());
    let n_selected = state.selected.with_untracked(|s| s.len());

    // nom proposé (dossier et modèle des paramètres)
    spawn_local(async move {
        if let Ok(p) = api::call::<String>("suggest_path", json!({ "format": format.get_untracked() })).await {
            path.set(p);
        }
    });
    // avancement transmis par le cœur pendant l'enregistrement
    Effect::new(move |_| {
        if let Some(p) = state.export_progress.get()
            && saving.get_untracked().is_some()
        {
            saving.set(Some(p));
        }
    });

    let choose_format = move |f: ExportFormat| {
        format.set(f);
        path.update(|p| *p = with_extension(p, f.extension()));
    };
    let browse = move |_| {
        spawn_local(async move {
            let current = path.get_untracked();
            if let Ok(Some(p)) = api::call::<Option<String>>("pick_save_path", json!({ "path": current, "format": format.get_untracked() })).await {
                path.set(p);
            }
        });
    };
    let submit = move || {
        if saving.get_untracked().is_some() || path.with_untracked(|p| p.trim().is_empty()) {
            return;
        }
        let pages = if only_selected.get_untracked() { state.targets() } else { Vec::new() };
        let count = if pages.is_empty() { total } else { pages.len() };
        let request = ExportRequest {
            path: path.get_untracked(),
            format: format.get_untracked(),
            pages,
            quality: quality.get_untracked(),
        };
        saving.set(Some((0, count)));
        state.export_progress.set(None);
        spawn_local(async move {
            let res = api::call::<ExportResult>("export_document", json!({ "request": request })).await;
            saving.set(None);
            match res {
                Ok(r) => {
                    state.dialog.set(None);
                    if !only_selected.get_untracked() {
                        state.dirty.set(false);
                    }
                    state.settings.update(|s| {
                        s.format = request.format;
                        s.quality = request.quality;
                    });
                    let first = r.files.first().cloned().unwrap_or_default();
                    let text = if r.files.len() == 1 {
                        trf!("Enregistré : {} ({})", "Saved: {} ({})", file_name(&first), human_size(r.bytes))
                    } else {
                        trf!("{} fichiers enregistrés ({})", "{} files saved ({})", r.files.len(), human_size(r.bytes))
                    };
                    state.toast(
                        ToastKind::Success,
                        text,
                        vec![
                            (tr!("Ouvrir", "Open").into(), ToastAction::Open(first.clone())),
                            (tr!("Afficher dans le dossier", "Show in folder").into(), ToastAction::Reveal(first)),
                        ],
                    );
                }
                Err(e) => state.error(&e),
            }
        });
    };
    let lossy = move || matches!(format.get(), ExportFormat::Pdf | ExportFormat::Jpeg);
    let multi_images = move || {
        let n = if only_selected.get() { n_selected } else { total };
        format.get() != ExportFormat::Pdf && n > 1
    };
    let quality_label = move || match quality.get() {
        0..=60 => tr!("Compact", "Compact"),
        61..=85 => tr!("Équilibré", "Balanced"),
        _ => tr!("Haute fidélité", "High fidelity"),
    };

    view! {
        <Modal title=tr!("Enregistrer le document", "Save document") icon="save" class="save-dialog">
            <form class="modal-body" on:submit=move |ev| { ev.prevent_default(); submit(); }>
                <div class="field">
                    <label class="field-label">{tr!("Format", "Format")}</label>
                    <div class="format-grid">
                        {ExportFormat::ALL.iter().map(|&f| view! {
                            <button type="button" class="format-card" class:active=move || format.get() == f on:click=move |_| choose_format(f)>
                                <span class="format-icon"><Icon name=format_icon(f) size=22/></span>
                                <span class="format-name">{f.label()}</span>
                                <span class="format-hint">{f.hint()}</span>
                            </button>
                        }).collect_view()}
                    </div>
                </div>
                <Show when=lossy>
                    <div class="field">
                        <label class="field-label">
                            {tr!("Qualité", "Quality")}
                            <span class="grow"></span>
                            <span class="muted">{quality_label}" · "{move || quality.get()}</span>
                        </label>
                        <input type="range" min="30" max="100" step="5" prop:value=move || quality.get()
                            on:input=move |ev| { if let Ok(v) = event_target_value(&ev).parse() { quality.set(v) } }/>
                        <p class="hint small">{tr!(
                            "Les pages en noir et blanc sont enregistrées sans perte dans le PDF.",
                            "Black & white pages are stored losslessly in the PDF."
                        )}</p>
                    </div>
                </Show>
                <Show when=move || { n_selected > 0 && n_selected < total }>
                    <div class="field">
                        <label class="field-label">{tr!("Pages", "Pages")}</label>
                        <div class="segmented">
                            <button type="button" class:active=move || !only_selected.get() on:click=move |_| only_selected.set(false)>
                                {trf!("Toutes ({total})", "All ({total})")}
                            </button>
                            <button type="button" class:active=move || only_selected.get() on:click=move |_| only_selected.set(true)>
                                {trf!("Sélection ({n_selected})", "Selection ({n_selected})")}
                            </button>
                        </div>
                    </div>
                </Show>
                <div class="field">
                    <label class="field-label">{tr!("Emplacement", "Location")}</label>
                    <div class="row gap">
                        <input class="input grow mono" type="text" prop:value=move || path.get()
                            on:input=move |ev| path.set(event_target_value(&ev)) spellcheck="false"/>
                        <button type="button" class="btn" on:click=browse>
                            <Icon name="folder"/>{tr!("Parcourir…", "Browse…")}
                        </button>
                    </div>
                    <Show when=multi_images>
                        <p class="hint small"><Icon name="info" size=14/>{tr!(
                            "Une image par page, numérotées : nom-001, nom-002…",
                            "One image per page, numbered: name-001, name-002…"
                        )}</p>
                    </Show>
                </div>
                <div class="modal-foot">
                    {move || saving.get().map(|(done, total)| view! {
                        <span class="muted small row gap"><Spinner size=14/>{trf!("Page {} sur {}…", "Page {} of {}…", (done + 1).min(total), total)}</span>
                    })}
                    <span class="grow"></span>
                    <button type="button" class="btn ghost" on:click=move |_| state.dialog.set(None)>{tr!("Annuler", "Cancel")}</button>
                    <button type="submit" class="btn primary" disabled=move || saving.get().is_some()>
                        <Icon name="save"/>{tr!("Enregistrer", "Save")}
                    </button>
                </div>
            </form>
        </Modal>
    }
}

#[component]
fn PrintDialog() -> impl IntoView {
    let state = use_state();
    let printers = RwSignal::new(None::<Vec<Printer>>);
    let printer = RwSignal::new(String::new());
    let copies = RwSignal::new(1u32);
    let only_selected = RwSignal::new(false);
    let printing = RwSignal::new(false);
    let total = state.pages.with_untracked(|p| p.len());
    let n_selected = state.selected.with_untracked(|s| s.len());
    spawn_local(async move {
        let list = api::call::<Vec<Printer>>("list_printers", json!({})).await.unwrap_or_default();
        if let Some(p) = list.iter().find(|p| p.default).or(list.first()) {
            printer.set(p.name.clone());
        }
        printers.set(Some(list));
    });
    let submit = move || {
        printing.set(true);
        let pages = if only_selected.get_untracked() { state.targets() } else { Vec::new() };
        spawn_local(async move {
            let res = api::call::<String>(
                "print_document",
                json!({ "pages": pages, "printer": printer.get_untracked(), "copies": copies.get_untracked() }),
            )
            .await;
            printing.set(false);
            match res {
                Ok(_) => {
                    state.dialog.set(None);
                    state.toast(ToastKind::Success, trf!("Envoyé à {}", "Sent to {}", printer.get_untracked()), Vec::new());
                }
                Err(e) => state.error(&e),
            }
        });
    };
    view! {
        <Modal title=tr!("Imprimer", "Print") icon="print">
            <form class="modal-body" on:submit=move |ev| { ev.prevent_default(); submit(); }>
                {move || match printers.get() {
                    None => view! { <div class="row gap muted"><Spinner/>{tr!("Recherche des imprimantes…", "Looking for printers…")}</div> }.into_any(),
                    Some(list) if list.is_empty() => view! {
                        <div class="notice"><Icon name="info"/><p>{tr!("Aucune imprimante n'est configurée.", "No printer is configured.")}</p></div>
                    }.into_any(),
                    Some(list) => view! {
                        <div class="field">
                            <label class="field-label">{tr!("Imprimante", "Printer")}</label>
                            <select class="select" on:change=move |ev| printer.set(event_target_value(&ev))>
                                {list.into_iter().map(|p| {
                                    let label = if p.default { format!("{} ({})", p.name, tr!("par défaut", "default")) } else { p.name.clone() };
                                    view! { <option value=p.name.clone() selected=move || printer.get() == p.name>{label}</option> }
                                }).collect_view()}
                            </select>
                        </div>
                    }.into_any(),
                }}
                <div class="row gap">
                    <div class="field">
                        <label class="field-label">{tr!("Copies", "Copies")}</label>
                        <input class="input narrow" type="number" min="1" max="99" prop:value=move || copies.get()
                            on:input=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<u32>() { copies.set(v.clamp(1, 99)) } }/>
                    </div>
                    <Show when=move || { n_selected > 0 && n_selected < total }>
                        <div class="field grow">
                            <label class="field-label">{tr!("Pages", "Pages")}</label>
                            <div class="segmented">
                                <button type="button" class:active=move || !only_selected.get() on:click=move |_| only_selected.set(false)>{trf!("Toutes ({total})", "All ({total})")}</button>
                                <button type="button" class:active=move || only_selected.get() on:click=move |_| only_selected.set(true)>{trf!("Sélection ({n_selected})", "Selection ({n_selected})")}</button>
                            </div>
                        </div>
                    </Show>
                </div>
                <div class="modal-foot">
                    <span class="grow"></span>
                    <button type="button" class="btn ghost" on:click=move |_| state.dialog.set(None)>{tr!("Annuler", "Cancel")}</button>
                    <button type="submit" class="btn primary" disabled=move || printing.get() || printer.get().is_empty()>
                        <Show when=move || printing.get() fallback=|| view! { <Icon name="print"/> }><Spinner size=14/></Show>
                        {tr!("Imprimer", "Print")}
                    </button>
                </div>
            </form>
        </Modal>
    }
}

#[component]
fn SettingsDialog() -> impl IntoView {
    let state = use_state();
    let draft = RwSignal::new(state.settings.get_untracked());
    let info = state.info.get_untracked();
    // sous Windows (WIA), les scanners réseau s'ajoutent dans les paramètres du système
    let wia = info.as_ref().is_some_and(|i| i.backend == "WIA");
    let default_dir = info.as_ref().map(|i| i.save_dir.clone()).unwrap_or_default();
    let dir_shown = {
        let default_dir = default_dir.clone();
        move || {
            let d = draft.with(|s| s.save_dir.clone());
            if d.is_empty() { default_dir.clone() } else { d }
        }
    };
    let pick = {
        let dir_shown = dir_shown.clone();
        move |_| {
            let start = dir_shown();
            spawn_local(async move {
                if let Ok(Some(p)) = api::call::<Option<String>>("pick_folder", json!({ "start": start })).await {
                    draft.update(|s| s.save_dir = p);
                }
            });
        }
    };
    let example = move || {
        let now = js_sys::Date::new_0();
        let date = format!("{:04}-{:02}-{:02}", now.get_full_year(), now.get_month() + 1, now.get_date());
        let time = format!("{:02}-{:02}-{:02}", now.get_hours(), now.get_minutes(), now.get_seconds());
        format!("{}.{}", expand_pattern(&draft.with(|s| s.name_pattern.clone()), &date, &time, 1), draft.with(|s| s.format.extension()))
    };
    let save = move |_| {
        state.save_settings(draft.get_untracked());
        state.dialog.set(None);
    };
    let toggle = move |label: &'static str, hint: &'static str, get: fn(&Settings) -> bool, set: fn(&mut Settings, bool)| {
        view! {
            <label class="switch-row">
                <input type="checkbox" prop:checked=move || draft.with(get) on:change=move |ev| {
                    let v = event_target_checked(&ev);
                    draft.update(|s| set(s, v));
                }/>
                <span class="switch"></span>
                <span class="switch-text">
                    <span>{label}</span>
                    <span class="muted small">{hint}</span>
                </span>
            </label>
        }
    };
    let choice = move |current: fn(&Settings) -> String, set: fn(&mut Settings, String), items: Vec<(&'static str, &'static str, &'static str)>| {
        let cols = format!("--cols:{}", items.len());
        view! {
            <div class="segmented" style=cols>
                {items.into_iter().map(|(value, label, icon)| view! {
                    <button type="button" class:active=move || draft.with(current) == value on:click=move |_| draft.update(|s| set(s, value.into()))>
                        <Icon name=icon size=16/>{label}
                    </button>
                }).collect_view()}
            </div>
        }
    };
    view! {
        <Modal title=tr!("Paramètres", "Settings") icon="settings" class="settings-dialog">
            <div class="modal-body">
                <div class="settings-grid">
                    <h4>{tr!("Apparence", "Appearance")}</h4>
                    <div class="field">
                        <label class="field-label">{tr!("Langue", "Language")}</label>
                        {choice(|s| s.language.clone(), |s, v| s.language = v, vec![
                            ("auto", tr!("Système", "System"), "monitor"),
                            ("fr", "Français", "globe"),
                            ("en", "English", "globe"),
                        ])}
                    </div>
                    <div class="field">
                        <label class="field-label">{tr!("Thème", "Theme")}</label>
                        {choice(|s| s.theme.clone(), |s, v| s.theme = v, vec![
                            ("auto", tr!("Système", "System"), "monitor"),
                            ("light", tr!("Clair", "Light"), "sun"),
                            ("dark", tr!("Sombre", "Dark"), "moon"),
                        ])}
                    </div>

                    <h4>{tr!("Enregistrement", "Saving")}</h4>
                    <div class="field">
                        <label class="field-label">{tr!("Dossier par défaut", "Default folder")}</label>
                        <div class="row gap">
                            <input class="input grow mono" type="text" readonly prop:value=dir_shown/>
                            <button type="button" class="btn" on:click=pick><Icon name="folder"/>{tr!("Choisir…", "Choose…")}</button>
                        </div>
                    </div>
                    <div class="field">
                        <label class="field-label">{tr!("Nom des fichiers", "File names")}</label>
                        <input class="input mono" type="text" prop:value=move || draft.with(|s| s.name_pattern.clone())
                            on:input=move |ev| { let v = event_target_value(&ev); draft.update(|s| s.name_pattern = v) } spellcheck="false"/>
                        <p class="hint small">
                            <code>"{date}"</code>" "<code>"{time}"</code>" "<code>"{n}"</code>" → "
                            <span class="mono">{example}</span>
                        </p>
                    </div>

                    <h4>{tr!("Scanners", "Scanners")}</h4>
                    {(!wia).then(|| toggle(
                        tr!("Rechercher les scanners réseau", "Look for network scanners"),
                        tr!("Wi-Fi et Ethernet (eSCL, AirScan, saned) ; la recherche est un peu plus longue.", "Wi-Fi and Ethernet (eSCL, AirScan, saned); searching takes a bit longer."),
                        |s| s.network_devices, |s, v| s.network_devices = v,
                    ))}
                    {toggle(
                        tr!("Rouvrir le dernier scanner", "Reopen the last scanner"),
                        tr!("Connexion automatique au démarrage.", "Connect automatically at startup."),
                        |s| s.auto_open, |s, v| s.auto_open = v,
                    )}
                    {toggle(
                        tr!("Mémoriser les réglages de chaque scanner", "Remember each scanner's settings"),
                        tr!("Résolution, couleur, zone… retrouvées à la prochaine ouverture.", "Resolution, color, area… restored next time."),
                        |s| s.remember_options, |s, v| s.remember_options = v,
                    )}
                </div>
                <p class="about muted small">
                    {move || info.as_ref().map(|i| format!(
                        "RustScanner {} · {} {} · Rust, Tauri & Leptos",
                        i.version,
                        if i.backend.is_empty() { "SANE" } else { i.backend.as_str() },
                        if i.sane { i.sane_version.clone() } else { "—".into() }
                    ))}
                </p>
                <div class="modal-foot">
                    <span class="grow"></span>
                    <button type="button" class="btn ghost" on:click=move |_| state.dialog.set(None)>{tr!("Annuler", "Cancel")}</button>
                    <button type="button" class="btn primary" on:click=save><Icon name="check"/>{tr!("Appliquer", "Apply")}</button>
                </div>
            </div>
        </Modal>
    }
}

#[component]
fn ShortcutsDialog() -> impl IntoView {
    view! {
        <Modal title=tr!("Raccourcis clavier", "Keyboard shortcuts") icon="keyboard" class="shortcuts-dialog">
            <div class="modal-body shortcuts">
                {crate::keys::shortcuts().into_iter().map(|(group, items)| view! {
                    <div class="shortcut-group">
                        <h4>{group}</h4>
                        {items.into_iter().map(|(keys, what)| view! {
                            <div class="shortcut">
                                <span>{what}</span>
                                <span class="keys">{keys.split(" / ").map(|combo| view! {
                                    <span class="combo">{combo.split(" + ").map(|k| view! { <kbd>{k.to_string()}</kbd> }).collect_view()}</span>
                                }).collect_view()}</span>
                            </div>
                        }).collect_view()}
                    </div>
                }).collect_view()}
            </div>
        </Modal>
    }
}

#[component]
fn CloseDialog(unsaved: usize) -> impl IntoView {
    let state = use_state();
    let scanning = state.scan.get_untracked().is_running();
    view! {
        <Modal title=tr!("Quitter RustScanner ?", "Quit RustScanner?") icon="alert" class="confirm">
            <div class="modal-body">
                <p>
                    {if unsaved > 0 {
                        trf!(
                            "{unsaved} page(s) n'ont pas été enregistrées et seront perdues.",
                            "{unsaved} page(s) have not been saved and will be lost."
                        )
                    } else if scanning {
                        tr!("Une numérisation est en cours ; elle sera interrompue.", "A scan is running; it will be interrupted.").to_string()
                    } else {
                        String::new()
                    }}
                </p>
                <div class="modal-foot">
                    <button class="btn danger-ghost" on:click=move |_| api::fire("quit", json!({}))>{tr!("Quitter sans enregistrer", "Quit without saving")}</button>
                    <span class="grow"></span>
                    <button class="btn ghost" on:click=move |_| state.dialog.set(None)>{tr!("Annuler", "Cancel")}</button>
                    <Show when=move || { unsaved != 0 }>
                        <button class="btn primary" on:click=move |_| state.dialog.set(Some(Dialog::Save))>
                            <Icon name="save"/>{tr!("Enregistrer…", "Save…")}
                        </button>
                    </Show>
                </div>
            </div>
        </Modal>
    }
}

#[component]
fn NewDocumentDialog() -> impl IntoView {
    let state = use_state();
    let n = state.pages.with_untracked(|p| p.len());
    view! {
        <Modal title=tr!("Nouveau document", "New document") icon="file-plus" class="confirm">
            <div class="modal-body">
                <p>{trf!(
                    "Le document actuel ({n} page(s)) n'est pas enregistré. Le remplacer par un document vide ?",
                    "The current document ({n} page(s)) is not saved. Replace it with an empty one?"
                )}</p>
                <div class="modal-foot">
                    <button class="btn danger-ghost" on:click=move |_| { state.dialog.set(None); state.new_document(true); }>
                        <Icon name="trash"/>{tr!("Abandonner les pages", "Discard pages")}
                    </button>
                    <span class="grow"></span>
                    <button class="btn ghost" on:click=move |_| state.dialog.set(None)>{tr!("Annuler", "Cancel")}</button>
                    <button class="btn primary" on:click=move |_| state.dialog.set(Some(Dialog::Save))>
                        <Icon name="save"/>{tr!("Enregistrer d'abord…", "Save first…")}
                    </button>
                </div>
            </div>
        </Modal>
    }
}
