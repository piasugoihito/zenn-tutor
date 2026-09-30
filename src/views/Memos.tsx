import { useEffect, useState } from "react";
import { api, Memo } from "../api";

export default function Memos() {
  const [memos, setMemos] = useState<Memo[] | null>(null);

  useEffect(() => {
    api.memos().then(setMemos).catch(console.error);
  }, []);

  if (!memos) return <div className="empty">読み込み中…</div>;
  const passed = memos.filter((m) => m.passed);

  return (
    <div className="page">
      <header className="page-header">
        <h1>理解メモ</h1>
        <p className="muted">
          獲得した概念 {passed.length} 個。ここに並ぶ概念は翌日以降の記事選定と対話に反映され、重複を避けて次の一歩を選びます。
        </p>
      </header>
      {memos.length === 0 ? (
        <p className="muted">まだありません。対話の最後に「一言まとめ」を提出するとここに記録されます。</p>
      ) : (
        <ul className="memo-list">
          {memos.map((m) => (
            <li key={m.id} className={`memo ${m.passed ? "pass" : "retry"}`}>
              <div className="memo-head">
                <span className={`tag ${m.passed ? "done" : ""}`}>{m.passed ? "パス" : "再挑戦"}</span>
                {m.passed && <strong>{m.concept}</strong>}
                <span className="muted small grow-right">{m.created_at.slice(0, 10)}</span>
              </div>
              <p className="muted small">{m.title}</p>
              <blockquote>{m.user_summary}</blockquote>
              <p className="small">{m.feedback}</p>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
