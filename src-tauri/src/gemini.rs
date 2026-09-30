//! Gemini API（無料枠）クライアント

use serde::{Deserialize, Serialize};
use serde_json::json;

const BASE: &str = "https://generativelanguage.googleapis.com/v1beta";

#[derive(Debug)]
pub enum GeminiError {
    /// 無料枠のレート制限（HTTP 429）
    RateLimited(String),
    Api(String),
    Network(String),
    Parse(String),
}

impl std::fmt::Display for GeminiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateLimited(_) => write!(
                f,
                "Gemini の無料枠レート制限に達しました。しばらく時間をおいてから再試行してください（上限が日次の場合は翌日に回復します）。"
            ),
            Self::Api(m) => write!(f, "Gemini API エラー: {m}"),
            Self::Network(m) => write!(f, "通信エラー: {m}"),
            Self::Parse(m) => write!(f, "Gemini 応答の解析に失敗しました: {m}"),
        }
    }
}

impl GeminiError {
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, Self::RateLimited(_))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    /// "user" | "model"
    pub role: String,
    pub text: String,
}

pub struct Gemini {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl Gemini {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("reqwest client"),
            api_key,
            model,
        }
    }

    /// テキスト生成。`json_mode` の場合は JSON を返すよう強制する。
    pub async fn generate(
        &self,
        system: &str,
        turns: &[Turn],
        json_mode: bool,
        temperature: f32,
    ) -> Result<String, GeminiError> {
        let contents: Vec<_> = turns
            .iter()
            .map(|t| json!({ "role": t.role, "parts": [{ "text": t.text }] }))
            .collect();
        let mut generation_config = json!({ "temperature": temperature });
        if json_mode {
            generation_config["responseMimeType"] = json!("application/json");
        }
        let body = json!({
            "systemInstruction": { "parts": [{ "text": system }] },
            "contents": contents,
            "generationConfig": generation_config,
        });

        let url = format!("{BASE}/models/{}:generateContent", self.model);
        let res = self
            .client
            .post(url)
            .header("x-goog-api-key", &self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| GeminiError::Network(e.to_string()))?;

        let status = res.status();
        let text = res.text().await.map_err(|e| GeminiError::Network(e.to_string()))?;
        if status.as_u16() == 429 {
            return Err(GeminiError::RateLimited(text));
        }
        if !status.is_success() {
            let msg = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v["error"]["message"].as_str().map(String::from))
                .unwrap_or(text);
            return Err(GeminiError::Api(format!("{status}: {msg}")));
        }

        let v: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| GeminiError::Parse(e.to_string()))?;
        let parts = v["candidates"][0]["content"]["parts"]
            .as_array()
            .ok_or_else(|| {
                let reason = v["candidates"][0]["finishReason"]
                    .as_str()
                    .or(v["promptFeedback"]["blockReason"].as_str())
                    .unwrap_or("unknown");
                GeminiError::Parse(format!("本文が空です（理由: {reason}）"))
            })?;
        let out: String = parts
            .iter()
            .filter(|p| !p["thought"].as_bool().unwrap_or(false))
            .filter_map(|p| p["text"].as_str())
            .collect();
        if out.trim().is_empty() {
            return Err(GeminiError::Parse("本文が空です".into()));
        }
        Ok(out)
    }

    pub async fn generate_json<T: for<'de> Deserialize<'de>>(
        &self,
        system: &str,
        turns: &[Turn],
        temperature: f32,
    ) -> Result<T, GeminiError> {
        let raw = self.generate(system, turns, true, temperature).await?;
        let trimmed = raw
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        serde_json::from_str(trimmed).map_err(|e| GeminiError::Parse(format!("{e}: {raw}")))
    }
}
