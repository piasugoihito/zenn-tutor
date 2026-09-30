import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, Status } from "./api";
import Home from "./views/Home";
import Session from "./views/Session";
import History from "./views/History";
import Memos from "./views/Memos";
import SettingsView from "./views/Settings";
import "./App.css";

export type Route =
  | { name: "home" }
  | { name: "session"; pickId: number }
  | { name: "history" }
  | { name: "memos" }
  | { name: "settings" };

export default function App() {
  const [route, setRoute] = useState<Route>({ name: "home" });
  const [status, setStatus] = useState<Status | null>(null);

  const refresh = useCallback(() => {
    api.status().then(setStatus).catch(console.error);
  }, []);

  useEffect(() => {
    refresh();
    const un = listen("status-changed", refresh);
    // 日付をまたいで開きっぱなしの場合にも表示を追従させる
    const t = setInterval(refresh, 60_000);
    window.addEventListener("focus", refresh);
    return () => {
      un.then((f) => f());
      clearInterval(t);
      window.removeEventListener("focus", refresh);
    };
  }, [refresh]);

  const nav = (r: Route["name"], label: string) => (
    <button
      className={`nav-item ${route.name === r || (r === "home" && route.name === "session") ? "active" : ""}`}
      onClick={() => setRoute({ name: r } as Route)}
    >
      {label}
      {r === "home" && status && status.backlog.length > 0 && (
        <span className="badge">{status.backlog.length}</span>
      )}
    </button>
  );

  return (
    <div className={`app ${route.name === "session" ? "in-session" : ""}`}>
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">Z</span>
          <span>ZennTutor</span>
        </div>
        <nav>
          {nav("home", "今日の1本")}
          {nav("history", "履歴")}
          {nav("memos", "理解メモ")}
          {nav("settings", "設定")}
        </nav>
        {status && <div className={`sidebar-status st-${status.state}`}>{status.label}</div>}
      </aside>
      <main className="main">
        {route.name === "home" && (
          <Home status={status} onRefresh={refresh} onOpen={(pickId) => setRoute({ name: "session", pickId })} onSettings={() => setRoute({ name: "settings" })} />
        )}
        {route.name === "session" && (
          <Session key={route.pickId} pickId={route.pickId} onBack={() => setRoute({ name: "home" })} onChanged={refresh} />
        )}
        {route.name === "history" && <History onOpen={(pickId) => setRoute({ name: "session", pickId })} />}
        {route.name === "memos" && <Memos />}
        {route.name === "settings" && <SettingsView onSaved={refresh} />}
      </main>
    </div>
  );
}
