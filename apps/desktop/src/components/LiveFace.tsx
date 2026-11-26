import { useEffect, useRef } from "react";

/**
 * Live portrait player.
 *
 * Plays the LivePortrait frame atlas rendered from the character's own
 * portrait on Apple Silicon (`services/presence/render_atlas.py`): the head
 * turns and the eyes follow the gaze target, the lids blink on the persona's
 * rhythm, and moods cross-fade in 150–400 ms as the datasheets specify.
 * Breathing is a sub-pixel idle drift. Nothing here claims the agent is
 * thinking or speaking; moods come from real events (see `presenceMood.ts`).
 */

export type Mood =
  | "neutral" | "friendly" | "happy" | "attentive" | "curious"
  | "thinking" | "concerned" | "confident" | "calm" | "determined";

export type Gaze = { x: number; y: number } | null; // client coords; null = look at the viewer

export type FaceAtlas = {
  yaws: number[];
  pitches: number[];
  exprYaws: number[];
  frames: Record<string, string>; // key -> url
};

export type Temperament = {
  blinkMin: number; // ms between blinks
  blinkMax: number;
  doubleBlink: number; // probability
  gazeGain: number; // how far the head follows (0–1.3)
  saccade: number; // idle eye wander, degrees
};

const BLINK_MS = 150; // open → half → closed → half → open
const MOOD_FADE = 300;
const POSE_FADE = 140;
// Sign of the rendered grid relative to screen space (verified against the atlas).
const YAW_SIGN = -1; // positive rendered yaw turns toward screen-left
const PITCH_SIGN = 1;

function nearest(values: number[], v: number) {
  let best = values[0] ?? 0;
  for (const x of values) if (Math.abs(x - v) < Math.abs(best - v)) best = x;
  return best;
}

