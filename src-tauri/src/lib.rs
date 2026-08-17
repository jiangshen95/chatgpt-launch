use chatgpt_launcher_core::{consistency, geo, launcher, model::*, platform, ProfileStore};
use tauri::{Manager, State};

struct AppState {
    store: ProfileStore,
}

#[tauri::command]
fn list_profiles(state: State<'_, AppState>) -> Result<Vec<Profile>, String> {
    state.store.list().map_err(|e| e.to_string())
}

#[tauri::command]
fn save_profile(state: State<'_, AppState>, profile: Profile) -> Result<Profile, String> {
    state.store.upsert(profile).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_profile(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.store.delete(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn duplicate_profile(state: State<'_, AppState>, id: String) -> Result<Profile, String> {
    state.store.duplicate(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn test_connection(state: State<'_, AppState>, id: String) -> Result<TestReport, String> {
    let profile = state
        .store
        .get(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "未找到该配置".to_string())?;

    // Only neutral, third-party endpoints are contacted — never OpenAI.
    let exit = geo::lookup_exit(&profile.proxy).map_err(|e| e.to_string())?;
    let consistency = consistency::check(&profile, &exit);
    Ok(TestReport { exit, consistency })
}

#[tauri::command]
fn apply_detected(
    state: State<'_, AppState>,
    id: String,
    timezone: String,
    language: String,
) -> Result<Profile, String> {
    let mut profile = state
        .store
        .get(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "未找到该配置".to_string())?;
    profile.timezone = Some(timezone);
    profile.language = Some(language);
    state.store.upsert(profile).map_err(|e| e.to_string())
}

#[tauri::command]
fn launch(
    state: State<'_, AppState>,
    id: String,
    diagnostic_mode: bool,
) -> Result<LaunchResult, String> {
    let profile = state
        .store
        .get(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "未找到该配置".to_string())?;
    launcher::launch(&profile, diagnostic_mode).map_err(|e| e.to_string())
}

#[tauri::command]
fn detect_app() -> AppDetection {
    platform::detect_app().unwrap_or_else(|e| AppDetection {
        path: None,
        source: "error".into(),
        message: e.to_string(),
    })
}

#[tauri::command]
fn observe_connections(pid: u32) -> Result<Vec<ConnectionInfo>, String> {
    platform::observe_connections(pid).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_config_dir().map_err(|e| {
                Box::<dyn std::error::Error>::from(format!("解析配置目录失败: {e}"))
            })?;
            app.manage(AppState {
                store: ProfileStore::new(&dir),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_profiles,
            save_profile,
            delete_profile,
            duplicate_profile,
            test_connection,
            apply_detected,
            launch,
            detect_app,
            observe_connections
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
