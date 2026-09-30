import { useEffect, useState } from "react";
import { api, Candidate, errorText, Settings } from "../api";

const isMobile = /Android|iPhone|iPad/i.test(navigator.userAgent);

export default function SettingsView({ onSaved }: { onSaved: () => void }) {
  const [s, setS] = useState<Settings | null>(null);
  const [keyInput, setKeyInput] = useState("");
  const [keywords, setKeywords] = useState("");
  const [topics, setTopics] = useState("");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [preview, setPreview] = useState<Candidate[] | null>(null);

  const load = (v: Settings) => {
    setS(v);
    setKeywords(v.keywords.join(", "));
    setTopics(v.topics.join(", "));
  };

  useEffect(() => {
    api.settings().then(load).catch(console.error);
  }, []);

  if (!s) return <div className="empty">読み込み中…</div>;

  const split = (v: string) =>
    v
      .split(/[,、\n]/)
      .map((x) => x.trim())
      .filter(Boolean);

  const act = async (fn: () => Promise<string>) => {
    setBusy(true);
    setMsg(null);
    try {
      setMsg({ ok: true, text: await fn() });
    } catch (e) {
      setMsg({ ok: false, text: errorText(e) });
    } finally {
      setBusy(false);
    }
  };

  const saveKey = () =>
    act(async () => {
      await api.setApiKey(keyInput);
      setKeyInput("");
      load(await api.settings());
      onSaved();
      return keyInput.trim() ? "API キーを保存しました" : "API キーを削除しました";
    });

  const save = () =>
    act(async () => {
      load(await api.saveSettings({ ...s, keywords: split(keywords), topics: split(topics) }));
      onSaved();
      return "設定を保存しました";
    });

  const runPreview = () =>
    act(async () => {
      await api.saveSettings({ ...s, keywords: split(keywords), topics: split(topics) });
      const c = await api.previewCandidates();
      setPreview(c);
      return `1次選抜の結果: ${c.length} 本（Gemini は呼び出していません）`;
    });

  return (
    <div className="page settings">
      <header className="page-header">
        <h1>設定</h1>
      </header>

      <section>
        <h3>Gemini API</h3>
        <p className="muted small">
          {s.has_api_key ? "✅ API キー設定済み" : "⚠️ API キー未設定"}（Google AI Studio で無料発行できます{isMobile ? "" : "。環境変数 GEMINI_API_KEY があればそちらを優先"}）
        </p>
        <div className="row">
          <input
            type="password"
            placeholder={s.has_api_key ? "新しいキーで上書き（空で保存すると削除）" : "AIza..."}
            value={keyInput}
            onChange={(e) => setKeyInput(e.target.value)}
          />
          <button onClick={saveKey} disabled={busy}>保存</button>
          <button onClick={() => act(api.testApiKey)} disabled={busy || !s.has_api_key}>接続テスト</button>
        </div>
        <label>
          モデル
          <input value={s.model} onChange={(e) => setS({ ...s, model: e.target.value })} />
        </label>
      </section>

      <section>
        <h3>スカウト</h3>
        <div className="grid">
          <label>
            自動スカウト時刻（この時刻を過ぎたら実行）
            <select value={s.scout_hour} onChange={(e) => setS({ ...s, scout_hour: Number(e.target.value) })}>
              {Array.from({ length: 24 }, (_, h) => (
                <option key={h} value={h}>{h}:00</option>
              ))}
            </select>
          </label>
          <label>
            Gemini に渡す候補数
            <input type="number" min={1} max={10} value={s.candidates} onChange={(e) => setS({ ...s, candidates: Number(e.target.value) })} />
          </label>
          <label>
            対象とする記事の新しさ（日）
            <input type="number" min={1} max={60} value={s.max_age_days} onChange={(e) => setS({ ...s, max_age_days: Number(e.target.value) })} />
          </label>
        </div>
        <label>
          キーワードシード（カンマ区切り）
          <textarea rows={2} value={keywords} onChange={(e) => setKeywords(e.target.value)} />
        </label>
        <label>
          取得する Zenn トピック（zenn.dev/topics/◯◯ の ◯◯、カンマ区切り）
          <textarea rows={2} value={topics} onChange={(e) => setTopics(e.target.value)} />
        </label>
      </section>

      {!isMobile && (
      <section>
        <h3>起動</h3>
        <label className="check">
          <input type="checkbox" checked={s.autostart} onChange={(e) => setS({ ...s, autostart: e.target.checked })} />
          OS ログイン時に自動起動してトレイに常駐する（リリースビルドで有効）
        </label>
      </section>
      )}

      <div className="row">
        <button className="primary" onClick={save} disabled={busy}>設定を保存</button>
        <button onClick={runPreview} disabled={busy}>1次選抜をプレビュー</button>
      </div>
      {msg && <p className={msg.ok ? "ok" : "error"}>{msg.text}</p>}

      {preview && (
        <ol className="preview">
          {preview.map((c) => (
            <li key={c.url}>
              <b>{c.score.toFixed(1)}</b> {c.title}
              <div className="muted small">{c.reasons.join(" / ")}</div>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}
