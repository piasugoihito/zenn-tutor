import { useEffect, useRef, useState } from "react";
import Markdown from "react-markdown";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, errorText, Message, Pick, Verdict } from "../api";

interface Props {
  pickId: number;
  onBack: () => void;
  onChanged: () => void;
}

const QUICK = ["ここが分からない", "例えて", "Dartでいうと？", "Djangoでいうと？"];

export default function Session({ pickId, onBack, onChanged }: Props) {
  const [pick, setPick] = useState<Pick | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [mode, setMode] = useState<"chat" | "summary">("chat");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [verdict, setVerdict] = useState<Verdict | null>(null);
  const bottom = useRef<HTMLDivElement>(null);

  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
      onChanged();
    }
  };

  const start = () =>
    run(async () => {
      setMessages(await api.startChat(pickId));
    });

  useEffect(() => {
    (async () => {
      const p = await api.getPick(pickId);
      setPick(p);
      const msgs = await api.messages(pickId);
      setMessages(msgs);
      if (p && msgs.length === 0 && p.status !== "completed") start();
    })().catch((e) => setError(errorText(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pickId]);

  useEffect(() => {
    bottom.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, busy]);

  const submit = (text = input) => {
    const t = text.trim();
    if (!t || busy) return;
    setInput("");
    // 送信直後に自分の発言を表示（API 失敗時もサーバ側に保存済み）
    setMessages((m) => [
      ...m,
      { id: -Date.now(), role: "user", kind: mode === "summary" ? "summary" : "chat", content: t, created_at: "" },
    ]);
    if (mode === "summary") {
      run(async () => {
        const v = await api.submitSummary(pickId, t);
        setVerdict(v);
        setMessages(await api.messages(pickId));
        setPick(await api.getPick(pickId));
        if (!v.passed) setMode("chat");
      });
    } else {
      run(async () => {
        setMessages(await api.send(pickId, t));
      });
    }
  };

  const retry = () => {
    if (messages.length === 0) return start();
    const last = messages[messages.length - 1];
    if (last.role === "user" && last.kind === "chat") {
      run(async () => {
        // 直前の発言への応答が失敗した場合は、同じ内容で再問い合わせ
        setMessages(await api.send(pickId, "（先ほどの質問への回答をお願いします）"));
      });
    }
  };

  if (!pick) return <div className="empty">{error ?? "読み込み中…"}</div>;
  const completed = pick.status === "completed";

  return (
    <div className="session">
      <aside className="session-side">
        <button className="link back" onClick={onBack}>← 戻る</button>
        <h2>{pick.title}</h2>
        <ol className="bridge small">
          {pick.bridge.map((l, i) => (
            <li key={i}>{l}</li>
          ))}
        </ol>
        <p className="muted small">{pick.summary}</p>
        <button onClick={() => openUrl(pick.url)}>記事を開く ↗</button>
        <div className="howto small muted">
          <p>AI の問いかけに自分の言葉で答えていきましょう。分からなければ遠慮なく「例えて」「Dartでいうと？」と聞いてOK。</p>
          <p>最後に「一言まとめ」を提出し、パスが出たら今日の学習は完了です。</p>
        </div>
      </aside>

      <section className="chat">
        <div className="chat-log">
          {messages.map((m) => (
            <div key={m.id} className={`msg ${m.role} ${m.kind}`}>
              {m.kind === "summary" && <div className="msg-label">一言まとめ</div>}
              <Markdown>{m.content}</Markdown>
            </div>
          ))}
          {busy && <div className="msg model typing">考えています…</div>}
          {verdict && (
            <div className={`verdict ${verdict.passed ? "pass" : "retry"}`}>
              <strong>{verdict.passed ? "🎉 パス！今日の学習は完了です" : "もう一歩。対話を続けてから再提出しましょう"}</strong>
              <ul>
                <li>{verdict.checklist.core ? "✅" : "⬜️"} 中心概念を自分の言葉で言い換えている</li>
                <li>{verdict.checklist.purpose ? "✅" : "⬜️"} 何のためのものかに触れている</li>
                <li>{verdict.checklist.example ? "✅" : "⬜️"} 具体例・手持ち知識との対応を挙げている</li>
              </ul>
              {verdict.passed && verdict.concept && <p>獲得した概念：<b>{verdict.concept}</b></p>}
            </div>
          )}
          {error && (
            <div className="error">
              {error}
              <button className="link" onClick={retry}>再試行</button>
            </div>
          )}
          <div ref={bottom} />
        </div>

        {!completed && (
          <div className="composer">
            <div className="composer-top">
              <div className="mode">
                <button className={mode === "chat" ? "active" : ""} onClick={() => setMode("chat")}>対話</button>
                <button className={mode === "summary" ? "active" : ""} onClick={() => setMode("summary")}>一言まとめを提出</button>
              </div>
              {mode === "chat" && (
                <div className="quick">
                  {QUICK.map((q) => (
                    <button key={q} disabled={busy} onClick={() => submit(q)}>{q}</button>
                  ))}
                </div>
              )}
            </div>
            <div className="composer-input">
              <textarea
                value={input}
                placeholder={
                  mode === "summary"
                    ? "この記事を一言でいうとどういうこと？ 何のためのものか・具体例（Django/Flutterでいうと等）も添えて"
                    : "自分の言葉で答えてみよう（⌘+Enter で送信）"
                }
                onChange={(e) => setInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                    e.preventDefault();
                    submit();
                  }
                }}
                rows={3}
              />
              <button className="primary" disabled={busy || !input.trim()} onClick={() => submit()}>
                {mode === "summary" ? "提出" : "送信"}
              </button>
            </div>
          </div>
        )}
        {completed && <div className="completed-bar">✅ この記事の学習は完了しています（{pick.completed_at?.slice(0, 10)}）</div>}
      </section>
    </div>
  );
}
