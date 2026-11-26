import { useEffect, useRef, useState } from "react";
import type { Gaze, Mood, Temperament } from "../components/LiveFace";

/**
 * Mood and gaze for Sharee and Nexa, driven only by real events: where the
 * cursor is, what the user hovers or clicks, what the user types and sends.
 * The personalities come from the expression datasheets: Sharee is analytical
 * and composed (smaller, slower reactions), Nexa is empathetic and expressive.
 * Until a model provider is connected (M17) the agents cannot answer, so they
 * react to the user but never pretend to reply.
 */

export type Agent = "Sharee" | "Nexa";

export const TEMPERAMENT: Record<Agent, Temperament> = {
  Sharee: { blinkMin: 3200, blinkMax: 6400, doubleBlink: 0.08, gazeGain: 0.9, saccade: 1.6 },
  Nexa: { blinkMin: 2400, blinkMax: 5200, doubleBlink: 0.16, gazeGain: 1.05, saccade: 2.4 },
};

export const MOOD_LABEL: Record<Mood, string> = {
  neutral: "Present",
  friendly: "Friendly",
  happy: "Happy",
  attentive: "Listening",
  curious: "Curious",
  thinking: "Thinking",
  concerned: "Concerned",
  confident: "Confident",
  calm: "Calm",
  determined: "Determined",
};

/** What the user's words suggest, before any model reads them. */
export function moodFromText(text: string, who: Agent): Mood {
  const t = text.toLowerCase();
  if (!t.trim()) return "neutral";
  if (/\b(thank|thanks|great|awesome|love|nice|perfect|amazing|good job|well done|shukriya)\b/.test(t)) return "happy";
  if (/\b(error|bug|broken|fail|failed|crash|problem|issue|stuck|urgent|wrong|help|sad|worried)\b/.test(t)) return "concerned";
  if (/\b(build|fix|plan|ship|make|create|let'?s|deadline|finish|deploy)\b/.test(t)) return "determined";
  if (t.includes("?") || /^(what|why|how|when|where|who|can|could|should|is|are|do|does)\b/.test(t)) {
    return who === "Sharee" ? "thinking" : "curious";
  }
  return "attentive";
}

const ANALYTICAL_NODES = new Set(["Reasoning", "Planning", "Memory"]);

type Inputs = {
  hoverAgent: Agent | null;
  hoverNode: string | null;
  draft: string;
  sentAt: number; // ms timestamp of the last send, 0 if none
  greetAt: Record<Agent, number>; // last click on each agent
  nodeEl: (id: string) => HTMLElement | null;
  agentEl: (who: Agent) => HTMLElement | null;
  inputEl: () => HTMLElement | null;
};

export type AgentPresence = { mood: Mood; gaze: Gaze };
export type PresenceView = Record<Agent, { mood: Mood; gaze: () => Gaze }>;

const centre = (el: HTMLElement | null): Gaze => {
  if (!el) return null;
  const r = el.getBoundingClientRect();
  return { x: r.left + r.width / 2, y: r.top + r.height * 0.4 };
};

export function usePresenceMood(inputs: Inputs): PresenceView {
  const ref = useRef(inputs);
  ref.current = inputs;
  const pointer = useRef({ x: 0, y: 0, at: 0 });
  const hoverSince = useRef<{ who: Agent | null; at: number }>({ who: null, at: 0 });
  // gaze changes every frame, so it lives in a ref; only mood changes re-render
  const gazes = useRef<Record<Agent, Gaze>>({ Sharee: null, Nexa: null });
  const [moods, setMoods] = useState<Record<Agent, Mood>>({ Sharee: "neutral", Nexa: "neutral" });
  const [view] = useState(() => ({
    Sharee: () => gazes.current.Sharee,
    Nexa: () => gazes.current.Nexa,
  }));

  useEffect(() => {
    const onMove = (e: PointerEvent) => { pointer.current = { x: e.clientX, y: e.clientY, at: performance.now() }; };
    window.addEventListener("pointermove", onMove);

    const decide = (who: Agent, now: number): AgentPresence => {
      const i = ref.current;
      const other: Agent = who === "Sharee" ? "Nexa" : "Sharee";
      const p = pointer.current;
      const cursor: Gaze = now - p.at < 4000 ? { x: p.x, y: p.y } : null;
      const hs = hoverSince.current;
      const sinceSend = now - i.sentAt;
      const sinceGreet = now - i.greetAt[who];

      // 1. just greeted (clicked): a warm reaction
      if (sinceGreet < 2600) return { mood: who === "Nexa" ? "happy" : "friendly", gaze: cursor };
      // 2. a message was sent but nobody can answer yet
      if (i.sentAt && sinceSend < 1800) return { mood: "thinking", gaze: centre(i.inputEl()) };
      if (i.sentAt && sinceSend < 4200) return { mood: who === "Nexa" ? "concerned" : "calm", gaze: null };
      // 3. the user is typing: listen, read the tone
      if (i.draft.trim()) {
        const m = moodFromText(i.draft, who);
        return { mood: m === "neutral" ? "attentive" : m, gaze: centre(i.inputEl()) };
      }
      // 4. the user is looking at this agent
      if (i.hoverAgent === who) {
        const held = now - (hs.who === who ? hs.at : now);
        const mood: Mood = held < 900 ? "attentive" : held < 4000 ? "friendly" : who === "Nexa" ? "happy" : "confident";
        return { mood, gaze: cursor };
      }
      // 5. the user is looking at the partner: glance over
      if (i.hoverAgent === other) return { mood: "neutral", gaze: centre(i.agentEl(other)) };
      // 6. a cognitive node is hovered: both study it
      if (i.hoverNode) {
        const m: Mood = ANALYTICAL_NODES.has(i.hoverNode) ? (who === "Sharee" ? "thinking" : "curious") : "curious";
        return { mood: m, gaze: centre(i.nodeEl(i.hoverNode)) };
      }
      // 7. resting: follow the cursor while it moves, settle to calm when the user is away
      if (now - p.at > 25000 && p.at > 0) return { mood: "calm", gaze: null };
      return { mood: "neutral", gaze: cursor };
    };

    const tick = () => {
      const now = performance.now();
      const hs = hoverSince.current;
      if (ref.current.hoverAgent !== hs.who) hoverSince.current = { who: ref.current.hoverAgent, at: now };
      const s = decide("Sharee", now);
      const n = decide("Nexa", now);
      gazes.current = { Sharee: s.gaze, Nexa: n.gaze };
      setMoods((prev) => (prev.Sharee === s.mood && prev.Nexa === n.mood ? prev : { Sharee: s.mood, Nexa: n.mood }));
    };
    const id = window.setInterval(tick, 60);
    return () => { window.clearInterval(id); window.removeEventListener("pointermove", onMove); };
  }, []);

  return {
    Sharee: { mood: moods.Sharee, gaze: view.Sharee },
    Nexa: { mood: moods.Nexa, gaze: view.Nexa },
  };
}
