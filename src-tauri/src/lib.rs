mod db;
mod gemini;
mod prompts;
mod scout;
mod settings;
mod tutor;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
#[cfg(desktop)]
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    RunEvent, WindowEvent,
};
#[cfg(desktop)]
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use db::Db;
use settings::Settings;

/// アプリ内タイマーの確認間隔
const TICK: Duration = Duration::from_secs(5 * 60);
/// 失敗後の再試行間隔（通常 / レート制限時）
const RETRY_AFTER: Duration = Duration::from_secs(30 * 60);
const RETRY_AFTER_RATE_LIMIT: Duration = Duration::from_secs(60 * 60);
#[cfg(desktop)]
const HIDDEN_ARG: &str = "--hidden";

#[derive(Default)]
struct ScoutState {
    running: bool,
    last_error: Option<String>,
    rate_limited: bool,
    last_failure: Option<Instant>,
}

struct AppState {
    db: Db,
    scout: Mutex<ScoutState>,
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

// ---------- スケジューリング（アプリ内タイマー方式） ----------

/// 日付が変わり規定時刻を過ぎていれば、裏側でスカウト処理を実行する
async fn maybe_scout(app: &AppHandle, force: bool) {
    let state = app.state::<AppState>();
    let date = today();
    {
        let conn = state.db.lock().unwrap();
        let s = settings::load(&conn);
        let last_run = db::get_setting(&conn, "last_run_date");
        if last_run.as_deref() == Some(date.as_str()) {
            return;
        }
        if let Ok(Some(_)) = db::get_pick_by_date(&conn, &date) {
            let _ = db::set_setting(&conn, "last_run_date", &date);
            return;
        }
        if !force {
            if chrono::Timelike::hour(&chrono::Local::now()) < s.scout_hour || !s.has_api_key {
                return;
            }
            let sc = state.scout.lock().unwrap();
            let wait = if sc.rate_limited { RETRY_AFTER_RATE_LIMIT } else { RETRY_AFTER };
            if sc.last_failure.is_some_and(|t| t.elapsed() < wait) {
                return;
            }
        }
    }
    {
        let mut sc = state.scout.lock().unwrap();
        if sc.running {
            return;
        }
        sc.running = true;
    }
    notify(app);

    let result = scout::run(&state.db, &date).await;
    {
        let mut sc = state.scout.lock().unwrap();
        sc.running = false;
        match &result {
            Ok(_) => {
                *sc = ScoutState::default();
                let conn = state.db.lock().unwrap();
                let _ = db::set_setting(&conn, "last_run_date", &date);
            }
            Err(e) => {
                eprintln!("[scout] {e}");
                sc.last_error = Some(e.to_string());
                sc.rate_limited = matches!(e, scout::ScoutError::Gemini(g) if g.is_rate_limited());
                sc.last_failure = Some(Instant::now());
            }
        }
    }
    notify(app);
}

fn spawn_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            maybe_scout(&app, false).await;
            tokio::time::sleep(TICK).await;
        }
    });
}

/// 画面とトレイの状態表示を更新
fn notify(app: &AppHandle) {
    #[cfg(desktop)]
    if let Ok(status) = build_status(app) {
        if let Some(tray) = app.tray_by_id("main") {
            let _ = tray.set_tooltip(Some(format!("ZennTutor — {}", status.label)));
        }
    }
    let _ = app.emit("status-changed", ());
}

// ---------- 状態 ----------

#[derive(Serialize)]
struct Status {
    /// no_key | waiting | preparing | ready | completed | error
    state: String,
    label: String,
    today: Option<db::Pick>,
    /// 未完了のまま溜まっている過去の記事
    backlog: Vec<db::Pick>,
    error: Option<String>,
    scout_hour: u32,
}

