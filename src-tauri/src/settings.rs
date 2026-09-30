//! 後から自由に調整可能な設定項目（キーワードシード・取得トピック・実行時刻など）

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db;

pub const DEFAULT_MODEL: &str = "gemini-2.5-flash";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// 画面へは「設定済みかどうか」だけを返し、キー本体は返さない
    #[serde(default)]
    pub has_api_key: bool,
    pub model: String,
    /// この時刻（0-23時）を過ぎたら当日のスカウトを実行する
    pub scout_hour: u32,
    /// 1次選抜のキーワードシード
    pub keywords: Vec<String>,
    /// 取得する Zenn トピック（zenn.dev/topics/{topic}/feed）
    pub topics: Vec<String>,
    /// 1次選抜で Gemini に渡す候補数
    pub candidates: usize,
    /// この日数より古い記事は対象外
    pub max_age_days: i64,
    pub autostart: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            has_api_key: false,
            model: DEFAULT_MODEL.into(),
            scout_hour: 6,
            keywords: [
                "Next.js", "React", "TypeScript", "Vercel", "Cloudflare", "Render", "AWS",
                "AIエージェント", "システム設計", "アーキテクチャ",
            ]
            .map(String::from)
            .to_vec(),
            topics: [
                "nextjs", "react", "typescript", "vercel", "cloudflare", "render", "aws",
                "aiagent", "systemdesign", "architecture",
            ]
            .map(String::from)
            .to_vec(),
            candidates: 5,
            max_age_days: 14,
            autostart: true,
        }
    }
}

pub fn load(conn: &Connection) -> Settings {
    let mut s = db::get_setting(conn, "app_settings")
        .and_then(|v| serde_json::from_str::<Settings>(&v).ok())
        .unwrap_or_default();
    s.has_api_key = api_key(conn).is_some();
    s
}

pub fn save(conn: &Connection, s: &Settings) -> rusqlite::Result<()> {
    db::set_setting(conn, "app_settings", &serde_json::to_string(s).unwrap())
}

/// 環境変数 GEMINI_API_KEY を優先し、なければ設定画面で保存したキーを使う
pub fn api_key(conn: &Connection) -> Option<String> {
    std::env::var("GEMINI_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .or_else(|| db::get_setting(conn, "gemini_api_key"))
        .filter(|k| !k.trim().is_empty())
}
