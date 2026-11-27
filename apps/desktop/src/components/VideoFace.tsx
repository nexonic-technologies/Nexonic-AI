import { useEffect, useRef, useState } from "react";

/**
 * Real video presence for Sharee and Nexa.
 *
 * Plays real filmed-style clips (made from each character's own portrait)
 * with a transparent background. Every clip is packed side by side: colour on
 * the left, alpha matte on the right (see services/presence/process_clip.py),
 * and composited here in WebGL. Every clip starts and ends on the same
 * front-facing frame, so switching between them is seamless; a short
 * cross-fade hides any remaining difference.
 *
 * `idle` loops. Other clips play once when asked and then return to idle.
 */

/** idle loops; chat = turning to the partner and talking; the rest are front-facing one-shots. */
export type ClipName = "idle" | "chat" | "look" | "listen" | "talk" | "smile";
export type ClipSet = Partial<Record<ClipName, string>>;
export type FaceBox = { cx: number; cy: number; w: number };

const FADE_MS = 320;
const FACE_X = 0.5;
const FACE_Y = 0.36;
const FACE_W = 0.4;

// box = x, y, w, h of the picture in canvas units (0–1, origin top-left)
const VERT = `attribute vec2 p; varying vec2 uv; varying vec2 sp; uniform vec4 box;
void main(){ uv = p; vec2 q = box.xy + p * box.zw; sp = q; gl_Position = vec4(q.x * 2.0 - 1.0, 1.0 - q.y * 2.0, 0.0, 1.0); }`;
// sp = position on the canvas (0–1, origin top-left). The fade is measured on the
// canvas, not on the video frame, so the body always melts into the stage, however
// the clip is scaled to line the faces up: no straight cut at the bottom or sides.
const FRAG = `precision mediump float; varying vec2 uv; varying vec2 sp; uniform sampler2D tex; uniform float alpha;
void main(){
  vec3 c = texture2D(tex, vec2(uv.x * 0.5, uv.y)).rgb;
  float a = texture2D(tex, vec2(0.5 + uv.x * 0.5, uv.y)).r;
  // never show the clip's own frame edges
  float frame = smoothstep(0.0, 0.05, uv.x) * smoothstep(1.0, 0.95, uv.x) * smoothstep(1.0, 0.93, uv.y);
  // soft oval around head and shoulders, long fade down the chest
  vec2 d = (sp - vec2(0.5, 0.4)) / vec2(0.56, 0.66);
  float oval = 1.0 - smoothstep(0.72, 1.0, length(d));
  float low = smoothstep(1.0, 0.58, sp.y);
  low = low * low * (3.0 - 2.0 * low);
  float sides = smoothstep(0.0, 0.14, sp.x) * smoothstep(1.0, 0.86, sp.x);
  float fade = oval * low * sides;
  // light falls off where the body fades, like a figure lit in a dark room
  c *= mix(0.55, 1.0, smoothstep(0.0, 0.8, fade));
  a *= frame * fade * alpha;
  gl_FragColor = vec4(c * a, a);
}`;