fn build_status(app: &AppHandle) -> Result<Status, String> {
    let state = app.state::<AppState>();
    let date = today();
    let conn = state.db.lock().unwrap();
    let s = settings::load(&conn);
    let today_pick = db::get_pick_by_date(&conn, &date).map_err(|e| e.to_string())?;
    let backlog: Vec<_> = db::list_picks(&conn)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|p| p.date != date && p.status != "completed")
        .collect();
    let sc = state.scout.lock().unwrap();

    let (st, mut label) = match (&today_pick, sc.running) {
        (Some(p), _) if p.status == "completed" => ("completed", "今日の学習は完了".to_string()),
        (Some(_), _) => ("ready", "今日の1本あり".to_string()),
        (None, true) => ("preparing", "準備中（スカウト実行中）".to_string()),
        (None, false) if !s.has_api_key => ("no_key", "API キー未設定".to_string()),
        (None, false) if sc.last_error.is_some() => ("error", "スカウト失敗".to_string()),
        (None, false) => ("waiting", format!("準備中（{}時以降に自動スカウト）", s.scout_hour)),
    };
    if !backlog.is_empty() {
        label.push_str(&format!(" / 未読 {} 本が溜まっています", backlog.len()));
    }
    Ok(Status {
        state: st.into(),
        label,
        today: today_pick,
        backlog,
        error: sc.last_error.clone(),
        scout_hour: s.scout_hour,
    })
}

// ---------- コマンド ----------

#[tauri::command]
fn get_status(app: AppHandle) -> Result<Status, String> {
    build_status(&app)
}

