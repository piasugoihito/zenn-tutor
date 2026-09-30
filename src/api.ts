import { invoke } from "@tauri-apps/api/core";

export type PickStatus = "ready" | "in_progress" | "completed";

export interface Pick {
  id: number;
  date: string;
  url: string;
  title: string;
  author: string;
  topics: string[];
  summary: string;
  bridge: string[];
  related: string;
  rule_score: number;
  status: PickStatus;
  created_at: string;
  completed_at: string | null;
}

export interface Status {
  state: "no_key" | "waiting" | "preparing" | "ready" | "completed" | "error";
  label: string;
  today: Pick | null;
  backlog: Pick[];
  error: string | null;
  scout_hour: number;
}

export interface Message {
  id: number;
  role: "user" | "model";
  kind: "chat" | "summary" | "verdict";
  content: string;
  created_at: string;
}

export interface Memo {
  id: number;
  pick_id: number;
  title: string;
  concept: string;
  user_summary: string;
  feedback: string;
  passed: boolean;
  created_at: string;
}

export interface Verdict {
  passed: boolean;
  checklist: { core: boolean; purpose: boolean; example: boolean };
  feedback: string;
  concept: string;
}

export interface Settings {
  has_api_key: boolean;
  model: string;
  scout_hour: number;
  keywords: string[];
  topics: string[];
  candidates: number;
  max_age_days: number;
  autostart: boolean;
}

export interface Candidate {
  url: string;
  title: string;
  topics: string[];
  score: number;
  reasons: string[];
}

export const api = {
  status: () => invoke<Status>("get_status"),
  scoutNow: () => invoke<void>("scout_now"),
  previewCandidates: () => invoke<Candidate[]>("preview_candidates"),
  getPick: (id: number) => invoke<Pick | null>("get_pick", { id }),
  listPicks: () => invoke<Pick[]>("list_picks"),
  messages: (pickId: number) => invoke<Message[]>("get_messages", { pickId }),
  memos: () => invoke<Memo[]>("list_memos"),
  startChat: (pickId: number) => invoke<Message[]>("start_chat", { pickId }),
  send: (pickId: number, text: string) => invoke<Message[]>("send_message", { pickId, text }),
  submitSummary: (pickId: number, summary: string) =>
    invoke<Verdict>("submit_summary", { pickId, summary }),
  settings: () => invoke<Settings>("get_settings"),
  saveSettings: (s: Settings) => invoke<Settings>("save_settings", { new: s }),
  setApiKey: (key: string) => invoke<void>("set_api_key", { key }),
  testApiKey: () => invoke<string>("test_api_key"),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
