//! 自動アップデート（デスクトップのみ）
//!
//! GitHub Releases の latest.json を定期的に確認し、新しい版があれば署名を検証して入れ替える。
//! Windows ではインストーラが無音（quiet）で動き、終了後に同じ起動引数（--hidden 含む）で再起動する。

use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;

/// 起動直後はネットワークが繋がっていないことがあるため少し待つ
const FIRST_CHECK_AFTER: Duration = Duration::from_secs(60);
const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);

pub fn spawn(app: AppHandle) {
    // 開発ビルドをリリース版で上書きしないよう、リリースビルドのみで有効
    if cfg!(debug_assertions) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_AFTER).await;
        loop {
            // 対話の途中で再起動しないよう、ウィンドウを閉じてトレイ常駐している間だけ適用する
            if !window_visible(&app) {
                if let Err(e) = install_if_available(&app).await {
                    eprintln!("[updater] {e}");
                }
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

fn window_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// 新しい版があればダウンロード・インストールして再起動する。最新なら `Ok(false)`。
pub async fn install_if_available(app: &AppHandle) -> Result<bool, String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    let Some(update) = update else {
        return Ok(false);
    };
    eprintln!("[updater] {} -> {}", update.current_version, update.version);
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    // Windows はインストーラ起動時にこのプロセスが終了するため、ここに来るのは macOS / Linux のみ
    app.restart();
}
