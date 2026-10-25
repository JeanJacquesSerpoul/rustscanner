//! Barre supérieure : marque, choix du scanner et actions du document.

use leptos::prelude::*;
use leptos::task::spawn_local;
use rustscanner_shared::{DeviceInfo, tr};

use crate::icons::{Icon, Spinner};
use crate::state::{DeviceState, Dialog, use_state};

fn device_icon(d: &DeviceInfo) -> &'static str {
    match d.backend() {
        "escl" | "airscan" | "net" => "wifi",
        "v4l" => "camera",
        "test" => "cube",
        _ if d.name.contains("192.168.") || d.name.contains("://") => "wifi",
        _ => "usb",
    }
}

#[component]
pub fn Header() -> impl IntoView {
    let state = use_state();
    let has_pages = move || state.has_pages();
    let scanning = move || state.scan.with(|s| s.is_running());
    view! {
        <header class="topbar">
            <div class="brand">
                <img src="icon.svg" alt="" class="brand-logo"/>
                <span class="brand-name">"RustScanner"</span>
            </div>
            <DevicePicker/>
            <div class="spacer"></div>
            <div class="toolbar-group">
                <button
                    class="btn ghost"
                    title=tr!("Nouveau document (Ctrl+N)", "New document (Ctrl+N)")
                    disabled=move || !has_pages() || scanning()
                    on:click=move |_| state.new_document(false)
                >
                    <Icon name="file-plus"/>
                    <span class="label">{tr!("Nouveau", "New")}</span>
                </button>
                <Show when=move || state.info.with(|i| i.as_ref().is_some_and(|i| i.printing))>
                    <button
                        class="btn ghost"
                        title=tr!("Imprimer (Ctrl+P)", "Print (Ctrl+P)")
                        disabled=move || !has_pages()
                        on:click=move |_| state.dialog.set(Some(Dialog::Print))
                    >
                        <Icon name="print"/>
                        <span class="label">{tr!("Imprimer", "Print")}</span>
                    </button>
                </Show>
                <button
                    class="btn primary"
                    title=tr!("Enregistrer le document (Ctrl+S)", "Save the document (Ctrl+S)")
                    disabled=move || !has_pages()
                    on:click=move |_| state.dialog.set(Some(Dialog::Save))
                >
                    <Icon name="save"/>
                    <span>{tr!("Enregistrer", "Save")}</span>
                </button>
            </div>
            <div class="divider-v"></div>
            <LangSwitch/>
            <button
                class="btn icon-only ghost"
                title=tr!("Raccourcis clavier (F1)", "Keyboard shortcuts (F1)")
                on:click=move |_| state.dialog.set(Some(Dialog::Shortcuts))
            >
                <Icon name="keyboard"/>
            </button>
            <button
                class="btn icon-only ghost"
                title=tr!("Paramètres (Ctrl+,)", "Settings (Ctrl+,)")
                on:click=move |_| state.dialog.set(Some(Dialog::Settings))
            >
                <Icon name="settings"/>
            </button>
        </header>
    }
}

/// Bascule rapide français / anglais.
#[component]
fn LangSwitch() -> impl IntoView {
    let state = use_state();
    let set = move |code: &'static str| {
        let mut s = state.settings.get_untracked();
        if s.language != code {
            s.language = code.into();
            state.save_settings(s);
        }
    };
    let is = move |code: &'static str| state.lang.get().code() == code;
    view! {
        <div class="segmented tiny" role="group" aria-label=tr!("Langue", "Language")>
            <button class:active=move || is("fr") on:click=move |_| set("fr") title="Français">"FR"</button>
            <button class:active=move || is("en") on:click=move |_| set("en") title="English">"EN"</button>
        </div>
    }
}

