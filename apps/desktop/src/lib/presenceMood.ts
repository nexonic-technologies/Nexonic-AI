import { useEffect, useRef, useState } from "react";
import type { Gaze, Mood, Temperament } from "../components/LiveFace";

/**
 * Behaviour for Sharee and Nexa.
 *
 * They do not react to the mouse. They react to the conversation:
 *  - while the user types they listen (look at the message box, attentive,
 *    the tone of the words colours their face);
 *  - after a message is sent they think, and since no model is connected yet
 *    (M17) they cannot answer, so they never pretend to reply;
 *  - a click on one of them is a greeting.
 * Between conversations they behave like two colleagues in the same room:
 * they glance at each other, smile, nod, look back at the user, and Sharee
 * sometimes drifts into thought while Nexa watches him. This idle body
 * language is decorative; it never claims that work is happening.
 * Personalities follow the datasheets: Sharee composed, Nexa expressive.
 */

export type Agent = "Sharee" | "Nexa";

export const TEMPERAMENT: Record<Agent, Temperament> = {
  Sharee: { blinkMin: 3200, blinkMax: 6400, doubleBlink: 0.08, gazeGain: 0.9, saccade: 1.2 },
  Nexa: { blinkMin: 2400, blinkMax: 5200, doubleBlink: 0.16, gazeGain: 1.05, saccade: 1.8 },
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

type Look = "user" | "partner" | "away" | "input";
type Beat = { s: { mood: Mood; look: Look }; n: { mood: Mood; look: Look }; ms: number };

// Idle scenes between conversations: short exchanges of looks and smiles.
const SCENES: Beat[][] = [
  [
    { s: { mood: "neutral", look: "partner" }, n: { mood: "neutral", look: "user" }, ms: 1100 },
    { s: { mood: "friendly", look: "partner" }, n: { mood: "neutral", look: "partner" }, ms: 900 },
    { s: { mood: "friendly", look: "partner" }, n: { mood: "happy", look: "partner" }, ms: 2200 },
    { s: { mood: "neutral", look: "user" }, n: { mood: "friendly", look: "user" }, ms: 2400 },
  ],
  [
    { s: { mood: "thinking", look: "away" }, n: { mood: "neutral", look: "partner" }, ms: 2600 },
    { s: { mood: "thinking", look: "away" }, n: { mood: "curious", look: "partner" }, ms: 1600 },
    { s: { mood: "friendly", look: "partner" }, n: { mood: "happy", look: "partner" }, ms: 2000 },
    { s: { mood: "neutral", look: "user" }, n: { mood: "neutral", look: "user" }, ms: 2200 },
  ],
  [
    { s: { mood: "neutral", look: "partner" }, n: { mood: "neutral", look: "user" }, ms: 1400 },
    { s: { mood: "confident", look: "partner" }, n: { mood: "friendly", look: "partner" }, ms: 1800 },
    { s: { mood: "neutral", look: "user" }, n: { mood: "neutral", look: "user" }, ms: 2600 },
  ],
  [{ s: { mood: "neutral", look: "user" }, n: { mood: "neutral", look: "user" }, ms: 5200 }],
  [{ s: { mood: "calm", look: "user" }, n: { mood: "friendly", look: "user" }, ms: 4200 }],
];

type Inputs = {
  draft: string;
  sentAt: number; // ms timestamp of the last send, 0 if none
  greetAt: Record<Agent, number>; // last click on each agent
  agentEl: (who: Agent) => HTMLElement | null;
  inputEl: () => HTMLElement | null;
  reduced: boolean;
};

export type PresenceView = Record<Agent, { mood: Mood; gaze: () => Gaze }>;

const point = (el: HTMLElement | null, yFrac = 0.35): Gaze => {
  if (!el) return null;
  const r = el.getBoundingClientRect();
  return { x: r.left + r.width / 2, y: r.top + r.height * yFrac };
};

export function usePresenceMood(inputs: Inputs): PresenceView {
  const ref = useRef(inputs);
  ref.current = inputs;
  const gazes = useRef<Record<Agent, Gaze>>({ Sharee: null, Nexa: null });
  const [moods, setMoods] = useState<Record<Agent, Mood>>({ Sharee: "neutral", Nexa: "neutral" });
  const [view] = useState(() => ({ Sharee: () => gazes.current.Sharee, Nexa: () => gazes.current.Nexa }));

  useEffect(() => {
    let scene: Beat[] = SCENES[3]!;
    let beat = 0;
    let beatEnds = performance.now() + 2500;
    let lastIdx = 3;

    const resolve = (who: Agent, look: Look): Gaze => {
      const i = ref.current;
      const other: Agent = who === "Sharee" ? "Nexa" : "Sharee";
      if (look === "partner") return point(i.agentEl(other));
      if (look === "input") return point(i.inputEl(), 0.5);
      if (look === "away") {
        const me = i.agentEl(who);
        if (!me) return null;
        const r = me.getBoundingClientRect();
        return { x: r.left + r.width * (who === "Sharee" ? -0.6 : 1.6), y: r.top - r.height * 0.4 };
      }
      return null; // the user
    };

    const tick = () => {
      const now = performance.now();
      const i = ref.current;
      let s: { mood: Mood; look: Look };
      let n: { mood: Mood; look: Look };
      const sinceSend = now - i.sentAt;
      if (i.greetAt.Sharee && now - i.greetAt.Sharee < 2600) {
        s = { mood: "friendly", look: "user" };
        n = { mood: "happy", look: "partner" };
      } else if (i.greetAt.Nexa && now - i.greetAt.Nexa < 2600) {
        s = { mood: "friendly", look: "partner" };
        n = { mood: "happy", look: "user" };
      } else if (i.sentAt && sinceSend < 1800) {
        s = { mood: "thinking", look: "input" };
        n = { mood: "thinking", look: "input" };
      } else if (i.sentAt && sinceSend < 4200) {
        s = { mood: "calm", look: "partner" };
        n = { mood: "concerned", look: "partner" };
      } else if (i.draft.trim()) {
        const ms = moodFromText(i.draft, "Sharee");
        const mn = moodFromText(i.draft, "Nexa");
        s = { mood: ms === "neutral" ? "attentive" : ms, look: "input" };
        n = { mood: mn === "neutral" ? "attentive" : mn, look: "input" };
      } else if (i.reduced) {
        s = { mood: "neutral", look: "user" };
        n = { mood: "neutral", look: "user" };
      } else {
        if (now > beatEnds) {
          beat++;
          if (beat >= scene.length) {
            let idx = Math.floor(Math.random() * SCENES.length);
            if (idx === lastIdx) idx = (idx + 1) % SCENES.length;
            lastIdx = idx;
            scene = SCENES[idx]!;
            beat = 0;
          }
          beatEnds = now + scene[beat]!.ms * (0.85 + Math.random() * 0.3);
        }
        s = scene[beat]!.s;
        n = scene[beat]!.n;
      }
      gazes.current = { Sharee: resolve("Sharee", s.look), Nexa: resolve("Nexa", n.look) };
      setMoods((prev) => (prev.Sharee === s.mood && prev.Nexa === n.mood ? prev : { Sharee: s.mood, Nexa: n.mood }));
    };
    const id = window.setInterval(tick, 80);
    return () => window.clearInterval(id);
  }, []);

  return { Sharee: { mood: moods.Sharee, gaze: view.Sharee }, Nexa: { mood: moods.Nexa, gaze: view.Nexa } };
}
