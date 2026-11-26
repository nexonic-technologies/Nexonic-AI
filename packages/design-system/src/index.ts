/** Nexonic AI design tokens for code that can't read CSS variables (WebGL, canvas). */

export type Theme = "dark" | "light";

export const palette = {
  dark: {
    bg: "#0b0f14",
    surface: "#111827",
    elevated: "#1e293b",
    card: "#2a3342",
    sharee: "#00dec6",
    nexa: "#ffb568",
    info: "#3b82f6",
    error: "#ef4444",
    success: "#10b981",
    intelligence: "#a78bfa",
  },
  light: {
    bg: "#f8fafc",
    surface: "#ffffff",
    elevated: "#ffffff",
    card: "#eef2f7",
    sharee: "#0ea5e9",
    nexa: "#f59e0b",
    info: "#3b82f6",
    error: "#ef4444",
    success: "#22c55e",
    intelligence: "#8b5cf6",
  },
} as const satisfies Record<Theme, Record<string, string>>;

export const space = [0, 4, 8, 12, 16, 24, 32, 48, 64] as const;
export const radius = { control: 8, card: 12, panel: 16, hero: 24 } as const;

/** Motion levels the user can pick in Settings. */
export type MotionLevel = "off" | "minimal" | "balanced" | "cinematic";

export const motion = {
  duration: { fast: 140, base: 240, slow: 480, scene: 900 },
  /** Particle budget per level; the renderer lowers it further on weak GPUs. */
  particles: { off: 0, minimal: 1200, balanced: 4200, cinematic: 9000 } satisfies Record<MotionLevel, number>,
} as const;

/** Interaction states from the design system. Each must come from a real event. */
export type PresenceState =
  | "idle"
  | "listening"
  | "thinking"
  | "speaking"
  | "executing"
  | "learning"
  | "collaborating"
  | "completed"
  | "attention"
  | "error"
  | "offline";

export const presenceLabel: Record<PresenceState, string> = {
  idle: "Idle",
  listening: "Listening",
  thinking: "Thinking",
  speaking: "Speaking",
  executing: "Executing",
  learning: "Learning",
  collaborating: "Collaborating",
  completed: "Completed",
  attention: "Needs you",
  error: "Error",
  offline: "Offline",
};
