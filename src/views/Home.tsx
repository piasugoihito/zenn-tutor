import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, errorText, Pick, Status } from "../api";

interface Props {
  status: Status | null;
  onRefresh: () => void;
  onOpen: (pickId: number) => void;
  onSettings: () => void;
}

export function PickCard({ pick, onOpen }: { pick: Pick; onOpen: (id: number) => void }) {
  const cta = pick.status === "completed" ? "対話を振り返る" : pick.status === "in_progress" ? "対話を続ける" : "対話を始める";
  return (
    <article className="pick-card">
      <div className="pick-meta">
        <span>{pick.date}</span>
        {pick.author && <span>@{pick.author}</span>}
        {pick.topics.slice(0, 5).map((t) => (
          <span key={t} className="tag">{t}</span>
        ))}
        {pick.status === "completed" && <span className="tag done">完了</span>}
      </div>
      <h2 className="pick-title">{pick.title}</h2>
      <ol className="bridge">
        {pick.bridge.map((line, i) => (
          <li key={i}>{line}</li>
        ))}
      </ol>
      <p className="pick-summary">{pick.summary}</p>
      {pick.related && <p className="pick-related">関連する手持ち知識：{pick.related}</p>}
      <div className="actions">
        <button className="primary" onClick={() => onOpen(pick.id)}>{cta}</button>
        <button onClick={() => openUrl(pick.url)}>記事を開く ↗</button>
      </div>
    </article>
  );
}

export default function Home({ status, onRefresh, onOpen, onSettings }: Props) {
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const scout = async () => {
    setRunning(true);
    setError(null);
    try {
      await api.scoutNow();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setRunning(false);
      onRefresh();
    }
  };

  if (!status) return <div className="empty">読み込み中…</div>;

  const today = status.today;
  const preparing = running || status.state === "preparing";

  return (
    <div className="page">
      <header className="page-header">
        <h1>今日の1本</h1>
        <p className="muted">{status.label}</p>
      </header>

      {today ? (
        <PickCard pick={today} onOpen={onOpen} />
      ) : (
        <section className="placeholder">
          {status.state === "no_key" ? (
            <>
              <p>Gemini API キーが未設定です。設定後、毎日 {status.scout_hour} 時以降に自動で記事を厳選します。</p>
              <button className="primary" onClick={onSettings}>設定を開く</button>
            </>
          ) : preparing ? (
            <p className="loading">RSS を取得し、今日のあなたに効く1本を選んでいます…</p>
          ) : (
            <>
              <p>
                {status.state === "error"
                  ? "前回のスカウトに失敗しました。時間をおいて自動で再試行します。"
                  : `今日の記事は ${status.scout_hour} 時以降に裏側で自動選定されます。`}
              </p>
              <button className="primary" onClick={scout}>今すぐスカウト</button>
            </>
          )}
          {(error ?? status.error) && !preparing && <p className="error">{error ?? status.error}</p>}
        </section>
      )}

      {status.backlog.length > 0 && (
        <section className="backlog">
          <h3>未読で溜まっている記事（{status.backlog.length}）</h3>
          <ul>
            {status.backlog.map((p) => (
              <li key={p.id}>
                <button className="link" onClick={() => onOpen(p.id)}>
                  <span className="muted">{p.date}</span> {p.title}
                  {p.status === "in_progress" && <span className="tag">対話途中</span>}
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
