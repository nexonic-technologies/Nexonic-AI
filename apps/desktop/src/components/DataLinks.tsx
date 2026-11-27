import { useEffect, useRef } from "react";

/**
 * Light pulses travelling along the six node lines: "in" pulses flow from a
 * node to the core in the node's colour, "out" pulses flow back in a lighter
 * shade. Hovering a node or bringing the cursor near a line energises it;
 * clicking a node sends a burst. Real activity (a reply being written) speeds
 * every line up. Idle flow is decorative and never reported as activity.
 *
 * Drawn on a canvas over the core so pulses stay round however the SVG of the
 * lines is stretched. Paths are read from the SVG (`path[data-flow]`).
 */

interface Pulse {
  t: number; // 0 = node end, 1 = core end
  dir: 1 | -1; // 1 = in (to the core), -1 = out (to the node)
  speed: number; // path lengths per second
  size: number;
  life: number; // >0 for burst pulses: they fade out instead of looping
}

interface Line {
  id: string;
  color: [number, number, number];
  pts: { x: number; y: number }[]; // sampled path in CSS pixels
  len: number;
  pulses: Pulse[];
  energy: number; // 0..1 smoothed
}

const SAMPLES = 72;

function hexRgb(hex: string): [number, number, number] {
  const n = parseInt(hex.replace("#", ""), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function smooth(a: number, b: number, x: number) {
  const t = Math.min(Math.max((x - a) / (b - a), 0), 1);
  return t * t * (3 - 2 * t);
}

function at(line: Line, t: number) {
  const f = Math.min(Math.max(t, 0), 1) * (line.pts.length - 1);
  const i = Math.floor(f);
  const a = line.pts[i]!;
  const b = line.pts[Math.min(i + 1, line.pts.length - 1)]!;
  const k = f - i;
  return { x: a.x + (b.x - a.x) * k, y: a.y + (b.y - a.y) * k };
}

function seed(): Pulse[] {
  const out: Pulse[] = [];
  for (let i = 0; i < 4; i++) out.push({ t: i / 4 + Math.random() * 0.1, dir: 1, speed: 0.22 + Math.random() * 0.1, size: 1, life: 0 });
  for (let i = 0; i < 2; i++) out.push({ t: i / 2 + 0.25, dir: -1, speed: 0.18 + Math.random() * 0.08, size: 0.8, life: 0 });
  return out;
}

export function DataLinks({
  colors,
  focus,
  burst,
  activity,
  reduced,
  layoutKey,
}: {
  colors: Record<string, string>;
  /** Hovered or selected node. */
  focus: string | null;
  /** Bump `at` to send a burst along `id`'s line. */
  burst: { id: string; at: number } | null;
  /** 0–1, real activity only. */
  activity: number;
  reduced: boolean;
  /** Changes when node positions change, so paths are re-read. */
  layoutKey: string;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const lines = useRef<Map<string, Line>>(new Map());
  const props = useRef({ focus, activity, reduced });
  props.current = { focus, activity, reduced };

  // read and sample the SVG paths (in CSS pixels of the core box)
  useEffect(() => {
    const cv = canvas.current;
    const box = cv?.parentElement;
    const svg = box?.querySelector<SVGSVGElement>("svg.links");
    if (!cv || !box || !svg) return;
    const read = () => {
      const w = box.clientWidth;
      const h = box.clientHeight;
      const vb = svg.viewBox.baseVal;
      const sx = w / (vb.width || 600);
      const sy = h / (vb.height || 400);
      const seen = new Set<string>();
      svg.querySelectorAll<SVGPathElement>("path[data-flow]").forEach((p) => {
        const id = p.dataset.flow!;
        seen.add(id);
        const total = p.getTotalLength();
        const pts = Array.from({ length: SAMPLES }, (_, i) => {
          const q = p.getPointAtLength((total * i) / (SAMPLES - 1));
          return { x: q.x * sx, y: q.y * sy };
        });
        // carry the fibre on into the ring so it dissolves inside the core, not at its edge
        const hub = pts[pts.length - 1]!;
        const cx = w / 2;
        const cy = h / 2;
        for (let i = 1; i <= 14; i++) {
          const k = (i / 14) * 0.42;
          pts.push({ x: hub.x + (cx - hub.x) * k, y: hub.y + (cy - hub.y) * k });
        }
        let len = 0;
        for (let i = 1; i < pts.length; i++) len += Math.hypot(pts[i]!.x - pts[i - 1]!.x, pts[i]!.y - pts[i - 1]!.y);
        const prev = lines.current.get(id);
        lines.current.set(id, {
          id,
          color: hexRgb(colors[id] ?? "#9fb4d8"),
          pts,
          len,
          pulses: prev?.pulses ?? seed(),
          energy: prev?.energy ?? 0,
        });
      });
      for (const id of [...lines.current.keys()]) if (!seen.has(id)) lines.current.delete(id);
    };
    read();
    const ro = new ResizeObserver(read);
    ro.observe(box);
    return () => ro.disconnect();
  }, [colors, layoutKey]);

  // burst: a quick salvo in both directions
  useEffect(() => {
    if (!burst) return;
    const line = lines.current.get(burst.id);
    if (!line) return;
    for (let i = 0; i < 6; i++) {
      line.pulses.push({ t: -i * 0.07, dir: 1, speed: 0.9 + Math.random() * 0.3, size: 1.5, life: 1 });
      line.pulses.push({ t: 1 + i * 0.07, dir: -1, speed: 0.8 + Math.random() * 0.3, size: 1.2, life: 1 });
    }
    line.energy = 1;
  }, [burst]);

  useEffect(() => {
    const cv = canvas.current;
    const box = cv?.parentElement;
    if (!cv || !box) return;
    const ctx = cv.getContext("2d");
    if (!ctx) return;
    let ptr: { x: number; y: number } | null = null;
    const onMove = (e: PointerEvent) => {
      const r = box.getBoundingClientRect();
      ptr = { x: e.clientX - r.left, y: e.clientY - r.top };
    };
    const onLeave = () => { ptr = null; };
    window.addEventListener("pointermove", onMove);
    document.documentElement.addEventListener("pointerleave", onLeave);

    let raf = 0;
    let last = performance.now();
    const frame = (now: number) => {
      raf = requestAnimationFrame(frame);
      if (document.visibilityState !== "visible") return;
      const dt = Math.min((now - last) / 1000, 0.05);
      last = now;
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      const w = box.clientWidth;
      const h = box.clientHeight;
      if (cv.width !== Math.round(w * dpr) || cv.height !== Math.round(h * dpr)) {
        cv.width = Math.round(w * dpr);
        cv.height = Math.round(h * dpr);
      }
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);
      ctx.globalCompositeOperation = "lighter";
      const { focus: f, activity: act, reduced: red } = props.current;

      for (const line of lines.current.values()) {
        if (line.pts.length < 2) continue;
        // cursor near the line energises it
        let near = 0;
        if (ptr) {
          let best = Infinity;
          for (let i = 0; i < line.pts.length; i += 3) {
            const p = line.pts[i]!;
            best = Math.min(best, Math.hypot(p.x - ptr.x, p.y - ptr.y));
          }
          near = Math.max(0, 1 - best / 46);
        }
        const want = Math.max(f === line.id ? 1 : 0, near, act * 0.7);
        line.energy += (want - line.energy) * Math.min(dt * 5, 1);
        const e = line.energy;
        const [r, g, b] = line.color;

        // A fine fibre, brightest at the node and fading to nothing where it
        // enters the ring: one continuous stroke with a gradient along it.
        ctx.lineCap = "round";
        ctx.lineJoin = "round";
        const p0 = line.pts[0]!;
        const p1 = line.pts[line.pts.length - 1]!;
        const fibre = (alpha: number) => {
          const gr = ctx.createLinearGradient(p0.x, p0.y, p1.x, p1.y);
          gr.addColorStop(0, `rgba(${r},${g},${b},${alpha})`);
          gr.addColorStop(0.45, `rgba(${r},${g},${b},${alpha * 0.9})`);
          gr.addColorStop(0.8, `rgba(${r},${g},${b},${alpha * 0.25})`);
          gr.addColorStop(1, `rgba(${r},${g},${b},0)`);
          return gr;
        };
        ctx.beginPath();
        line.pts.forEach((p, i) => (i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y)));
        ctx.strokeStyle = fibre(0.08 + e * 0.12);
        ctx.lineWidth = 5 + e * 4;
        ctx.stroke();
        ctx.strokeStyle = fibre(0.34 + e * 0.36);
        ctx.lineWidth = 1.2 + e * 0.8;
        ctx.stroke();

        if (red) continue;
        const speedK = (1 + e * 2.2 + act * 1.5) * (60 / Math.max(line.len, 60)) * 3.2;
        const keep: Pulse[] = [];
        for (const p of line.pulses) {
          p.t += p.dir * p.speed * speedK * dt;
          if (p.life > 0) {
            // burst pulses travel once, then disappear
            if ((p.dir === 1 && p.t > 1.05) || (p.dir === -1 && p.t < -0.05)) continue;
          } else if (p.dir === 1 && p.t > 1) p.t -= 1;
          else if (p.dir === -1 && p.t < 0) p.t += 1;
          keep.push(p);
          if (p.t < 0 || p.t > 1) continue;
          // comet: bright head with a short tail behind it
          const head = at(line, p.t);
          const tailT = p.t - p.dir * (0.05 + e * 0.04);
          const tail = at(line, tailT);
          // fade in at the node, melt away as it enters the core
          const fade = Math.min(1, p.t * 8) * (1 - smooth(0.55, 1.0, p.t));
          const inbound = p.dir === 1;
          const cr = inbound ? r : Math.round(r + (255 - r) * 0.6);
          const cg = inbound ? g : Math.round(g + (255 - g) * 0.6);
          const cb = inbound ? b : Math.round(b + (255 - b) * 0.6);
          const alpha = (0.55 + e * 0.45) * fade;
          const size = (1.1 + e * 1.1) * p.size;
          const grad = ctx.createLinearGradient(tail.x, tail.y, head.x, head.y);
          grad.addColorStop(0, `rgba(${cr},${cg},${cb},0)`);
          grad.addColorStop(1, `rgba(${cr},${cg},${cb},${alpha})`);
          ctx.strokeStyle = grad;
          ctx.lineWidth = size * 1.6;
          ctx.beginPath();
          ctx.moveTo(tail.x, tail.y);
          ctx.lineTo(head.x, head.y);
          ctx.stroke();
          const glow = ctx.createRadialGradient(head.x, head.y, 0, head.x, head.y, size * 4);
          glow.addColorStop(0, `rgba(255,255,255,${alpha})`);
          glow.addColorStop(0.3, `rgba(${cr},${cg},${cb},${alpha * 0.8})`);
          glow.addColorStop(1, `rgba(${cr},${cg},${cb},0)`);
          ctx.fillStyle = glow;
          ctx.beginPath();
          ctx.arc(head.x, head.y, size * 4, 0, Math.PI * 2);
          ctx.fill();
        }
        line.pulses = keep;

        // a small socket at the node end only; the core end has none
        const pin = line.pts[0]!;
        ctx.fillStyle = `rgba(${r},${g},${b},${0.45 + e * 0.4})`;
        ctx.beginPath();
        ctx.arc(pin.x, pin.y, 1.8 + e * 0.8, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.globalCompositeOperation = "source-over";
    };
    raf = requestAnimationFrame(frame);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener("pointermove", onMove);
      document.documentElement.removeEventListener("pointerleave", onLeave);
    };
  }, []);

  return <canvas ref={canvas} className="data-links" aria-hidden="true" />;
}
