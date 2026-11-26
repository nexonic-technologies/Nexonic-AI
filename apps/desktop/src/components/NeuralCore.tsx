import { useEffect, useRef } from "react";
import * as THREE from "three";
import type { MotionLevel } from "@nexonic/design-system";

/**
 * Collective Intelligence core.
 *
 * A slowly tumbling torus woven from thousands of streamlines. Energy pulses
 * flow along every filament. Two bands orbit the rim in opposite phase —
 * Sharee's cyan and Nexa's amber — and push out radial spikes where they pass.
 * The torus leans toward the cursor; hovering a cognitive node steers the
 * bands to that node's sector.
 *
 * Cursor interaction (direct response to the user's pointer, never reported
 * as activity): filaments bend toward the cursor like a magnetic field, rim
 * needles turn to point at it, sparks swirl around it, and a click sends a
 * shockwave through the whole weave. Dragging anywhere on the stage spins the
 * torus in 3D with momentum while Sharee's band follows the hand; on release
 * it glides and settles back to its resting pose.
 *
 * `activity` (0–1) must come from real events. At 0 only the slow idle drift
 * runs, which is decorative.
 */

export const NODE_ANGLES: Record<string, number> = {
  Reasoning: 2.35, Perception: 0.8, Memory: 2.95, Planning: 0.2, Learning: 3.75, Action: -0.6,
  Sharee: Math.PI, Nexa: 0,
};

const BUDGET: Record<MotionLevel, { lines: number; seg: number; spikes: number; dust: number; nucleus: number }> = {
  off: { lines: 0, seg: 0, spikes: 0, dust: 0, nucleus: 0 },
  minimal: { lines: 260, seg: 48, spikes: 500, dust: 600, nucleus: 1400 },
  balanced: { lines: 620, seg: 64, spikes: 1100, dust: 1500, nucleus: 3200 },
  cinematic: { lines: 1100, seg: 80, spikes: 1800, dust: 2600, nucleus: 5200 },
};

const R = 1.72; // major radius
const r = 0.56; // tube radius

