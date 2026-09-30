//! 記事スカウト：1次選抜（RSS + ルールベース、API 呼び出しなし）→ 2次選抜（Gemini）

use std::collections::{BTreeSet, HashMap, HashSet};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::db::{self, Db, NewPick};
use crate::gemini::{Gemini, GeminiError, Turn};
use crate::prompts;
use crate::settings::{self, Settings};

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub url: String,
    pub title: String,
    pub author: String,
    pub description: String,
    /// 記事が現れたトピックフィード（= タグ）
    pub topics: BTreeSet<String>,
    pub published: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub score: f64,
    pub reasons: Vec<String>,
}

#[derive(Debug)]
pub enum ScoutError {
    NoApiKey,
    NoCandidates,
    Gemini(GeminiError),
    Other(String),
}

impl std::fmt::Display for ScoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoApiKey => write!(f, "Gemini API キーが未設定です。設定画面から登録してください。"),
            Self::NoCandidates => write!(f, "条件に合う新着記事が見つかりませんでした。"),
            Self::Gemini(e) => write!(f, "{e}"),
            Self::Other(m) => write!(f, "{m}"),
        }
    }
}

impl From<GeminiError> for ScoutError {
    fn from(e: GeminiError) -> Self {
        Self::Gemini(e)
    }
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("ZennTutor/0.1 (personal learning app)")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("reqwest client")
}

// ---------- 取得 ----------

async fn fetch_feeds(topics: &[String]) -> Result<Vec<Candidate>, ScoutError> {
    let client = http();
    let mut by_url: HashMap<String, Candidate> = HashMap::new();
    let mut ok_count = 0;

    for topic in topics {
        let url = format!("https://zenn.dev/topics/{}/feed", topic.trim());
        let bytes = match client.get(&url).send().await.and_then(|r| r.error_for_status()) {
            Ok(r) => match r.bytes().await {
                Ok(b) => b,
                Err(_) => continue,
            },
            Err(e) => {
                eprintln!("[scout] feed fetch failed {url}: {e}");
                continue;
            }
        };
        let Ok(channel) = rss::Channel::read_from(&bytes[..]) else {
            eprintln!("[scout] feed parse failed {url}");
            continue;
        };
        ok_count += 1;
        for item in channel.items() {
            let Some(link) = item.link().map(|l| l.split('?').next().unwrap_or(l).to_string()) else {
                continue;
            };
            let entry = by_url.entry(link.clone()).or_insert_with(|| Candidate {
                url: link,
                title: item.title().unwrap_or_default().to_string(),
                author: item
                    .dublin_core_ext()
                    .and_then(|dc| dc.creators().first().cloned())
                    .unwrap_or_default(),
                description: clean_text(item.description().unwrap_or_default()),
                topics: BTreeSet::new(),
                published: item
                    .pub_date()
                    .and_then(|d| chrono::DateTime::parse_from_rfc2822(d).ok()),
                score: 0.0,
                reasons: vec![],
            });
            entry.topics.insert(topic.trim().to_string());
        }
    }

    if ok_count == 0 {
        return Err(ScoutError::Other("Zenn の RSS を取得できませんでした（ネットワークを確認してください）。".into()));
    }
    Ok(by_url.into_values().collect())
}

// ---------- 1次選抜（ルールベーススコアリング） ----------

struct Rule {
    label: &'static str,
    re: Regex,
    weight: f64,
}

fn rule(label: &'static str, pattern: &str, weight: f64) -> Rule {
    Rule { label, re: Regex::new(&format!("(?i){pattern}")).unwrap(), weight }
}