export function LiveFace({
  atlas,
  fallback,
  mood,
  gaze,
  temperament,
  reduced,
  label,
}: {
  atlas: FaceAtlas | null;
  fallback: string;
  mood: Mood;
  gaze: () => Gaze;
  temperament: Temperament;
  reduced: boolean;
  label: string;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const moodRef = useRef(mood);
  const gazeRef = useRef(gaze);
  moodRef.current = mood;
  gazeRef.current = gaze;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !atlas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    // decode every frame up front so switching never stalls
    const images = new Map<string, HTMLImageElement>();
    let ready = false;
    let cancelled = false;
    void Promise.all(
      Object.entries(atlas.frames).map(([key, url]) => {
        const img = new Image();
        img.decoding = "async";
        img.src = url;
        images.set(key, img);
        return img.decode().catch(() => undefined);
      }),
    ).then(() => { if (!cancelled) ready = true; });

    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const fit = () => {
      const r = canvas.getBoundingClientRect();
      canvas.width = Math.max(1, Math.round(r.width * dpr));
      canvas.height = Math.max(1, Math.round(r.height * dpr));
    };
    const ro = new ResizeObserver(fit);
    ro.observe(canvas);
    fit();

    let shown = "";
    let prev = "";
    let fadeStart = 0;
    let fadeMs = POSE_FADE;
    let yawCur = 0;
    let pitchCur = 0;
    let nextBlink = performance.now() + 1200 + Math.random() * 2000;
    let blinkUntil = 0;
    let wander = { x: 0, y: 0, until: 0 };
    let lastMood: Mood = moodRef.current;
    let raf = 0;
    let rect = canvas.getBoundingClientRect();
    let drawBox = { x: 0, y: 0, w: 0, h: 0 };

    const scheduleBlink = (now: number) => {
      const { blinkMin, blinkMax, doubleBlink } = temperament;
      nextBlink = now + blinkMin + Math.random() * (blinkMax - blinkMin);
      if (Math.random() < doubleBlink) nextBlink = now + BLINK_MS * 2.6;
    };

    const keyFor = (m: Mood, yaw: number, pitch: number, eye: "o" | "h" | "c") => {
      if (m === "neutral") {
        const k = `g_${nearest(atlas.yaws, yaw)}_${nearest(atlas.pitches, pitch)}_${eye}`;
        return images.has(k) ? k : `g_0_0_${eye}`;
      }
      const e = eye === "h" ? "o" : eye; // expressions have open and closed lids only
      const k = `e_${m}_${nearest(atlas.exprYaws, yaw)}_${e}`;
      return images.has(k) ? k : `g_${nearest(atlas.yaws, yaw)}_0_${e}`;
    };

    const frame = (now: number) => {
      raf = requestAnimationFrame(frame);
      if (!ready || document.visibilityState !== "visible") return;
      rect = canvas.getBoundingClientRect();

      // where to look: target point -> head yaw/pitch in degrees
      const g = gazeRef.current();
      let wantYaw = 0;
      let wantPitch = 0;
      if (g && drawBox.w > 0) {
        const fx = rect.left + (drawBox.x + drawBox.w * 0.5) / dpr;
        const fy = rect.top + (drawBox.y + drawBox.h * 0.3) / dpr;
        const depth = 620;
        wantYaw = (Math.atan2(g.x - fx, depth) * 180) / Math.PI * temperament.gazeGain * YAW_SIGN;
        wantPitch = (Math.atan2(g.y - fy, depth) * 180) / Math.PI * temperament.gazeGain * PITCH_SIGN;
      }
      if (!reduced) {
        if (now > wander.until) {
          const s = temperament.saccade;
          wander = { x: (Math.random() - 0.5) * 2 * s, y: (Math.random() - 0.5) * s, until: now + 900 + Math.random() * 2600 };
        }
        wantYaw += wander.x;
        wantPitch += wander.y;
      }
      const maxYaw = Math.max(...atlas.yaws.map(Math.abs));
      const maxPitch = Math.max(...atlas.pitches.map(Math.abs));
      wantYaw = Math.max(-maxYaw, Math.min(maxYaw, wantYaw));
      wantPitch = Math.max(-maxPitch, Math.min(maxPitch, wantPitch));
      // eased head: the eyes lead, the head settles
      yawCur += (wantYaw - yawCur) * 0.12;
      pitchCur += (wantPitch - pitchCur) * 0.12;

      if (!reduced && now > nextBlink) {
        blinkUntil = now + BLINK_MS;
        scheduleBlink(now);
      }
      const into = BLINK_MS - (blinkUntil - now); // ms since the blink started
      const eye: "o" | "h" | "c" = now >= blinkUntil ? "o" : into < 35 || into > 115 ? "h" : "c";
      const closed = eye !== "o";
      const m = moodRef.current;
      const key = keyFor(m, yawCur, pitchCur, eye);
      if (key !== shown) {
        prev = shown;
        shown = key;
        fadeStart = now;
        const eyeChanged = key.slice(-1) !== prev.slice(-1);
        fadeMs = m !== lastMood ? MOOD_FADE : eyeChanged || closed ? 28 : POSE_FADE;
        lastMood = m;
      }

      const cur = images.get(shown);
      if (!cur || !cur.naturalWidth) return;
      // contain-fit, anchored to the bottom like the old portrait
      const cw = canvas.width;
      const ch = canvas.height;
      const sc = Math.min(cw / cur.naturalWidth, ch / cur.naturalHeight);
      const w = cur.naturalWidth * sc;
      const h = cur.naturalHeight * sc;
      drawBox = { x: (cw - w) / 2, y: ch - h, w, h };
      const breath = reduced ? 0 : Math.sin((now / 4300) * Math.PI * 2);
      const bs = 1 + breath * 0.004;
      ctx.clearRect(0, 0, cw, ch);
      ctx.save();
      ctx.translate(cw / 2, ch);
      ctx.scale(bs, bs);
      ctx.translate(-cw / 2, -ch - breath * 1.2 * dpr);
      const t = Math.min(1, (now - fadeStart) / fadeMs);
      const before = images.get(prev);
      if (t < 1 && before?.naturalWidth) {
        ctx.globalAlpha = 1;
        ctx.drawImage(before, drawBox.x, drawBox.y, w, h);
        ctx.globalAlpha = t * t * (3 - 2 * t);
      }
      ctx.drawImage(cur, drawBox.x, drawBox.y, w, h);
      ctx.restore();
      ctx.globalAlpha = 1;
    };
    raf = requestAnimationFrame(frame);
    return () => {
      cancelled = true;
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [atlas, reduced, temperament]);

  if (!atlas) return <img className="portrait" src={fallback} alt={label} draggable={false} />;
  return <canvas ref={canvasRef} className="portrait live" role="img" aria-label={label} />;
}