#[tauri::command]
async fn scout_now(app: AppHandle) -> Result<(), String> {
    maybe_scout(&app, true).await;
    let sc = app.state::<AppState>();
    let err = sc.scout.lock().unwrap().last_error.clone();
    match err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

#[tauri::command]
async fn preview_candidates(state: tauri::State<'_, AppState>) -> Result<Vec<scout::Candidate>, String> {
    scout::preview(&state.db).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn get_pick(state: tauri::State<'_, AppState>, id: i64) -> Result<Option<db::Pick>, String> {
    db::get_pick(&state.db.lock().unwrap(), id).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_picks(state: tauri::State<'_, AppState>) -> Result<Vec<db::Pick>, String> {
    db::list_picks(&state.db.lock().unwrap()).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_messages(state: tauri::State<'_, AppState>, pick_id: i64) -> Result<Vec<db::Message>, String> {
    db::list_messages(&state.db.lock().unwrap(), pick_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_memos(state: tauri::State<'_, AppState>) -> Result<Vec<db::Memo>, String> {
    db::list_memos(&state.db.lock().unwrap()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn start_chat(app: AppHandle, pick_id: i64) -> Result<Vec<db::Message>, String> {
    let r = tutor::start(&app.state::<AppState>().db, pick_id).await;
    notify(&app);
    r
}

#[tauri::command]
async fn send_message(app: AppHandle, pick_id: i64, text: String) -> Result<Vec<db::Message>, String> {
    let r = tutor::send(&app.state::<AppState>().db, pick_id, &text).await;
    notify(&app);
    r
}

#[tauri::command]
async fn retry_reply(app: AppHandle, pick_id: i64) -> Result<Vec<db::Message>, String> {
    let r = tutor::reply(&app.state::<AppState>().db, pick_id).await;
    notify(&app);
    r
}

#[tauri::command]
async fn submit_summary(app: AppHandle, pick_id: i64, summary: String) -> Result<tutor::Verdict, String> {
    let r = tutor::submit_summary(&app.state::<AppState>().db, pick_id, &summary).await;
    notify(&app);
    r
}

#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> Settings {
    settings::load(&state.db.lock().unwrap())
}

#[tauri::command]
fn save_settings(app: AppHandle, mut new: Settings) -> Result<Settings, String> {
    new.keywords = new.keywords.into_iter().map(|k| k.trim().to_string()).filter(|k| !k.is_empty()).collect();
    new.topics = new.topics.into_iter().map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()).collect();
    new.scout_hour = new.scout_hour.min(23);
    new.candidates = new.candidates.clamp(1, 10);
    if new.model.trim().is_empty() {
        new.model = settings::DEFAULT_MODEL.into();
    }
    {
        let state = app.state::<AppState>();
        let conn = state.db.lock().unwrap();
        settings::save(&conn, &new).map_err(|e| e.to_string())?;
    }
    apply_autostart(&app, new.autostart);
    notify(&app);
    Ok(get_settings(app.state()))
}

#[tauri::command]
fn set_api_key(app: AppHandle, key: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let conn = state.db.lock().unwrap();
        let key = key.trim();
        if key.is_empty() {
            db::delete_setting(&conn, "gemini_api_key")
        } else {
            db::set_setting(&conn, "gemini_api_key", key)
        }
        .map_err(|e| e.to_string())?;
    }
    {
        let mut sc = state.scout.lock().unwrap();
        sc.last_error = None;
        sc.last_failure = None;
    }
    notify(&app);
    Ok(())
}

/// API キーとモデルの疎通確認
#[tauri::command]
async fn test_api_key(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let (key, model) = {
        let conn = state.db.lock().unwrap();
        (settings::api_key(&conn), settings::load(&conn).model)
    };
    let key = key.ok_or("API キーが未設定です")?;
    let g = gemini::Gemini::new(key, model.clone());
    let out = g
        .generate(
            "一言で返答してください。",
            &[gemini::Turn { role: "user".into(), text: "こんにちは".into() }],
            false,
            0.0,
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(format!("{model}: {}", out.trim()))
}

// ---------- 起動・常駐 ----------

#[cfg(mobile)]
fn apply_autostart(_app: &AppHandle, _enabled: bool) {}

#[cfg(desktop)]
fn apply_autostart(app: &AppHandle, enabled: bool) {
    // 開発ビルドの実行ファイルをログイン項目に登録しないよう、リリースビルドのみで反映
    if cfg!(debug_assertions) {
        return;
    }
    let al = app.autolaunch();
    let current = al.is_enabled().unwrap_or(false);
    let r = match (enabled, current) {
        (true, false) => al.enable(),
        (false, true) => al.disable(),
        _ => Ok(()),
    };
    if let Err(e) = r {
        eprintln!("[autostart] {e}");
    }
}

#[cfg(desktop)]
fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[cfg(desktop)]
fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "ZennTutor を開く", true, None::<&str>)?;
    let scout_item = MenuItem::with_id(app, "scout", "今すぐスカウト", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &scout_item, &quit])?;

    TrayIconBuilder::with_id("main")
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .tooltip("ZennTutor")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => show_main(app),
            "scout" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move { maybe_scout(&app, true).await });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![HIDDEN_ARG])))
        .on_window_event(|window, event| {
            // ウィンドウを閉じても終了せずトレイに常駐
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        });

    let app = builder
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let conn = db::open(&dir.join("zenntutor.sqlite3"))?;
            let autostart = settings::load(&conn).autostart;
            app.manage(AppState { db: Arc::new(Mutex::new(conn)), scout: Mutex::default() });

            let handle = app.handle().clone();
            apply_autostart(&handle, autostart);
            #[cfg(desktop)]
            {
                setup_tray(&handle)?;
                // OS ログイン時の自動起動ではウィンドウを出さずトレイに常駐
                if !std::env::args().any(|a| a == HIDDEN_ARG) {
                    show_main(&handle);
                }
            }
            // モバイルではアプリ起動中のみタイマーが動く（開いた時点で当日分を用意）
            spawn_scheduler(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            scout_now,
            preview_candidates,
            get_pick,
            list_picks,
            get_messages,
            list_memos,
            start_chat,
            send_message,
            retry_reply,
            submit_summary,
            get_settings,
            save_settings,
            set_api_key,
            test_api_key,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app, _event| {
        #[cfg(target_os = "macos")]
        if let RunEvent::Reopen { .. } = _event {
            show_main(_app);
        }
    });
}