/// キーワードを表記ゆれに強い正規表現にする（"Next.js" → "next\.?\s*js" など）
fn keyword_regex(keyword: &str) -> Option<Regex> {
    let norm: String = keyword.split_whitespace().collect::<Vec<_>>().join("");
    if norm.is_empty() {
        return None;
    }
    let mut pat = String::new();
    for (i, ch) in norm.chars().enumerate() {
        if ch == '.' || ch == '-' || ch == '_' {
            pat.push_str(r"[.\-_]?\s*");
            continue;
        }
        if i > 0 {
            pat.push_str(r"\s*");
        }
        pat.push_str(&regex::escape(&ch.to_string()));
    }
    // よく使われる別表記
    let alias = match norm.to_lowercase().as_str() {
        "aiエージェント" => r"|ai\s*agents?|エージェント|agentic|mcp",
        "システム設計" => r"|system\s*design|設計",
        "アーキテクチャ" => r"|architecture",
        "typescript" => r"|\bts\b",
        _ => "",
    };
    Regex::new(&format!("(?i)(?:{pat}{alias})")).ok()
}

/// 1次選抜のスコアを計算する。返り値は上位 `limit` 件。
pub fn score_candidates(mut cands: Vec<Candidate>, s: &Settings, exclude: &HashSet<String>) -> Vec<Candidate> {
    let keywords: Vec<(String, Regex)> = s
        .keywords
        .iter()
        .filter_map(|k| keyword_regex(k).map(|r| (k.clone(), r)))
        .collect();

    // 手持ち知識との「橋渡し」になる語
    let bridge_rules = [
        rule("Django/Flutter との接点", r"django|flutter|dart|riverpod|provider|freezed|dio|simplejwt|jwt|postgres", 3.0),
        rule("抽象概念の比較", r"比較|違い|対応|置き換え|移行|乗り換え|から学ぶ|vs\.?", 1.5),
    ];
    // 設計・ベストプラクティス系
    let design_rules = [
        rule("設計", r"設計|アーキテクチャ|architecture|design", 2.0),
        rule("ベストプラクティス", r"ベストプラクティス|best\s*practice|プラクティス|原則|パターン|pattern|アンチパターン", 2.0),
        rule("責務・構成", r"責務|関心の分離|レイヤ|境界|構成|全体像|選定|トレードオフ|キャッシュ戦略|認証|認可|状態管理|データフェッチ|レンダリング戦略", 1.5),
        rule("運用", r"運用|本番|スケール|パフォーマンス|可観測性|監視|セキュリティ", 1.0),
    ];
    // 難易度不足
    let too_easy = [
        rule("入門すぎ", r"入門|初心者|初学者|はじめて|初めて|始め方|超基礎|基礎の基礎|チュートリアル|hello\s*world|環境構築|インストール|セットアップ手順|やってみた|触ってみた|試してみた", -4.0),
    ];
    // 尖りすぎ・ポエム
    let too_niche = [
        rule("個別バグ修正ログ", r"エラー|error|解決|ハマった|ハマり|詰まった|遭遇|不具合|バグ|直し方|対処|うまくいかない|できない|trouble", -3.0),
        rule("ポエム・雑記", r"ポエム|振り返り|退職|転職|入社|感想|雑記|日記|備忘録|キャリア|年収|反省|所感|参加レポ|登壇", -3.0),
    ];

    let now = chrono::Local::now().fixed_offset();

    for c in cands.iter_mut() {
        let title = &c.title;
        let body = &c.description;
        let mut score = 0.0;
        let mut reasons = vec![];

        // キーワードシード：タイトル一致 +3、本文冒頭の出現頻度 +0.5/回（上限 +2）、トピックタグ一致 +2
        for (kw, re) in &keywords {
            let mut kw_score = 0.0;
            if re.is_match(title) {
                kw_score += 3.0;
            }
            kw_score += (re.find_iter(body).count() as f64 * 0.5).min(2.0);
            if c.topics.iter().any(|t| re.is_match(t) || t.eq_ignore_ascii_case(&kw.replace(['.', ' '], ""))) {
                kw_score += 2.0;
            }
            if kw_score > 0.0 {
                score += kw_score;
                reasons.push(format!("{kw} +{kw_score:.1}"));
            }
        }
        // 複数トピックにまたがる記事 = 横断的な設計記事の可能性が高い
        if c.topics.len() > 1 {
            let bonus = (c.topics.len() - 1) as f64;
            score += bonus;
            reasons.push(format!("複数トピック +{bonus:.1}"));
        }

        let mut apply = |rules: &[Rule], title_only: bool, cap: f64| {
            let mut sub = 0.0;
            for r in rules {
                let hit = r.re.is_match(title) || (!title_only && r.re.is_match(body));
                if hit {
                    sub += r.weight;
                    reasons.push(format!("{} {:+.1}", r.label, r.weight));
                }
            }
            score += if sub > 0.0 { sub.min(cap) } else { sub.max(-cap) };
        };
        apply(&bridge_rules, false, 4.5);
        apply(&design_rules, false, 5.0);
        // 減点はタイトルで判定（本文冒頭の「エラー」等の言及まで減点すると良記事を落とすため）
        apply(&too_easy, true, 6.0);
        apply(&too_niche, true, 6.0);

        // 本文が極端に短い記事は薄い可能性
        if body.chars().count() < 80 {
            score -= 1.0;
            reasons.push("本文が短い -1.0".into());
        }
        // 新しさ（直近ほどわずかに加点）
        if let Some(p) = c.published {
            let days = (now - p).num_days();
            if days <= 2 {
                score += 0.5;
            }
        }

        c.score = score;
        c.reasons = reasons;
    }

    let max_age = chrono::Duration::days(s.max_age_days);
    cands.retain(|c| {
        !exclude.contains(&c.url)
            && c.published.map(|p| now - p <= max_age).unwrap_or(true)
            && c.score > 0.0
    });
    cands.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    cands.truncate(s.candidates.max(1));
    cands
}

