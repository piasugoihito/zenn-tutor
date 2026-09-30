//! ソクラテス式対話と理解度チェック

use serde::{Deserialize, Serialize};

use crate::db::{self, Db, Message, Pick};
use crate::gemini::{Gemini, Turn};
use crate::prompts;
use crate::settings;

fn context(db: &Db, pick_id: i64) -> Result<(Gemini, Pick, Vec<Message>, Vec<String>), String> {
    let conn = db.lock().unwrap();
    let s = settings::load(&conn);
    let key = settings::api_key(&conn)
        .ok_or("Gemini API キーが未設定です。設定画面から登録してください。")?;
    let pick = db::get_pick(&conn, pick_id)
        .map_err(|e| e.to_string())?
        .ok_or("記事が見つかりません")?;
    let msgs = db::list_messages(&conn, pick_id).map_err(|e| e.to_string())?;
    let understood = db::understood_concepts(&conn, 30);
    Ok((Gemini::new(key, s.model), pick, msgs, understood))
}

/// 対話履歴を Gemini の turns に変換（先頭は必ず user、連続する同一ロールは結合）
fn to_turns(msgs: &[Message]) -> Vec<Turn> {
    let mut turns: Vec<Turn> = vec![Turn { role: "user".into(), text: prompts::TUTOR_OPENING.into() }];
    for m in msgs {
        let text = match m.kind.as_str() {
            "summary" => format!("【一言まとめ（理解度チェックに提出）】{}", m.content),
            "verdict" => format!("【判定結果】{}", m.content),
            _ => m.content.clone(),
        };
        match turns.last_mut() {
            Some(last) if last.role == m.role => {
                last.text.push_str("\n\n");
                last.text.push_str(&text);
            }
            _ => turns.push(Turn { role: m.role.clone(), text }),
        }
    }
    turns
}

/// 対話を開始（AI からの最初の問いかけを生成）。既に開始済みなら何もしない。
pub async fn start(db: &Db, pick_id: i64) -> Result<Vec<Message>, String> {
    let (gemini, pick, msgs, understood) = context(db, pick_id)?;
    if !msgs.is_empty() {
        return Ok(msgs);
    }
    let reply = gemini
        .generate(&prompts::tutor_system(&pick, &understood), &to_turns(&[]), false, 0.7)
        .await
        .map_err(|e| e.to_string())?;
    let conn = db.lock().unwrap();
    db::add_message(&conn, pick_id, "model", "chat", &reply).map_err(|e| e.to_string())?;
    if pick.status == "ready" {
        db::set_pick_status(&conn, pick_id, "in_progress").map_err(|e| e.to_string())?;
    }
    db::list_messages(&conn, pick_id).map_err(|e| e.to_string())
}

pub async fn send(db: &Db, pick_id: i64, text: &str) -> Result<Vec<Message>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("メッセージが空です".into());
    }
    // ユーザー発言は API 失敗時も失われないよう先に保存する（失敗時は reply で再生成できる）
    {
        let conn = db.lock().unwrap();
        db::get_pick(&conn, pick_id).map_err(|e| e.to_string())?.ok_or("記事が見つかりません")?;
        db::add_message(&conn, pick_id, "user", "chat", text).map_err(|e| e.to_string())?;
    }
    reply(db, pick_id).await
}

/// 直近のユーザー発言に対する AI の応答を生成する
pub async fn reply(db: &Db, pick_id: i64) -> Result<Vec<Message>, String> {
    let (gemini, pick, msgs, understood) = context(db, pick_id)?;
    if msgs.last().map(|m| m.role.as_str()) != Some("user") {
        return Ok(msgs);
    }
    let reply = gemini
        .generate(&prompts::tutor_system(&pick, &understood), &to_turns(&msgs), false, 0.7)
        .await
        .map_err(|e| e.to_string())?;
    let conn = db.lock().unwrap();
    db::add_message(&conn, pick_id, "model", "chat", &reply).map_err(|e| e.to_string())?;
    if pick.status == "ready" {
        db::set_pick_status(&conn, pick_id, "in_progress").map_err(|e| e.to_string())?;
    }
    db::list_messages(&conn, pick_id).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Checklist {
    #[serde(default)]
    pub core: bool,
    #[serde(default)]
    pub purpose: bool,
    #[serde(default)]
    pub example: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verdict {
    pub passed: bool,
    #[serde(default)]
    pub checklist: Checklist,
    pub feedback: String,
    #[serde(default)]
    pub concept: String,
}

/// 「一言でいうと」の提出と判定
pub async fn submit_summary(db: &Db, pick_id: i64, summary: &str) -> Result<Verdict, String> {
    let summary = summary.trim();
    if summary.is_empty() {
        return Err("まとめが空です".into());
    }
    let (gemini, pick, _msgs, _) = context(db, pick_id)?;
    let mut verdict: Verdict = gemini
        .generate_json(
            &prompts::judge_system(&pick),
            &[Turn { role: "user".into(), text: summary.into() }],
            0.2,
        )
        .await
        .map_err(|e| e.to_string())?;
    // チェックリストと合否の整合を取る（core と purpose、example が揃ってパス）
    let c = &verdict.checklist;
    verdict.passed = verdict.passed && c.core && c.purpose && c.example;

    let conn = db.lock().unwrap();
    let label = if verdict.passed { "✅ パス" } else { "🔁 もう一歩" };
    db::add_message(&conn, pick_id, "user", "summary", summary).map_err(|e| e.to_string())?;
    db::add_message(&conn, pick_id, "model", "verdict", &format!("{label}\n\n{}", verdict.feedback))
        .map_err(|e| e.to_string())?;
    db::add_memo(&conn, pick_id, &verdict.concept, summary, &verdict.feedback, verdict.passed)
        .map_err(|e| e.to_string())?;
    if verdict.passed {
        db::set_pick_status(&conn, pick_id, "completed").map_err(|e| e.to_string())?;
    }
    Ok(verdict)
}
