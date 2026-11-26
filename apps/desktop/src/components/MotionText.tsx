import { useEffect, useRef, useState, type CSSProperties } from "react";

/** Motion typography. Every effect is skipped when the user prefers reduced motion. */

const reducedMotion = () => typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;

/** Characters rise out of a soft blur, one after another. */
export function SplitText({
  text,
  delay = 0,
  stagger = 24,
  className,
  as: Tag = "span",
}: {
  text: string;
  delay?: number;
  stagger?: number;
  className?: string;
  as?: "span" | "h1" | "h2" | "h3" | "strong" | "b";
}) {
  return (
    <Tag className={`split ${className ?? ""}`} aria-label={text}>
      {Array.from(text).map((ch, i) => (
        <span
          key={i}
          aria-hidden="true"
          className={ch === " " ? "split-ch split-sp" : "split-ch"}
          style={{ "--i": i, "--d": `${delay}ms`, "--s": `${stagger}ms` } as CSSProperties}
        >
          {ch === " " ? " " : ch}
        </span>
      ))}
    </Tag>
  );
}

const GLYPHS = "ABCDEFGHJKLMNPQRSTUVWXYZ0123456789#%&*+<>/\\";

/** Decodes from random glyphs into the text (HUD style). Re-runs when the text changes. */
export function ScrambleText({ text, duration = 650, delay = 0, className }: { text: string; duration?: number; delay?: number; className?: string }) {
  const [shown, setShown] = useState(text);
  useEffect(() => {
    if (reducedMotion()) {
      setShown(text);
      return;
    }
    let raf = 0;
    let start = 0;
    const tick = (now: number) => {
      if (!start) start = now + delay;
      const p = Math.min(Math.max((now - start) / duration, 0), 1);
      const fixed = Math.floor(p * text.length);
      let s = "";
      for (let i = 0; i < text.length; i++) {
        const c = text[i]!;
        s += i < fixed || c === " " || c === "·" ? c : GLYPHS[(Math.random() * GLYPHS.length) | 0];
      }
      setShown(s);
      if (p < 1) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [text, duration, delay]);
  return (
    <span className={`scramble ${className ?? ""}`} aria-label={text}>
      <span aria-hidden="true">{shown}</span>
    </span>
  );
}

/** Vertical roll between values (statuses, labels). */
export function RollText({ value, className }: { value: string; className?: string }) {
  const [items, setItems] = useState<{ key: number; text: string; leaving: boolean }[]>([{ key: 0, text: value, leaving: false }]);
  const k = useRef(0);
  useEffect(() => {
    setItems((cur) => {
      if (cur[cur.length - 1]?.text === value) return cur;
      k.current += 1;
      return [...cur.map((c) => ({ ...c, leaving: true })), { key: k.current, text: value, leaving: false }];
    });
    const t = setTimeout(() => setItems((cur) => cur.filter((c) => !c.leaving)), 420);
    return () => clearTimeout(t);
  }, [value]);
  return (
    <span className={`roll ${className ?? ""}`} aria-live="polite">
      {items.map((it) => (
        <span key={it.key} className={it.leaving ? "roll-out" : "roll-in"} aria-hidden={it.leaving}>
          {it.text}
        </span>
      ))}
    </span>
  );
}

/** Counts to a number with tabular figures. */
export function CountUp({ value, duration = 700 }: { value: number; duration?: number }) {
  const [n, setN] = useState(value);
  const from = useRef(value);
  useEffect(() => {
    if (reducedMotion()) {
      setN(value);
      from.current = value;
      return;
    }
    const a = from.current;
    let raf = 0;
    const t0 = performance.now();
    const tick = (now: number) => {
      const p = Math.min((now - t0) / duration, 1);
      const e = 1 - Math.pow(1 - p, 3);
      setN(Math.round(a + (value - a) * e));
      if (p < 1) raf = requestAnimationFrame(tick);
      else from.current = value;
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [value, duration]);
  return <span className="num">{n}</span>;
}

/** Cycles hint phrases with a roll transition (used for empty inputs). */
export function useCycle(items: string[], every = 3600): string {
  const [i, setI] = useState(0);
  useEffect(() => {
    if (reducedMotion() || items.length < 2) return;
    const id = setInterval(() => setI((x) => (x + 1) % items.length), every);
    return () => clearInterval(id);
  }, [items.length, every]);
  return items[i] ?? "";
}