// ---------- 記事本文 ----------

pub fn clean_text(html: &str) -> String {
    let no_script = Regex::new(r"(?is)<(script|style)[^>]*>.*?</\s*(script|style)>").unwrap();
    let block = Regex::new(r"(?i)</?(p|div|br|li|h[1-6]|pre|tr|blockquote)[^>]*>").unwrap();
    let tags = Regex::new(r"(?s)<[^>]+>").unwrap();
    let blank = Regex::new(r"\n\s*\n+").unwrap();
    let s = no_script.replace_all(html, "");
    let s = block.replace_all(&s, "\n");
    let s = tags.replace_all(&s, "");
    let s = html_escape::decode_html_entities(&s);
    blank.replace_all(s.trim(), "\n\n").to_string()
}

/// Zenn の記事 API から本文を取得（失敗時は RSS の抜粋で代替）
async fn fetch_body(url: &str) -> Option<(String, Vec<String>)> {
    let slug = url.trim_end_matches('/').rsplit('/').next()?;
    if !url.contains("/articles/") {
        return None;
    }
    let api = format!("https://zenn.dev/api/articles/{slug}");
    let v: serde_json::Value = http().get(api).send().await.ok()?.json().await.ok()?;
    let html = v["article"]["body_html"].as_str()?;
    let topics = v["article"]["topics"]
        .as_array()
        .map(|a| a.iter().filter_map(|t| t["display_name"].as_str().map(String::from)).collect())
        .unwrap_or_default();
    Some((clean_text(html), topics))
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{t}\n…（以下省略）")
    }
}

// ---------- 2次選抜 ----------

#[derive(Deserialize)]
struct Selection {
    index: usize,
    summary: String,
    bridge: Vec<String>,
    #[serde(default)]
    related: String,
}

/// 1次選抜の結果を返す（デバッグ・画面表示用。API 呼び出しなし）
pub async fn preview(db: &Db) -> Result<Vec<Candidate>, ScoutError> {
    let (s, exclude) = {
        let conn = db.lock().unwrap();
        (settings::load(&conn), db::picked_urls(&conn).unwrap_or_default())
    };
    let cands = fetch_feeds(&s.topics).await?;
    Ok(score_candidates(cands, &s, &exclude.into_iter().collect()))
}

