//! Pont vers le cœur Tauri : commandes (invoke), événements et adresses des images.

use rustscanner_shared::tr;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke, catch)]
    async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], js_name = listen)]
    async fn tauri_listen(event: &str, handler: &Closure<dyn FnMut(JsValue)>) -> JsValue;
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ApiError {
    #[serde(default)]
    pub kind: String,
    pub message: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

fn to_js<T: Serialize>(value: &T) -> JsValue {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .unwrap_or(JsValue::NULL)
}

fn to_error(err: JsValue) -> ApiError {
    if let Some(s) = err.as_string() {
        return ApiError {
            kind: "other".into(),
            message: s,
        };
    }
    serde_wasm_bindgen::from_value::<ApiError>(err.clone()).unwrap_or_else(|_| ApiError {
        kind: "other".into(),
        message: js_sys::JSON::stringify(&err)
            .map(String::from)
            .unwrap_or_else(|_| tr!("erreur inconnue", "unknown error").into()),
    })
}

/// Appelle une commande du cœur ; `args` est un objet JSON.
pub async fn call<R: DeserializeOwned>(cmd: &str, args: Value) -> Result<R, ApiError> {
    let res = tauri_invoke(cmd, to_js(&args)).await.map_err(to_error)?;
    serde_wasm_bindgen::from_value(res).map_err(|e| ApiError {
        kind: "decode".into(),
        message: e.to_string(),
    })
}

/// Variante sans résultat : les erreurs sont journalisées dans la console.
pub fn fire(cmd: &'static str, args: Value) {
    leptos::task::spawn_local(async move {
        if let Err(e) = call::<Value>(cmd, args).await {
            log(&format!("{cmd} : {e}"));
        }
    });
}

pub fn log(msg: &str) {
    web_sys::console::warn_1(&msg.into());
}

#[derive(Deserialize)]
struct EventPayload<T> {
    payload: T,
}

/// Écoute un événement du cœur pendant toute la vie de l'application.
pub fn listen<T: DeserializeOwned + 'static>(event: &'static str, mut handler: impl FnMut(T) + 'static) {
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |v: JsValue| {
        match serde_wasm_bindgen::from_value::<EventPayload<T>>(v) {
            Ok(e) => handler(e.payload),
            Err(e) => log(&format!("{event} : {e}")),
        }
    });
    leptos::task::spawn_local(async move {
        tauri_listen(event, &closure).await;
        closure.forget();
    });
}

/// Mode développement (page servie par trunk hors de Tauri, cœur simulé).
pub fn is_dev() -> bool {
    js_sys::Reflect::get(&js_sys::global(), &"__RS_DEV__".into())
        .map(|v| !v.is_undefined())
        .unwrap_or(false)
}

/// Adresse d'une image du document : `kind` = thumb | view | preview.
pub fn image_url(kind: &str, id: u64, rev: u32) -> String {
    if is_dev()
        && let Ok(dev) = js_sys::Reflect::get(&js_sys::global(), &"__RS_DEV__".into())
        && let Ok(f) = js_sys::Reflect::get(&dev, &"image".into())
        && let Some(f) = f.dyn_ref::<js_sys::Function>()
        && let Ok(url) = f.call3(&dev, &kind.into(), &(id as f64).into(), &rev.into())
        && let Some(url) = url.as_string()
    {
        return url;
    }
    let windows = web_sys::window()
        .and_then(|w| w.navigator().user_agent().ok())
        .is_some_and(|ua| ua.contains("Windows"));
    let base = if windows { "http://scan.localhost" } else { "scan://localhost" };
    if kind == "preview" {
        format!("{base}/preview?r={rev}")
    } else {
        format!("{base}/{kind}/{id}?r={rev}")
    }
}

/// Titre de la fenêtre (document et fenêtre native).
pub fn set_title(title: &str) {
    crate::document().set_title(title);
    let get = |o: &JsValue, k: &str| js_sys::Reflect::get(o, &k.into()).ok().filter(|v| !v.is_undefined());
    let current = get(&js_sys::global(), "__TAURI__")
        .and_then(|t| get(&t, "window"))
        .and_then(|w| get(&w, "getCurrentWindow"))
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .and_then(|f| f.call0(&JsValue::NULL).ok());
    if let Some(win) = current
        && let Some(f) = get(&win, "setTitle").and_then(|f| f.dyn_into::<js_sys::Function>().ok())
    {
        // promesse ignorée : un échec laisse simplement l'ancien titre
        let _ = f.call1(&win, &title.into());
    }
}
