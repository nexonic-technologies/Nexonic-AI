import { useEffect, useRef } from "react";

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
const VERT = `attribute vec2 p; varying vec2 uv; uniform vec4 box;
void main(){ uv = p; vec2 q = box.xy + p * box.zw; gl_Position = vec4(q.x * 2.0 - 1.0, 1.0 - q.y * 2.0, 0.0, 1.0); }`;
const FRAG = `precision mediump float; varying vec2 uv; uniform sampler2D tex; uniform float alpha;
void main(){
  vec3 c = texture2D(tex, vec2(uv.x * 0.5, uv.y)).rgb;
  float a = texture2D(tex, vec2(0.5 + uv.x * 0.5, uv.y)).r;
  // dissolve the frame edges and the lower torso into the stage
  float edge = smoothstep(0.0, 0.1, uv.x) * smoothstep(1.0, 0.9, uv.x) * smoothstep(1.0, 0.8, uv.y);
  a *= edge * alpha;
  gl_FragColor = vec4(c * a, a);
}`;

export function VideoFace({
  clips,
  face,
  want,
  onDone,
  reduced,
  label,
}: {
  clips: ClipSet;
  face?: FaceBox;
  /** Clip the behaviour engine wants now; one-shot clips play to the end once. */
  want: ClipName;
  onDone?: (clip: ClipName) => void;
  reduced: boolean;
  label: string;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wantRef = useRef(want);
  wantRef.current = want;
  const doneRef = useRef(onDone);
  doneRef.current = onDone;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !clips.idle) return;
    const gl = canvas.getContext("webgl", { premultipliedAlpha: true, alpha: true });
    if (!gl) return;

    const mk = (type: number, src: string) => {
      const s = gl.createShader(type)!;
      gl.shaderSource(s, src);
      gl.compileShader(s);
      return s;
    };
    const prog = gl.createProgram()!;
    gl.attachShader(prog, mk(gl.VERTEX_SHADER, VERT));
    gl.attachShader(prog, mk(gl.FRAGMENT_SHADER, FRAG));
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
    if (!reduced) void idle.v.play().catch(() => undefined);

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
    const fit = () => {
      const r = canvas.getBoundingClientRect();
      canvas.width = Math.max(1, Math.round(r.width * dpr));
      canvas.height = Math.max(1, Math.round(r.height * dpr));
    };
    const ro = new ResizeObserver(fit);
    ro.observe(canvas);
    fit();

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
        w = (FACE_W * cw) / face.w;
        h = (w * ih) / iw;
        x = cw * FACE_X - face.cx * w;
        y = ch * FACE_Y - face.cy * h;
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
      ro.disconnect();
      for (const { v } of players.values()) {
        v.pause();
        v.removeAttribute("src");
        v.load();
      }
    };
  }, [clips, face, reduced]);

  return <canvas ref={canvasRef} className="portrait live" role="img" aria-label={label} />;
}