/// スカウトを実行し、`date` の1本を保存する
pub async fn run(db: &Db, date: &str) -> Result<i64, ScoutError> {
    let (s, api_key, exclude, understood) = {
        let conn = db.lock().unwrap();
        if let Ok(Some(p)) = db::get_pick_by_date(&conn, date) {
            return Ok(p.id);
        }
        (
            settings::load(&conn),
            settings::api_key(&conn),
            db::picked_urls(&conn).unwrap_or_default(),
            db::understood_concepts(&conn, 30),
        )
    };
    let api_key = api_key.ok_or(ScoutError::NoApiKey)?;

    // 1次選抜
    let cands = fetch_feeds(&s.topics).await?;
    let top = score_candidates(cands, &s, &exclude.into_iter().collect());
    if top.is_empty() {
        return Err(ScoutError::NoCandidates);
    }

    // 2次選抜（1次通過分のみを Gemini へ）
    let list = top
        .iter()
        .enumerate()
        .map(|(i, c)| {
            format!(
                "[{i}] {}\nタグ: {}\n冒頭: {}",
                c.title,
                c.topics.iter().cloned().collect::<Vec<_>>().join(", "),
                truncate_chars(&c.description.replace('\n', " "), 280)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let gemini = Gemini::new(api_key, s.model.clone());
    let sel: Selection = gemini
        .generate_json(
            &prompts::selection_system(&understood),
            &[Turn { role: "user".into(), text: format!("# 候補記事\n{list}") }],
            0.4,
        )
        .await?;
    let chosen = top.get(sel.index).unwrap_or(&top[0]);

    let (body, api_topics) = fetch_body(&chosen.url)
        .await
        .unwrap_or_else(|| (chosen.description.clone(), vec![]));
    let topics = if api_topics.is_empty() {
        chosen.topics.iter().cloned().collect()
    } else {
        api_topics
    };
    let mut bridge = sel.bridge;
    bridge.truncate(3);

    let conn = db.lock().unwrap();
    let id = db::insert_pick(
        &conn,
        &NewPick {
            date: date.to_string(),
            url: chosen.url.clone(),
            title: chosen.title.clone(),
            author: chosen.author.clone(),
            topics,
            summary: sel.summary,
            bridge,
            related: sel.related,
            body_text: truncate_chars(&body, 20_000),
            rule_score: chosen.score,
        },
    )
    .map_err(|e| ScoutError::Other(e.to_string()))?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(title: &str, desc: &str, topics: &[&str]) -> Candidate {
        Candidate {
            url: format!("https://zenn.dev/x/articles/{}", title.len()),
            title: title.into(),
            author: String::new(),
            description: desc.repeat(3),
            topics: topics.iter().map(|t| t.to_string()).collect(),
            published: None,
            score: 0.0,
            reasons: vec![],
        }
    }

    #[test]
    fn keyword_regex_handles_variants() {
        let re = keyword_regex("Next.js").unwrap();
        assert!(re.is_match("NextJS の App Router"));
        assert!(re.is_match("next.js"));
        let re = keyword_regex("AIエージェント").unwrap();
        assert!(re.is_match("AI Agent を設計する"));
    }

    #[test]
    fn scoring_prefers_design_over_intro_and_bugfix() {
        let s = Settings::default();
        let cands = vec![
            cand("Next.js 入門：環境構築から Hello World まで", "Next.js をインストールします。", &["nextjs"]),
            cand("Next.js で遭遇したエラーの解決策", "ビルド時にエラーが出たので直しました。", &["nextjs"]),
            cand(
                "Flutter 経験者のための React 状態管理設計ガイド",
                "Riverpod と比較しながら React の状態管理のベストプラクティスを整理します。",
                &["react", "typescript"],
            ),
        ];
        let top = score_candidates(cands, &s, &HashSet::new());
        assert_eq!(top[0].title, "Flutter 経験者のための React 状態管理設計ガイド");
        assert!(top.iter().all(|c| !c.title.contains("入門")));
    }

    #[test]
    fn clean_text_strips_html() {
        assert_eq!(clean_text("<p>a &amp; b</p><script>x</script>"), "a & b");
    }
}
