//! RustScanner — numérisation de documents (SANE), dans l'esprit de Skanpage.

pub mod commands;
pub mod config;
pub mod document;
pub mod error;
pub mod export;
pub mod frames;
pub mod gettext;
pub mod sane;
pub mod scanner;

use tauri::http::{Response, StatusCode, header};
use tauri::{Emitter, Manager, RunEvent, WindowEvent};

use crate::commands::AppState;

/// Images du document servies à l'interface : `scan://localhost/thumb/<id>`,
/// `/view/<id>`, `/preview` (le paramètre `?r=` ne sert qu'à contourner le cache).
fn serve(app: &tauri::AppHandle, path: &str) -> Option<Vec<u8>> {
    let state = app.state::<AppState>();
    let doc = state.doc.lock().unwrap();
    let mut parts = path.trim_matches('/').split('/');
    match (parts.next()?, parts.next()) {
        ("thumb", Some(id)) => doc.thumb(id.parse().ok()?),
        ("view", Some(id)) => doc.view(id.parse().ok()?),
        ("preview", _) => doc.preview(),
        _ => None,
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .register_asynchronous_uri_scheme_protocol("scan", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let path = request.uri().path().to_string();
            std::thread::spawn(move || {
                let resp = match serve(&app, &path) {
                    Some(bytes) => Response::builder()
                        .header(header::CONTENT_TYPE, "image/jpeg")
                        .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
                        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                        .body(bytes),
                    None => Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new()),
                };
                responder.respond(resp.unwrap());
            });
        })
        .setup(|app| {
            app.manage(AppState::new(app.handle().clone()));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                let unsaved = state.unsaved_pages();
                if unsaved > 0 || state.scanner.scanning() {
                    // l'interface demande confirmation puis appelle « quit »
                    api.prevent_close();
                    let _ = window.emit("close-requested", unsaved);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::save_settings,
            commands::list_devices,
            commands::open_device,
            commands::close_device,
            commands::get_options,
            commands::set_option,
            commands::set_area,
            commands::reset_options,
            commands::start_scan,
            commands::cancel_scan,
            commands::list_pages,
            commands::transform_pages,
            commands::delete_pages,
            commands::restore_pages,
            commands::reorder_pages,
            commands::clear_document,
            commands::suggest_path,
            commands::export_document,
            commands::pick_save_path,
            commands::pick_folder,
            commands::list_printers,
            commands::print_document,
            commands::open_path,
            commands::reveal_path,
            commands::quit,
        ])
        .build(tauri::generate_context!())
        .expect("démarrage de RustScanner")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                // fichiers temporaires de la session
                let state = app.state::<AppState>();
                let dir = state.doc.lock().unwrap().dir().to_path_buf();
                let _ = std::fs::remove_dir_all(dir);
            }
        });
}
