import { useEffect, useRef } from "react";
import * as THREE from "three";
import { motion as motionTokens, type MotionLevel } from "@nexonic/design-system";

/**
 * Collective Intelligence core: interlaced orbital shells of particles.
 * Sharee's cyan flows from the left, Nexa's amber from the right, and they meet
 * in the middle. `activity` (0–1) must come from real events; at 0 the core
 * shows only its slow idle drift, which is decorative and labelled as idle.
 */
export function NeuralCore({
  activity = 0,
  motion = "balanced",
  reduced = false,
}: {
  activity?: number;
  motion?: MotionLevel;
  reduced?: boolean;
}) {
  const host = useRef<HTMLDivElement>(null);
  const activityRef = useRef(activity);
  activityRef.current = activity;

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const count = reduced ? 1600 : motionTokens.particles[motion];
    if (count === 0) return;

    let renderer: THREE.WebGLRenderer;
    try {
      renderer = new THREE.WebGLRenderer({ antialias: false, alpha: true, powerPreference: "high-performance" });
    } catch {
      el.dataset.fallback = "true"; // CSS draws a static core
      return;
    }
    const dpr = Math.min(window.devicePixelRatio, 2);
    renderer.setPixelRatio(dpr);
    el.appendChild(renderer.domElement);

    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(38, 1, 0.1, 50);
    camera.position.set(0, 0, 5.7);

    // Each particle lives on one of several tilted orbital rings (a "shell").
    const rings = 14;
    const seed = new Float32Array(count * 4); // ring, phase, radius jitter, size
    const pos = new Float32Array(count * 3);
    for (let i = 0; i < count; i++) {
      seed[i * 4] = i % rings;
      seed[i * 4 + 1] = Math.random() * Math.PI * 2;
      seed[i * 4 + 2] = (Math.random() - 0.5) * 0.16;
      seed[i * 4 + 3] = Math.random() ** 3;
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.BufferAttribute(pos, 3));
    geo.setAttribute("seed", new THREE.BufferAttribute(seed, 4));

    const uniforms = {
      uTime: { value: 0 },
      uActivity: { value: 0 },
      uPointer: { value: new THREE.Vector3(9, 9, 0) },
      uPixel: { value: dpr },
      uSharee: { value: new THREE.Color("#00dec6") },
      uNexa: { value: new THREE.Color("#ffb568") },
      uRings: { value: rings },
    };

    const mat = new THREE.ShaderMaterial({
      uniforms,
      transparent: true,
      depthWrite: false,
      blending: THREE.AdditiveBlending,
      vertexShader: /* glsl */ `
        attribute vec4 seed;
        uniform float uTime, uActivity, uPixel, uRings;
        uniform vec3 uPointer;
        varying float vMix;
        varying float vAlpha;
        mat3 rotX(float a){float c=cos(a),s=sin(a);return mat3(1.,0.,0.,0.,c,-s,0.,s,c);}
        mat3 rotY(float a){float c=cos(a),s=sin(a);return mat3(c,0.,s,0.,1.,0.,-s,0.,c);}
        mat3 rotZ(float a){float c=cos(a),s=sin(a);return mat3(c,-s,0.,s,c,0.,0.,0.,1.);}
        void main(){
          float r = seed.x / uRings;
          float speed = 0.10 + 0.05 * fract(r * 7.13) + uActivity * 0.35;
          float t = seed.y + uTime * speed * (mod(seed.x, 2.) < 1. ? 1. : -1.);
          // lemniscate-ish orbit gives the woven, braided look
          float rad = 1.55 + seed.z + 0.18 * sin(t * 3. + r * 6.2831);
          vec3 p = vec3(cos(t) * rad, sin(t * 2.) * 0.22, sin(t) * rad);
          p = rotZ(r * 3.1416 + 0.4) * rotX(r * 2.1 + uTime * 0.03) * rotY(uTime * 0.05) * p;
          // pointer attraction in view space, soft and bounded
          vec4 mv = modelViewMatrix * vec4(p, 1.);
          vec3 d = uPointer - mv.xyz;
          float pull = exp(-dot(d.xy, d.xy) * 1.6) * 0.35;
          mv.xy += d.xy * pull;
          gl_Position = projectionMatrix * mv;
          float size = mix(1.6, 4.6, seed.w) * (1. + uActivity * 0.8);
          gl_PointSize = size * uPixel * (5.7 / -mv.z);
          vMix = smoothstep(-1.5, 1.5, p.x);
          vAlpha = (0.35 + 0.65 * seed.w) * smoothstep(-3.0, 0.6, mv.z + 5.7);
        }`,
      fragmentShader: /* glsl */ `
        uniform vec3 uSharee, uNexa;
        uniform float uActivity;
        varying float vMix;
        varying float vAlpha;
        void main(){
          vec2 c = gl_PointCoord - .5;
          float d = length(c);
          float a = smoothstep(.5, 0., d);
          vec3 col = mix(uSharee, uNexa, vMix);
          col += vec3(1.) * smoothstep(.18, 0., d) * .35; // hot centre
          gl_FragColor = vec4(col, a * vAlpha * (.9 + uActivity * .4));
        }`,
    });

    scene.add(new THREE.Points(geo, mat));

    // Woven filaments: tilted orbital loops, vertex-coloured cyan → amber.
    const filaments = new THREE.Group();
    const loopMat = new THREE.LineBasicMaterial({ vertexColors: true, transparent: true, opacity: 0.32, blending: THREE.AdditiveBlending, depthWrite: false });
    const cS = new THREE.Color("#00dec6"), cN = new THREE.Color("#ffb568");
    const loops = reduced ? 10 : 26;
    const loopGeos: THREE.BufferGeometry[] = [];
    for (let k = 0; k < loops; k++) {
      const n = 220, pts = new Float32Array(n * 3), cols = new Float32Array(n * 3);
      const rx = 1.45 + Math.random() * 0.35, ry = 0.55 + Math.random() * 0.9, wob = Math.random() * 0.25;
      for (let i = 0; i < n; i++) {
        const t = (i / n) * Math.PI * 2;
        const v = new THREE.Vector3(Math.cos(t) * rx, Math.sin(t) * ry, Math.sin(t * 3) * wob);
        pts.set([v.x, v.y, v.z], i * 3);
      }
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.BufferAttribute(pts, 3));
      const line = new THREE.LineLoop(g, loopMat);
      line.rotation.set(Math.random() * Math.PI, Math.random() * Math.PI, (k / loops) * Math.PI);
      line.updateMatrix();
      // colour by final x position so cyan sits left, amber right
      const tmp = new THREE.Vector3();
      for (let i = 0; i < n; i++) {
        tmp.fromBufferAttribute(g.getAttribute("position") as THREE.BufferAttribute, i).applyMatrix4(line.matrix);
        const c = cS.clone().lerp(cN, THREE.MathUtils.smoothstep(tmp.x, -1.4, 1.4));
        cols.set([c.r, c.g, c.b], i * 3);
      }
      g.setAttribute("color", new THREE.BufferAttribute(cols, 3));
      loopGeos.push(g);
      filaments.add(line);
    }
    scene.add(filaments);

    const resize = () => {
      const { width, height } = el.getBoundingClientRect();
      renderer.setSize(width, height, false);
      camera.aspect = width / Math.max(height, 1);
      camera.updateProjectionMatrix();
    };
    const ro = new ResizeObserver(resize);
    ro.observe(el);
    resize();

    const onMove = (e: PointerEvent) => {
      const r = el.getBoundingClientRect();
      const x = ((e.clientX - r.left) / r.width) * 2 - 1;
      const y = -(((e.clientY - r.top) / r.height) * 2 - 1);
      // map NDC to view space at the core's depth
      const h = Math.tan(THREE.MathUtils.degToRad(camera.fov / 2)) * camera.position.z;
      uniforms.uPointer.value.set(x * h * camera.aspect, y * h, -camera.position.z);
    };
    const onLeave = () => uniforms.uPointer.value.set(9, 9, 0);
    window.addEventListener("pointermove", onMove);
    el.addEventListener("pointerleave", onLeave);

    let raf = 0;
    let last = performance.now();
    let visible = true;
    const vis = () => { visible = document.visibilityState === "visible"; };
    document.addEventListener("visibilitychange", vis);

    const frame = (now: number) => {
      raf = requestAnimationFrame(frame);
      if (!visible) return; // suspend rendering in the background
      const dt = Math.min((now - last) / 1000, 0.05);
      last = now;
      uniforms.uTime.value += reduced ? 0 : dt;
      filaments.rotation.y = Math.sin(uniforms.uTime.value * 0.12) * 0.28;
      filaments.rotation.z += reduced ? 0 : dt * 0.015;
      uniforms.uActivity.value += (activityRef.current - uniforms.uActivity.value) * Math.min(dt * 3, 1);
      renderer.render(scene, camera);
    };
    raf = requestAnimationFrame(frame);

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      window.removeEventListener("pointermove", onMove);
      el.removeEventListener("pointerleave", onLeave);
      document.removeEventListener("visibilitychange", vis);
      geo.dispose();
      loopGeos.forEach((g) => g.dispose());
      loopMat.dispose();
      mat.dispose();
      renderer.dispose();
      renderer.domElement.remove();
    };
  }, [motion, reduced]);

  return <div ref={host} className="core-canvas" aria-hidden="true" />;
}
