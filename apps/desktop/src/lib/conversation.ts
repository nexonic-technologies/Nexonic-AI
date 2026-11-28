/** The conversation with Sharee and Nexa: real replies from the chosen brain,
 *  streamed into the panel. Nothing is generated here. */

import { useCallback, useRef, useState } from "react";
import { ipc, type ChatTurn } from "./ipc";

export type Speaker = "You" | "Sharee" | "Nexa";
export type Agent = "Sharee" | "Nexa";

export interface Turn {
  id: number;
  who: Speaker;
  text: string;
  /** Which model answered, e.g. "Qwen3.5 2B · on this Mac". */
  brain?: string;
  /** How many remembered items the agent was given for this reply. */
  remembered?: number;
  /** Set when the chosen brain failed and another one answered. */
  note?: string;
  error?: string;
  streaming?: boolean;
}

/** Name one of them to talk to just that agent; otherwise both answer, Sharee first. */
export function responders(text: string): Agent[] {
  const s = /\bsharee\b/i.test(text);
  const n = /\bnexa\b/i.test(text);
  if (s && !n) return ["Sharee"];
  if (n && !s) return ["Nexa"];
  return ["Sharee", "Nexa"];
}

/** History from one agent's point of view: its own replies are "assistant",
 *  the user's and the partner's lines are "user" (the partner's are labelled). */
export function historyFor(agent: Agent, turns: Turn[]): ChatTurn[] {
  return turns
    .filter((t) => !t.error && t.text.trim())
    .slice(-16)
    .map((t) =>
      t.who === agent
        ? { role: "assistant", content: t.text }
        : { role: "user", content: t.who === "You" ? t.text : `[${t.who}]: ${t.text}` },
    );
}

/** `onSaved` runs after an exchange wrote something to memory. */
export function useConversation(onSaved?: (written: number) => void) {
  const [turns, setTurns] = useState<Turn[]>([]);
  const [speaking, setSpeaking] = useState<Agent | null>(null);
  const [waiting, setWaiting] = useState<Agent | null>(null);
  const busy = useRef(false);
  const nextId = useRef(1);
  const log = useRef<Turn[]>([]);

  const commit = (next: Turn[]) => {
    log.current = next;
    setTurns(next);
  };
  const patch = (id: number, p: Partial<Turn>) =>
    commit(log.current.map((t) => (t.id === id ? { ...t, ...p } : t)));

  const send = useCallback(async (text: string) => {
    const msg = text.trim();
    if (!msg || busy.current) return;
    busy.current = true;
    commit([...log.current, { id: nextId.current++, who: "You", text: msg }]);
    const replies: { agent: Agent; text: string }[] = [];
    try {
      for (const agent of responders(msg)) {
        const history = historyFor(agent, log.current);
        const id = nextId.current++;
        commit([...log.current, { id, who: agent, text: "", streaming: true }]);
        setWaiting(agent);
        let acc = "";
        try {
          const full = await ipc.agentReply(agent, history, (e) => {
            if (e.kind === "start") patch(id, { brain: e.brain, remembered: e.remembered, note: e.note ?? undefined });
            else {
              acc += e.text;
              setWaiting(null);
              setSpeaking(agent);
              patch(id, { text: acc });
            }
          });
          if (full === null) {
            patch(id, { streaming: false, error: "Replies need the desktop app." });
            break;
          }
          patch(id, { text: full, streaming: false });
          if (full.trim()) replies.push({ agent, text: full });
        } catch (err) {
          patch(id, { streaming: false, text: acc, error: String(err) });
          break;
        } finally {
          setWaiting(null);
          setSpeaking(null);
        }
      }
      // Explicit "remember that …" facts are kept always; the exchange only
      // when the user turned on "Remember our conversations".
      const written = await ipc.rememberExchange(msg, replies).catch(() => 0);
      if (written) {
        onSaved?.(written);
        // In the background: tidy old notes once an area is full (on this Mac).
        void ipc.memoryConsolidate().then((n) => { if (n) onSaved?.(n); }).catch(() => undefined);
      }
    } finally {
      busy.current = false;
    }
  }, [onSaved]);

  const stop = useCallback(() => {
    void ipc.brainStop();
  }, []);

  const clear = useCallback(() => {
    if (!busy.current) commit([]);
  }, []);

  return { turns, send, stop, clear, speaking, waiting, busy: speaking !== null || waiting !== null };
}
