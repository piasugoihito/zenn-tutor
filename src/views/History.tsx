import { useEffect, useState } from "react";
import { api, Pick } from "../api";

const LABEL: Record<Pick["status"], string> = {
  ready: "未着手",
  in_progress: "対話途中",
  completed: "完了",
};

export default function History({ onOpen }: { onOpen: (pickId: number) => void }) {
  const [picks, setPicks] = useState<Pick[] | null>(null);

  useEffect(() => {
    api.listPicks().then(setPicks).catch(console.error);
  }, []);

  if (!picks) return <div className="empty">読み込み中…</div>;
  const done = picks.filter((p) => p.status === "completed").length;

  return (
    <div className="page">
      <header className="page-header">
        <h1>履歴</h1>
        <p className="muted">
          これまでの厳選記事 {picks.length} 本 / 対話完了 {done} 本
        </p>
      </header>
      {picks.length === 0 ? (
        <p className="muted">まだ記事がありません。</p>
      ) : (
        <ul className="list">
          {picks.map((p) => (
            <li key={p.id}>
              <button className="list-row" onClick={() => onOpen(p.id)}>
                <span className="muted mono">{p.date}</span>
                <span className="grow">{p.title}</span>
                <span className={`tag ${p.status === "completed" ? "done" : ""}`}>{LABEL[p.status]}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