export function NeuralCore({
  activity = 0,
  motion = "balanced",
  reduced = false,
  focus = null,
}: {
  activity?: number;
  motion?: MotionLevel;
  reduced?: boolean;
  focus?: string | null;
}) {
  const host = useRef<HTMLDivElement>(null);
  const activityRef = useRef(activity);
  const focusRef = useRef<string | null>(focus);
  activityRef.current = activity;
  focusRef.current = focus;

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const level: MotionLevel = reduced && motion !== "off" ? "minimal" : motion;
    const B = BUDGET[level];
    if (B.lines === 0) return;

    let renderer: THREE.WebGLRenderer;
    try {
      renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, powerPreference: "high-performance" });
    } catch {
      el.dataset.fallback = "true";
      return;
    }
    const dpr = Math.min(window.devicePixelRatio, 2);
    renderer.setPixelRatio(dpr);
    renderer.setClearColor(0x000000, 0);
    el.appendChild(renderer.domElement);

    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(34, 1, 0.1, 50);
    camera.position.set(0, 0, 7.4);
    const rig = new THREE.Group(); // tilt + tumble
    scene.add(rig);

    const shared = {
      uTime: { value: 0 },
      uActivity: { value: 0 },
      uBand: { value: 0 },
      uSharee: { value: new THREE.Color("#00dec6") },
      uNexa: { value: new THREE.Color("#ffb568") },
      uBase: { value: new THREE.Color("#a9c6ff") },
      uPixel: { value: dpr },
      uLight: { value: 0 },
      uClock: { value: 0 },
      uPtr: { value: new THREE.Vector3(9, 9, 0) },
      uPtrK: { value: 0 },
      uPtrVel: { value: new THREE.Vector2(0, 0) },
      uRip: { value: [0, 1, 2, 3].map(() => new THREE.Vector4(0, 0, -99, 0)) },
    };

    const bandGLSL = /* glsl */ `
      uniform float uTime, uActivity, uBand, uLight;
      uniform vec3 uSharee, uNexa, uBase;
      float wrapd(float a, float b){ float d = mod(a - b + 3.14159265, 6.2831853) - 3.14159265; return abs(d); }
      vec2 bands(float theta){
        float w = 0.55 + uActivity * 0.25;
        float s = exp(-pow(wrapd(theta, uBand) / w, 2.0));
        float n = exp(-pow(wrapd(theta, uBand + 3.14159265) / w, 2.0));
        return vec2(s, n);
      }
      uniform float uClock, uPtrK;
      uniform vec3 uPtr;
      uniform vec2 uPtrVel;
      uniform vec4 uRip[4];
      // Pointer field in rig space: magnetic pull + swirl + wake, plus click shockwaves.
      vec3 field(vec3 p, out float glow){
        vec3 d = uPtr - p;
        float dist = length(d);
        float f = uPtrK * exp(-dist * dist / 0.9);
        vec3 dir = d / max(dist, 1e-3);
        vec3 disp = dir * f * 0.22 * smoothstep(0.0, 0.6, dist);
        disp += vec3(-d.y, d.x, 0.0) * f * 0.16;
        disp += vec3(uPtrVel, 0.0) * f * 0.35;
        glow = f * 0.7;
        for (int i = 0; i < 4; i++) {
          float age = uClock - uRip[i].z;
          if (age > 0.0 && age < 2.4) {
            vec2 rv = p.xy - uRip[i].xy;
            float rd = length(rv);
            float w = exp(-pow((rd - age * 2.4) / 0.16, 2.0)) * pow(1.0 - age / 2.4, 1.5) * uRip[i].w;
            disp += vec3(rv / max(rd, 1e-3), 0.35) * w * 0.22;
            glow += w * 0.5;
          }
        }
        return disp;
      }
      vec3 tint(vec2 b){ vec3 c = uBase * 0.85; c = mix(c, uSharee * 1.3, clamp(b.x * 1.25, 0.0, 1.0)); c = mix(c, uNexa * 1.3, clamp(b.y * 1.25, 0.0, 1.0)); return c; }
    `;

    // ---------- Streamlines ----------
    const segs = B.seg;
    const lineCount = B.lines;
    const vCount = lineCount * segs * 2;
    const pos = new Float32Array(vCount * 3);
    const aTheta = new Float32Array(vCount);
    const aU = new Float32Array(vCount);
    const aSeed = new Float32Array(vCount);
    let vi = 0;
    const p3 = (theta: number, phi: number, rr: number): [number, number, number] => [
      (R + rr * Math.cos(phi)) * Math.cos(theta),
      (R + rr * Math.cos(phi)) * Math.sin(theta),
      rr * Math.sin(phi),
    ];
    for (let l = 0; l < lineCount; l++) {
      const t0 = Math.random() * Math.PI * 2;
      const span = 0.9 + Math.random() * 2.6;
      const wind = (Math.random() < 0.5 ? -1 : 1) * (0.6 + Math.random() * 2.4);
      const p0 = Math.random() * Math.PI * 2;
      const rr = r * (0.55 + Math.random() * 0.5);
      const seed = Math.random();
      let prev = p3(t0, p0, rr);
      for (let s = 1; s <= segs; s++) {
        const u = s / segs;
        const th = t0 + span * u;
        const ph = p0 + span * wind * u + Math.sin(u * 9 + seed * 6) * 0.15;
        const cur = p3(th, ph, rr * (1 + Math.sin(u * 5 + seed * 3) * 0.08));
        const pair: [[number, number, number], number][] = [[prev, u - 1 / segs], [cur, u]];
        for (const [pt, uu] of pair) {
          pos.set(pt, vi * 3);
          aTheta[vi] = Math.atan2(pt[1], pt[0]);
          aU[vi] = uu;
          aSeed[vi] = seed;
          vi++;
        }
        prev = cur;
      }
    }
    const lineGeo = new THREE.BufferGeometry();
    lineGeo.setAttribute("position", new THREE.BufferAttribute(pos, 3));
    lineGeo.setAttribute("aTheta", new THREE.BufferAttribute(aTheta, 1));
    lineGeo.setAttribute("aU", new THREE.BufferAttribute(aU, 1));
    lineGeo.setAttribute("aSeed", new THREE.BufferAttribute(aSeed, 1));
    const lineMat = new THREE.ShaderMaterial({
      uniforms: shared,
      transparent: true,
      depthWrite: false,
      blending: THREE.AdditiveBlending,
      vertexShader: /* glsl */ `
        ${bandGLSL}
        attribute float aTheta, aU, aSeed;
        varying vec3 vCol; varying float vA;
        void main(){
          vec2 b = bands(aTheta);
          float speed = 0.18 + aSeed * 0.25 + uActivity * 0.9;
          float pulse = pow(0.5 + 0.5 * sin((aU * 6.0 - uTime * speed * 6.0 + aSeed * 40.0)), 8.0);
          float g;
          vec3 p = position + field(position, g);
          vCol = tint(b) + vec3(0.35, 0.45, 0.6) * g;
          float edge = smoothstep(0.0, 0.08, aU) * smoothstep(1.0, 0.92, aU);
          vA = edge * (0.07 + 0.22 * pulse + 0.32 * (b.x + b.y) + 0.55 * g) * (1.0 + uActivity * 0.8) * mix(1.0, 1.6, uLight);
          gl_Position = projectionMatrix * modelViewMatrix * vec4(p, 1.0);
        }`,
      fragmentShader: /* glsl */ `
        uniform float uLight;
        varying vec3 vCol; varying float vA;
        void main(){ gl_FragColor = vec4(mix(vCol, vCol * 0.55, uLight), vA); }`,
    });
    rig.add(new THREE.LineSegments(lineGeo, lineMat));

    // ---------- Radial spikes on the outer rim ----------
    const sCount = B.spikes;
    const sPos = new Float32Array(sCount * 2 * 3);
    const sTheta = new Float32Array(sCount * 2);
    const sTip = new Float32Array(sCount * 2);
    const sLen = new Float32Array(sCount * 2);
    for (let i = 0; i < sCount; i++) {
      const th = Math.random() * Math.PI * 2;
      const ph = (Math.random() - 0.5) * 1.3;
      const base = p3(th, ph, r * 0.98);
      const len = 0.05 + Math.random() ** 2 * 0.55;
      for (let k = 0; k < 2; k++) {
        sPos.set(base, (i * 2 + k) * 3);
        sTheta[i * 2 + k] = th;
        sTip[i * 2 + k] = k;
        sLen[i * 2 + k] = len;
      }
    }
    const spikeGeo = new THREE.BufferGeometry();
    spikeGeo.setAttribute("position", new THREE.BufferAttribute(sPos, 3));
    spikeGeo.setAttribute("aTheta", new THREE.BufferAttribute(sTheta, 1));
    spikeGeo.setAttribute("aTip", new THREE.BufferAttribute(sTip, 1));
    spikeGeo.setAttribute("aLen", new THREE.BufferAttribute(sLen, 1));
    const spikeMat = new THREE.ShaderMaterial({
      uniforms: shared,
      transparent: true,
      depthWrite: false,
      blending: THREE.AdditiveBlending,
      vertexShader: /* glsl */ `
        ${bandGLSL}
        attribute float aTheta, aTip, aLen;
        varying vec3 vCol; varying float vA;
        void main(){
          vec2 b = bands(aTheta);
          float k = b.x + b.y;
          vec3 c = vec3(cos(aTheta), sin(aTheta), 0.0);
          vec3 n = normalize(position - c * ${R.toFixed(2)});
          float g;
          vec3 base = position + field(position, g);
          vec3 toPtr = normalize(uPtr - base + vec3(1e-4));
          vec3 dirN = normalize(mix(n, toPtr, clamp(g * 1.6, 0.0, 0.9)));
          float grow = aLen * (0.18 + 1.6 * k + uActivity * 0.6 + 1.4 * g);
          vec3 p = base + dirN * grow * aTip;
          vCol = tint(b) + vec3(0.3, 0.4, 0.55) * g;
          vA = mix(0.5, 0.0, aTip) * (0.15 + 0.85 * k + 0.9 * g);
          gl_Position = projectionMatrix * modelViewMatrix * vec4(p, 1.0);
        }`,
      fragmentShader: /* glsl */ `
        uniform float uLight;
        varying vec3 vCol; varying float vA;
        void main(){ gl_FragColor = vec4(mix(vCol, vCol * 0.55, uLight), vA); }`,
    });
    rig.add(new THREE.LineSegments(spikeGeo, spikeMat));

    // ---------- Sparks riding the tube ----------
    const dCount = B.dust;
    const dPos = new Float32Array(dCount * 3);
    const dSeed = new Float32Array(dCount * 3);
    for (let i = 0; i < dCount; i++) {
      dSeed.set([Math.random() * Math.PI * 2, Math.random() * Math.PI * 2, Math.random()], i * 3);
    }
    const dustGeo = new THREE.BufferGeometry();
    dustGeo.setAttribute("position", new THREE.BufferAttribute(dPos, 3));
    dustGeo.setAttribute("aSeed", new THREE.BufferAttribute(dSeed, 3));
    const dustMat = new THREE.ShaderMaterial({
      uniforms: shared,
      transparent: true,
      depthWrite: false,
      blending: THREE.AdditiveBlending,
      vertexShader: /* glsl */ `
        ${bandGLSL}
        uniform float uPixel;
        attribute vec3 aSeed;
        varying vec3 vCol; varying float vA;
        void main(){
          float th = aSeed.x + uTime * (0.05 + aSeed.z * 0.08) * (1.0 + uActivity * 2.0);
          float ph = aSeed.y + uTime * (0.3 + aSeed.z * 0.5);
          float rr = ${r.toFixed(2)} * (0.9 + aSeed.z * 0.35);
          vec3 p = vec3((${R.toFixed(2)} + rr * cos(ph)) * cos(th), (${R.toFixed(2)} + rr * cos(ph)) * sin(th), rr * sin(ph));
          vec2 b = bands(th);
          float g;
          p += field(p, g) * 1.6;
          vCol = tint(b) + vec3(0.2) + vec3(0.3, 0.4, 0.5) * g;
          vA = (0.25 + 0.75 * (b.x + b.y) + g) * (0.4 + aSeed.z * 0.6);
          vec4 mv = modelViewMatrix * vec4(p, 1.0);
          gl_PointSize = (1.0 + aSeed.z * 2.2 + (b.x + b.y) * 1.5) * uPixel * (7.4 / -mv.z);
          gl_Position = projectionMatrix * mv;
        }`,
      fragmentShader: /* glsl */ `
        varying vec3 vCol; varying float vA;
        void main(){ float d = length(gl_PointCoord - 0.5); gl_FragColor = vec4(vCol, vA * smoothstep(0.5, 0.0, d)); }`,
    });
    rig.add(new THREE.Points(dustGeo, dustMat));

    // ---------- Nucleus: a living sphere of particles fills the centre ----------
    const nCount = B.nucleus;
    const nSeed = new Float32Array(nCount * 4);
    for (let i = 0; i < nCount; i++) {
      // fibonacci sphere directions, radius biased toward the shell
      const k = i + 0.5;
      const phi = Math.acos(1 - (2 * k) / nCount);
      const th = Math.PI * (1 + Math.sqrt(5)) * k;
      nSeed.set([th, phi, Math.pow(Math.random(), 0.45), Math.random()], i * 4);
    }
    const nucGeo = new THREE.BufferGeometry();
    nucGeo.setAttribute("position", new THREE.BufferAttribute(new Float32Array(nCount * 3), 3));
    nucGeo.setAttribute("aSeed", new THREE.BufferAttribute(nSeed, 4));
    const nucMat = new THREE.ShaderMaterial({
      uniforms: shared,
      transparent: true,
      depthWrite: false,
      blending: THREE.AdditiveBlending,
      vertexShader: /* glsl */ `
        ${bandGLSL}
        uniform float uPixel;
        attribute vec4 aSeed;
        varying vec3 vCol; varying float vA;
        void main(){
          float t = uTime * (0.12 + aSeed.w * 0.18) * (1.0 + uActivity * 1.5);
          float th = aSeed.x + t;
          float ph = aSeed.y + sin(t * 0.7 + aSeed.w * 6.28) * 0.35;
          float rad = ${(R - r * 1.05).toFixed(2)} * (0.18 + 0.82 * aSeed.z) * (1.0 + 0.06 * sin(uTime * 1.3 + aSeed.w * 20.0));
          vec3 p = rad * vec3(sin(ph) * cos(th), sin(ph) * sin(th), cos(ph) * 0.55);
          // flowing streams: particles drift along slow swirling currents
          p.xy += 0.08 * vec2(sin(p.y * 3.0 + uTime * 0.8), cos(p.x * 3.0 - uTime * 0.7));
          float g;
          p += field(p, g) * 1.2;
          vec2 b = bands(atan(p.y, p.x));
          float core = 1.0 - aSeed.z;
          vCol = mix(tint(b), vec3(0.85, 0.92, 1.0), core * 0.5) + vec3(0.3, 0.4, 0.55) * g;
          vA = (0.22 + 0.5 * (b.x + b.y) * aSeed.z + 0.6 * core * core + g) * (0.35 + aSeed.w * 0.65);
          vec4 mv = modelViewMatrix * vec4(p, 1.0);
          gl_PointSize = (0.9 + aSeed.w * 1.8 + core * 1.4) * uPixel * (7.4 / -mv.z);
          gl_Position = projectionMatrix * mv;
        }`,
      fragmentShader: /* glsl */ `
        varying vec3 vCol; varying float vA;
        void main(){ float d = length(gl_PointCoord - 0.5); gl_FragColor = vec4(vCol, vA * smoothstep(0.5, 0.0, d)); }`,
    });
    rig.add(new THREE.Points(nucGeo, nucMat));

    const resize = () => {
      const { width, height } = el.getBoundingClientRect();
      renderer.setSize(width, height, false);
      camera.aspect = width / Math.max(height, 1);
      camera.position.z = 10.1 * Math.max(1, 1.08 / camera.aspect);
      camera.updateProjectionMatrix();
    };
    const ro = new ResizeObserver(resize);
    ro.observe(el);
    resize();

    const target = new THREE.Vector2(0, 0);
    const lean = new THREE.Vector2(0, 0);
    // cursor in normalised device coords of this canvas (null when outside the window)
    const ndc = new THREE.Vector2(9, 9);
    let ptrInside = false;
    const ray = new THREE.Raycaster();
    const plane = new THREE.Plane();
    const hit = new THREE.Vector3();
    const local = new THREE.Vector3();
    const prevLocal = new THREE.Vector3();
    const normal = new THREE.Vector3();
    let ripIndex = 0;
    const toNdc = (e: PointerEvent) => {
      const rc = el.getBoundingClientRect();
      return [((e.clientX - rc.left) / rc.width) * 2 - 1, -(((e.clientY - rc.top) / rc.height) * 2 - 1)] as const;
    };
    const onMove = (e: PointerEvent) => {
      const rc = el.getBoundingClientRect();
      target.set(((e.clientX - rc.left) / rc.width) * 2 - 1, ((e.clientY - rc.top) / rc.height) * 2 - 1);
      target.clampScalar(-1.2, 1.2);
      const [x, y] = toNdc(e);
      ndc.set(x, y);
      ptrInside = true;
    };
    const onLeave = () => { ptrInside = false; };
    const pointerLocal = (out: THREE.Vector3) => {
      ray.setFromCamera(ndc, camera);
      normal.set(0, 0, 1).applyQuaternion(rig.quaternion);
      plane.setFromNormalAndCoplanarPoint(normal, rig.position);
      if (!ray.ray.intersectPlane(plane, hit)) return false;
      out.copy(hit);
      rig.worldToLocal(out);
      return true;
    };
    // drag-to-spin: works anywhere on the stage except on controls
    const stage = el.closest<HTMLElement>(".stage") ?? el;
    const spin = new THREE.Vector2(0, 0); // accumulated drag rotation (radians)
    const spinVel = new THREE.Vector2(0, 0);
    let drag: { x: number; y: number; moved: number; t: number } | null = null;
    let bandFollow: number | null = null;
    const ripple = (e: PointerEvent) => {
      const rc = el.getBoundingClientRect();
      if (e.clientX < rc.left || e.clientX > rc.right || e.clientY < rc.top || e.clientY > rc.bottom) return;
      const [x, y] = toNdc(e);
      ndc.set(x, y);
      const p = new THREE.Vector3();
      if (!pointerLocal(p) || p.length() > R + r * 3) return;
      shared.uRip.value[ripIndex % 4]!.set(p.x, p.y, shared.uClock.value, 1);
      ripIndex++;
    };
    const onDown = (e: PointerEvent) => {
      if (reduced || e.button !== 0) return;
      const t = e.target as HTMLElement | null;
      if (!t || !stage.contains(t) || t.closest("button:not(.core-label), a, input, textarea, select, .presence")) return;
      drag = { x: e.clientX, y: e.clientY, moved: 0, t: performance.now() };
      spinVel.set(0, 0);
      stage.dataset.dragging = "true";
    };
    const onDragMove = (e: PointerEvent) => {
      if (!drag) return;
      const dx = e.clientX - drag.x;
      const dy = e.clientY - drag.y;
      const now = performance.now();
      const dt = Math.max((now - drag.t) / 1000, 1 / 240);
      drag = { x: e.clientX, y: e.clientY, moved: drag.moved + Math.hypot(dx, dy), t: now };
      const k = 0.0065;
      spin.x += dy * k;
      spin.y += dx * k;
      spinVel.set((dy * k) / dt, (dx * k) / dt).clampScalar(-9, 9);
      if (drag.moved > 6 && pointerLocal(local)) bandFollow = Math.atan2(local.y, local.x);
    };
    const onUp = (e: PointerEvent) => {
      if (!drag) return;
      if (drag.moved < 6) ripple(e);
      drag = null;
      bandFollow = null;
      delete stage.dataset.dragging;
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointermove", onDragMove);
    window.addEventListener("pointerdown", onDown);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    document.documentElement.addEventListener("pointerleave", onLeave);

    // follow the nearest theme scope (the presence stage stays dark in both themes)
    const scope = el.closest<HTMLElement>("[data-theme]") ?? document.documentElement;
    const readTheme = () => { shared.uLight.value = scope.dataset.theme === "light" ? 1 : 0; };
    readTheme();
    const themeObs = new MutationObserver(readTheme);
    themeObs.observe(scope, { attributes: true, attributeFilter: ["data-theme"] });

    let raf = 0;
    let last = performance.now();
    let visible = document.visibilityState === "visible";
    const onVis = () => { visible = document.visibilityState === "visible"; last = performance.now(); };
    document.addEventListener("visibilitychange", onVis);
    let band = 1.2;

    const frame = (now: number) => {
      raf = requestAnimationFrame(frame);
      if (!visible) return;
      const dt = Math.min((now - last) / 1000, 0.05);
      last = now;
      const t = (shared.uTime.value += reduced ? dt * 0.15 : dt);
      shared.uActivity.value += (activityRef.current - shared.uActivity.value) * Math.min(dt * 2.5, 1);

      const want = bandFollow ?? (focusRef.current ? NODE_ANGLES[focusRef.current] : undefined);
      if (want !== undefined) {
        const d = Math.atan2(Math.sin(want - band), Math.cos(want - band));
        band += d * Math.min(dt * 4, 1);
      } else {
        band += dt * (0.32 + shared.uActivity.value * 1.4) * (reduced ? 0.2 : 1);
      }
      shared.uBand.value = band;

      shared.uClock.value += dt;
      // pointer field strength fades in near the core and out when the cursor leaves
      let wantK = 0;
      if (ptrInside && !reduced && pointerLocal(local)) {
        const dist = Math.hypot(local.x, local.y);
        wantK = 1 - THREE.MathUtils.smoothstep(dist, R + r * 1.2, R + r * 4.5);
        const vx = (local.x - prevLocal.x) / Math.max(dt, 1e-3);
        const vy = (local.y - prevLocal.y) / Math.max(dt, 1e-3);
        shared.uPtrVel.value.lerp(new THREE.Vector2(THREE.MathUtils.clamp(vx * 0.08, -0.6, 0.6), THREE.MathUtils.clamp(vy * 0.08, -0.6, 0.6)), Math.min(dt * 6, 1));
        prevLocal.copy(local);
        shared.uPtr.value.lerp(local, Math.min(dt * 10, 1));
      } else {
        shared.uPtrVel.value.multiplyScalar(0.9);
      }
      shared.uPtrK.value += (wantK - shared.uPtrK.value) * Math.min(dt * 4, 1);

      // momentum after release, then a soft spring back to the resting pose
      if (!drag) {
        spin.x += spinVel.x * dt;
        spin.y += spinVel.y * dt;
        spinVel.multiplyScalar(Math.exp(-dt * 2.2));
        spin.multiplyScalar(Math.exp(-dt * 0.9));
      }
      // wake: the weave stretches along the drag direction
      if (drag) shared.uPtrVel.value.set(THREE.MathUtils.clamp(spinVel.y * 0.08, -0.6, 0.6), THREE.MathUtils.clamp(-spinVel.x * 0.08, -0.6, 0.6));

      lean.lerp(target, Math.min(dt * 3, 1));
      // near face-on so the label sits in the ring; slow tumble + cursor lean + drag spin
      rig.rotation.x = -0.32 + Math.sin(t * 0.11) * 0.1 + lean.y * 0.18 + spin.x;
      rig.rotation.y = 0.18 + Math.sin(t * 0.07) * 0.14 + lean.x * 0.24 + spin.y;
      rig.rotation.z = t * 0.035;
      renderer.render(scene, camera);
    };
    raf = requestAnimationFrame(frame);

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      themeObs.disconnect();
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerdown", onDown);
      window.removeEventListener("pointermove", onDragMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      document.documentElement.removeEventListener("pointerleave", onLeave);
      document.removeEventListener("visibilitychange", onVis);
      [lineGeo, spikeGeo, dustGeo, nucGeo].forEach((g) => g.dispose());
      [lineMat, spikeMat, dustMat, nucMat].forEach((m) => m.dispose());
      renderer.dispose();
      renderer.domElement.remove();
    };
  }, [motion, reduced]);

  return <div ref={host} className="core-canvas" aria-hidden="true" />;
}
