//! Project VECTOR desktop shell, as a library.
//!
//! `main.rs` is a two-line wrapper over [`run`]. Everything the application
//! does at the command boundary lives here so that `tests/` can drive it: a
//! binary-only crate cannot be imported by an integration test, and untestable
//! code is where stubs survive.
//!
//! [`self_check`] is the second entry point. It exercises the real command layer
//! against a real database from inside the shipped executable, which is the one
//! thing `cargo test` cannot prove: the release binary is a different build with
//! the real Tauri runtime and the embedded frontend, and `AGENTS.md` §9 does not
//! accept a library test as evidence about an artifact.

pub mod commands;
pub mod self_check;

use tauri::Manager;

/// Build the application, open the database, and run the window.
///
/// The database lives in the platform's application data directory, not beside
/// the executable: an installed application's own directory is frequently
/// read-only, and learner data must not be wiped by an upgrade.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_sql::Builder::default().build())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("cannot resolve the application data directory: {e}"))?;
            std::fs::create_dir_all(&data_dir)
                .map_err(|e| format!("cannot create {}: {e}", data_dir.display()))?;

            let db_path = data_dir.join("vector.db");
            let state = commands::AppState::open(&db_path)
                .map_err(|e| format!("cannot initialise {}: {e}", db_path.display()))?;

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_paths,
            commands::health,
            commands::create_profile,
            commands::get_profile,
            commands::list_profiles,
            commands::record_attempt,
            commands::analytics,
            commands::analytics_all,
            commands::set_mastery,
            commands::mastery,
            commands::study_plan,
            commands::readiness,
            commands::evidence_list,
            commands::evidence_get,
            commands::evidence_put,
            commands::backup_create,
            commands::backup_restore,
            commands::backup_list,
            commands::latency_probe,
            commands::reset_local_data,
            commands::ui_ready,
            commands::recompute_mastery,
            commands::content_generate,
            commands::content_next,
            commands::content_stats,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