export function VideoFace({
  clips,
  face,
  fallback,
  want,
  onDone,
  reduced,
  label,
}: {
  clips: ClipSet;
  face?: FaceBox;
  fallback?: string;
  /** Clip the behaviour engine wants now; one-shot clips play to the end once. */
  want: ClipName;
  onDone?: (clip: ClipName) => void;
  reduced: boolean;
  label: string;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [webglFailed, setWebglFailed] = useState(false);
  const wantRef = useRef(want);
  wantRef.current = want;
  const doneRef = useRef(onDone);
  doneRef.current = onDone;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !clips.idle) return;
    const gl = canvas.getContext("webgl", { premultipliedAlpha: true, alpha: true });
    if (!gl) {
      setWebglFailed(true);
      return;
    }

    const mk = (type: number, src: string) => {
      const s = gl.createShader(type)!;
      gl.shaderSource(s, src);
      gl.compileShader(s);
      return s;
    };
    const vs = mk(gl.VERTEX_SHADER, VERT);
    const fs = mk(gl.FRAGMENT_SHADER, FRAG);
    const prog = gl.createProgram()!;
    gl.attachShader(prog, vs);
    gl.attachShader(prog, fs);
    gl.linkProgram(prog);
    gl.useProgram(prog);
    const buf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, 0, 1, 0, 0, 1, 1, 1]), gl.STATIC_DRAW);
    const loc = gl.getAttribLocation(prog, "p");
    gl.enableVertexAttribArray(loc);
    gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);
    const uBox = gl.getUniformLocation(prog, "box");
    const uAlpha = gl.getUniformLocation(prog, "alpha");
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);

    // one <video> and one texture per clip
    const players = new Map<ClipName, { v: HTMLVideoElement; t: WebGLTexture }>();
    for (const [name, url] of Object.entries(clips) as [ClipName, string][]) {
      const v = document.createElement("video");
      v.src = url;
      v.muted = true;
      v.playsInline = true;
      v.preload = "auto";
      v.loop = name === "idle";
      const t = gl.createTexture()!;
      gl.bindTexture(gl.TEXTURE_2D, t);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      players.set(name, { v, t });
    }

    let current: ClipName = "idle";
    let previous: ClipName | null = null;
    let fadeStart = 0;
    let lastWant: ClipName = "idle";
    const idle = players.get("idle")!;
    if (reduced) {
      // Seek slightly into the clip so a resting frame decodes without looping playback.
      idle.v.addEventListener("loadedmetadata", () => { idle.v.currentTime = 0.04; }, { once: true });
    } else {
      void idle.v.play().catch(() => undefined);
    }

    const start = (name: ClipName) => {
      const p = players.get(name);
      if (!p || name === current) return;
      previous = current;
      current = name;
      fadeStart = performance.now();
      p.v.currentTime = 0;
      void p.v.play().catch(() => undefined);
      if (name !== "idle") {
        p.v.onended = () => {
          doneRef.current?.(name);
          if (current === name) start("idle");
        };
      }
    };

    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    let fitRaf = 0;
    const syncSize = () => {
      const r = canvas.getBoundingClientRect();
      const nw = Math.max(1, Math.round(r.width * dpr));
      const nh = Math.max(1, Math.round(r.height * dpr));
      if (canvas.width !== nw || canvas.height !== nh) {
        canvas.width = nw;
        canvas.height = nh;
      }
    };
    const scheduleFit = () => {
      if (fitRaf) return;
      fitRaf = requestAnimationFrame(() => { fitRaf = 0; syncSize(); });
    };
    const ro = new ResizeObserver(scheduleFit);
    ro.observe(canvas);
    syncSize();

    const draw = (p: { v: HTMLVideoElement; t: WebGLTexture }, a: number) => {
      const v = p.v;
      if (v.readyState < 2 || !v.videoWidth) return;
      gl.bindTexture(gl.TEXTURE_2D, p.t);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGB, gl.RGB, gl.UNSIGNED_BYTE, v);
      const iw = v.videoWidth / 2;
      const ih = v.videoHeight;
      const cw = canvas.width;
      const ch = canvas.height;
      let w: number;
      let h: number;
      let x: number;
      let y: number;
      if (face) {
        // Keep the head consistent horizontally while scaling down on compact screens
        // so the shoulders and chin always stay cleanly above the nameplate.
        w = Math.min((FACE_W * cw) / face.w, (0.52 * ch) / face.w);
        h = (w * ih) / iw;
        const maxH = ch * 1.12;
        if (h > maxH) {
          h = maxH;
          w = (h * iw) / ih;
        }
        x = cw * FACE_X - face.cx * w;
        y = Math.min(cw * 0 + ch * FACE_Y - face.cy * h, ch - h * 0.92);
        y = Math.max(y, -h * 0.04);
      } else {
        const s = Math.min(cw / iw, ch / ih);
        w = iw * s;
        h = ih * s;
        x = (cw - w) / 2;
        y = ch - h;
      }
      gl.uniform4f(uBox, x / cw, y / ch, w / cw, h / ch);
      gl.uniform1f(uAlpha, a);
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    };

    let raf = 0;
    const frame = (now: number) => {
      raf = requestAnimationFrame(frame);
      if (document.visibilityState !== "visible") return;
      syncSize();
      const w = wantRef.current;
      if (w !== lastWant) {
        lastWant = w;
        if (players.has(w)) start(w);
      }
      // While a clip rebuffers, keep the last frame on screen instead of a blank.
      const cur0 = players.get(current)!.v;
      if (cur0.readyState < 2 || !cur0.videoWidth) return;
      gl.viewport(0, 0, canvas.width, canvas.height);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
      const t = Math.min(1, (now - fadeStart) / FADE_MS);
      if (previous && t < 1) {
        const pp = players.get(previous);
        if (pp) draw(pp, 1);
      } else if (previous && t >= 1) {
        const pp = players.get(previous);
        if (pp && previous !== "idle") pp.v.pause();
        previous = null;
      }
      const cur = players.get(current)!;
      draw(cur, previous ? t * t * (3 - 2 * t) : 1);
    };
    raf = requestAnimationFrame(frame);

    return () => {
      cancelAnimationFrame(raf);
      if (fitRaf) cancelAnimationFrame(fitRaf);
      ro.disconnect();
      for (const { v, t } of players.values()) {
        v.pause();
        v.removeAttribute("src");
        v.load();
        gl.deleteTexture(t);
      }
      gl.deleteBuffer(buf);
      gl.deleteProgram(prog);
      gl.deleteShader(vs);
      gl.deleteShader(fs);
    };
  }, [clips, face, reduced]);

  if (webglFailed && fallback) return <img className="portrait" src={fallback} alt={label} draggable={false} />;
  return <canvas ref={canvasRef} className="portrait live" role="img" aria-label={label} />;
}