#[component]
fn DevicePicker() -> impl IntoView {
    let state = use_state();
    let open = RwSignal::new(false);
    let scanning = move || state.scan.with(|s| s.is_running());

    let label = move || match state.device.get() {
        DeviceState::Searching => tr!("Recherche des scanners…", "Looking for scanners…").to_string(),
        DeviceState::None => {
            if state.devices.with(|d| d.is_empty()) {
                tr!("Aucun scanner détecté", "No scanner found").to_string()
            } else {
                tr!("Choisir un scanner", "Choose a scanner").to_string()
            }
        }
        _ => state.device_info().map(|d| d.label()).unwrap_or_default(),
    };
    let sub = move || match state.device.get() {
        DeviceState::Opening(_) => tr!("Connexion…", "Connecting…").to_string(),
        DeviceState::Open(_) => state.device_info().map(|d| d.transport().to_string()).unwrap_or_default(),
        DeviceState::Failed(_, e) => e,
        DeviceState::Searching => tr!("Scanners USB et réseau", "USB and network scanners").to_string(),
        DeviceState::None => tr!("Cliquez pour rechercher", "Click to search").to_string(),
    };
    let status = move || match state.device.get() {
        DeviceState::Open(_) => "ok",
        DeviceState::Failed(..) => "err",
        DeviceState::Opening(_) | DeviceState::Searching => "busy",
        DeviceState::None => "off",
    };
    let refresh = move || {
        spawn_local(async move { state.refresh_devices(false).await });
    };
    let toggle = move |_| {
        if scanning() {
            return;
        }
        if state.devices.with_untracked(|d| d.is_empty()) && !matches!(state.device.get_untracked(), DeviceState::Searching) {
            refresh();
        }
        open.update(|o| *o = !*o);
    };

    view! {
        <div class="device-picker" class:open=move || open.get()>
            <button class="device-button" on:click=toggle disabled=scanning aria-haspopup="listbox">
                <span class=move || format!("status-dot {}", status())>
                    <Show when=move || status() == "busy"><Spinner size=12/></Show>
                </span>
                <span class="device-text">
                    <span class="device-label">{label}</span>
                    <span class="device-sub">{sub}</span>
                </span>
                <Icon name="chevron-down" size=16/>
            </button>
            <Show when=move || open.get()>
                <div class="backdrop clear" on:click=move |_| open.set(false)></div>
                <div class="popover device-menu" role="listbox">
                    <div class="popover-head">
                        <span>{tr!("Scanners disponibles", "Available scanners")}</span>
                        <button
                            class="btn tiny ghost"
                            disabled=move || matches!(state.device.get(), DeviceState::Searching)
                            on:click=move |_| refresh()
                        >
                            <Icon name="refresh" size=14/>
                            {tr!("Actualiser", "Refresh")}
                        </button>
                    </div>
                    <Show when=move || matches!(state.device.get(), DeviceState::Searching) && state.devices.with(|d| d.is_empty())>
                        <div class="menu-empty"><Spinner/>{tr!("Recherche en cours…", "Searching…")}</div>
                    </Show>
                    <Show when=move || !matches!(state.device.get(), DeviceState::Searching) && state.devices.with(|d| d.is_empty())>
                        <div class="menu-empty column">
                            <strong>{tr!("Aucun scanner trouvé", "No scanner found")}</strong>
                            <span class="muted small">
                                {tr!(
                                    "Vérifiez qu'il est allumé et branché, ou connecté au même réseau.",
                                    "Check that it is powered on and plugged in, or on the same network."
                                )}
                            </span>
                        </div>
                    </Show>
                    <For each=move || state.devices.get() key=|d| d.name.clone() let:d>
                        {
                            let name = d.name.clone();
                            let name2 = d.name.clone();
                            let selected = Signal::derive(move || match state.device.get() {
                                DeviceState::Open(n) | DeviceState::Opening(n) => n == name2,
                                _ => false,
                            });
                            view! {
                                <button
                                    class="menu-item device-item"
                                    class:selected=selected
                                    role="option"
                                    on:click=move |_| {
                                        open.set(false);
                                        let n = name.clone();
                                        spawn_local(async move { state.open_device(n).await });
                                    }
                                >
                                    <span class="device-item-icon"><Icon name=device_icon(&d)/></span>
                                    <span class="device-text">
                                        <span class="device-label">{d.label()}</span>
                                        <span class="device-sub">{d.transport()}" · "<span class="mono">{d.name.clone()}</span></span>
                                    </span>
                                    <Show when=move || selected.get()><Icon name="check" size=16/></Show>
                                </button>
                            }
                        }
                    </For>
                </div>
            </Show>
        </div>
    }
}
