import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { presenceLabel, type MotionLevel, type PresenceState, type Theme } from "@nexonic/design-system";
import { Icon, type IconName } from "./components/Icon";
import { NeuralCore } from "./components/NeuralCore";
import { LiveFace, type FaceAtlas, type Gaze, type Mood } from "./components/LiveFace";
import { VideoFace, type ClipName, type ClipSet, type FaceBox } from "./components/VideoFace";
import { MOOD_LABEL, TEMPERAMENT, usePresenceMood } from "./lib/presenceMood";
import { CountUp, RollText, ScrambleText, SplitText, useCycle } from "./components/MotionText";
import { capabilityLabel, formatBytes, inDesktop, ipc, type Capability, type MemorySummary, type MemoryView, type PermissionsView, type SystemInfo, type UpdateStatus } from "./lib/ipc";
import { usePersisted, useReducedMotion } from "./lib/store";
import shareeNeutral from "./assets/characters/sharee.webp";
import nexaNeutral from "./assets/characters/nexa.webp";

type Section = "presence" | "hub" | "memory" | "evolution" | "activity" | "computer" | "intelligence" | "settings";
type Mode = "world" | "collaborate" | "analyze" | "create" | "automate";
type Node = "Reasoning" | "Memory" | "Learning" | "Perception" | "Planning" | "Action";
type Agent = "Sharee" | "Nexa";

const FALLBACK: Record<Agent, string> = { Sharee: shareeNeutral, Nexa: nexaNeutral };

// Live-portrait atlas rendered on Apple Silicon by services/presence/render_atlas.py.
const ATLAS_FILES = import.meta.glob<string>("./assets/live/*/*.webp", { eager: true, query: "?url", import: "default" });
type Manifest = { yaws: number[]; pitches: number[]; exprYaws: number[]; faces?: Record<string, { cx: number; cy: number; w: number }> };
const ATLAS_MANIFEST = import.meta.glob<Manifest>("./assets/live/manifest.json", { eager: true, import: "default" });
function loadAtlas(who: Agent): FaceAtlas | null {
  const manifest = Object.values(ATLAS_MANIFEST)[0];
  if (!manifest) return null;
  const frames: Record<string, string> = {};
  const dir = `/live/${who.toLowerCase()}/`;
  for (const [path, url] of Object.entries(ATLAS_FILES)) {
    const at = path.indexOf(dir);
    if (at >= 0) frames[path.slice(at + dir.length, -".webp".length)] = url;
  }
  if (!frames["g_0_0_o"]) return null;
  return { yaws: manifest.yaws, pitches: manifest.pitches, exprYaws: manifest.exprYaws, frames, face: manifest.faces?.[who.toLowerCase()] };
}
const ATLAS: Record<Agent, FaceAtlas | null> = { Sharee: loadAtlas("Sharee"), Nexa: loadAtlas("Nexa") };

// Real video clips (services/presence/process_clip.py): colour and alpha packed side by side.
const CLIP_FILES = import.meta.glob<string>("./assets/clips/*/*.mp4", { eager: true, query: "?url", import: "default" });
const CLIP_MANIFEST = import.meta.glob<{ faces: Record<string, FaceBox> }>("./assets/clips/manifest.json", { eager: true, import: "default" });
function loadClips(who: Agent): { clips: ClipSet; face?: FaceBox } | null {
  const clips: ClipSet = {};
  const dir = `/clips/${who.toLowerCase()}/`;
  for (const [path, url] of Object.entries(CLIP_FILES)) {
    const at = path.indexOf(dir);
    if (at >= 0) clips[path.slice(at + dir.length, -".mp4".length) as ClipName] = url;
  }
  if (!clips.idle) return null;
  return { clips, face: Object.values(CLIP_MANIFEST)[0]?.faces[who.toLowerCase()] };
}
const CLIPS: Record<Agent, { clips: ClipSet; face?: FaceBox } | null> = { Sharee: loadClips("Sharee"), Nexa: loadClips("Nexa") };

/**
 * When nobody is talking to them, Sharee and Nexa now and then turn to each
 * other and chat, then face the user again. Decorative body language only:
 * no words are generated and it is never shown as activity.
 */
function usePartnerChat(quiet: boolean, reduced: boolean): boolean {
  const [chatting, setChatting] = useState(false);
  useEffect(() => {
    if (!quiet || reduced) {
      setChatting(false);
      return;
    }
    let t = 0;
    const plan = () => {
      t = window.setTimeout(() => {
        setChatting(true);
        t = window.setTimeout(() => {
          setChatting(false);
          plan();
        }, 8200);
      }, 14000 + Math.random() * 12000);
    };
    plan();
    return () => window.clearTimeout(t);
  }, [quiet, reduced]);
  return chatting;
}

const NAV: { id: Section; icon: IconName; title: string; sub: string }[] = [
  { id: "presence", icon: "presence", title: "Presence", sub: "Sharee, Nexa & You" },
  { id: "hub", icon: "hub", title: "Visual Hub", sub: "Adaptive Workspaces" },
  { id: "memory", icon: "memory", title: "Memory", sub: "Your Cognitive Memory" },
  { id: "evolution", icon: "evolution", title: "Evolution", sub: "Learning & Self-Training" },
  { id: "activity", icon: "activity", title: "Activity", sub: "Tasks, Goals & Automation" },
  { id: "computer", icon: "computer", title: "Computer Control", sub: "Files, Apps & Workflows" },
  { id: "intelligence", icon: "intelligence", title: "Intelligence", sub: "Models, Providers & Skills" },
  { id: "settings", icon: "settings", title: "Settings", sub: "Personalize & Manage" },
];

const MODES: { id: Mode; icon: IconName; label: string; panels: PanelId[] }[] = [
  { id: "world", icon: "world", label: "Your World", panels: ["focus", "collab", "memory", "awareness"] },
  { id: "collaborate", icon: "users", label: "Collaborate", panels: ["collab", "focus", "memory"] },
  { id: "analyze", icon: "chart", label: "Analyze", panels: ["memory", "awareness", "collab"] },
  { id: "create", icon: "pen", label: "Create", panels: ["focus", "collab"] },
  { id: "automate", icon: "layers", label: "Automate", panels: ["awareness", "focus"] },
];
type PanelId = "focus" | "collab" | "memory" | "awareness";

const NODES: { id: Node; icon: IconName; pos: string; owner: string; status: string; path: string; pin: [number, number]; hub: [number, number] }[] = [
  { id: "Reasoning", icon: "spark", pos: "n-tl", owner: "Agent runtime (M16)", status: "Waiting for a model provider", path: "M 134 56 H 196 L 236 118", pin: [134, 56], hub: [236, 118] },
  { id: "Perception", icon: "camera", pos: "n-tr", owner: "Awareness (M24–M27)", status: "Camera, microphone and screen are off", path: "M 466 56 H 404 L 364 118", pin: [466, 56], hub: [364, 118] },
  { id: "Memory", icon: "memory", pos: "n-ml", owner: "Memory engine (M20)", status: "Isolation active · in-memory store", path: "M 122 200 H 194", pin: [122, 200], hub: [194, 200] },
  { id: "Planning", icon: "target", pos: "n-mr", owner: "Agent runtime (M16)", status: "Waiting for a model provider", path: "M 478 200 H 406", pin: [478, 200], hub: [406, 200] },
  { id: "Learning", icon: "evolution", pos: "n-bl", owner: "Learning (M36–M39)", status: "Not started", path: "M 134 344 H 196 L 236 282", pin: [134, 344], hub: [236, 282] },
  { id: "Action", icon: "play", pos: "n-br", owner: "Computer control (M32)", status: "Automation permission is off", path: "M 466 344 H 404 L 364 282", pin: [466, 344], hub: [364, 282] },
];

interface Task { id: string; text: string; done: boolean }

/** Circuit trace from a node to the ring; follows a node that was dragged somewhere else. */
function tracePath(n: (typeof NODES)[number], pos?: { x: number; y: number }) {
  if (!pos) return { d: n.path, pin: n.pin, hub: n.hub };
  const px = (pos.x / 100) * 600;
  const py = (pos.y / 100) * 400;
  const ang = Math.atan2(py - 200, px - 300);
  const hub: [number, number] = [300 + Math.cos(ang) * 110, 200 + Math.sin(ang) * 84];
  const mx = px + (hub[0] - px) * 0.45;
  return { d: `M ${px.toFixed(1)} ${py.toFixed(1)} H ${mx.toFixed(1)} L ${hub[0].toFixed(1)} ${hub[1].toFixed(1)}`, pin: [px, py] as [number, number], hub };
}

export function App() {
  const reduced = useReducedMotion();
  const [theme, setTheme] = usePersisted<Theme>("theme", "dark");
  const [motion] = usePersisted<MotionLevel>("motion", "balanced");
  const [section, setSection] = useState<Section>("presence");
  const [mode, setMode] = usePersisted<Mode>("mode", "world");
  const [node, setNode] = useState<Node | null>(null);
  // Nodes can be dragged anywhere in the core area; positions are % of the core box.
  const [nodePos, setNodePos] = usePersisted<Partial<Record<Node, { x: number; y: number }>>>("node-pos", {});
  const nodeDrag = useRef<{ id: Node; sx: number; sy: number; moved: boolean } | null>(null);
  const [hoverNode, setHoverNode] = useState<Node | null>(null);
  const [draft, setDraft] = useState("");
  const [sentAt, setSentAt] = useState(0);
  const [greetAt, setGreetAt] = useState<Record<Agent, number>>({ Sharee: 0, Nexa: 0 });
  const [palette, setPalette] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [autoUpdate, setAutoUpdate] = usePersisted<boolean>("auto-update", false);
  const [updateInfo, setUpdateInfo] = useState<UpdateStatus | null>(null);
  const [sys, setSys] = useState<SystemInfo | null>(null);
  const [perms, setPerms] = useState<PermissionsView | null>(null);
  const [memoryInfo, setMemoryInfo] = useState<MemorySummary | null>(null);
  const [memories, setMemories] = useState<MemoryView[]>([]);

  const nodes = useMemo(
    () =>
      NODES.map((n) =>
        n.id === "Memory"
          ? {
              ...n,
              status: memoryInfo?.storage_error
                ? "Temporary store · encrypted storage unavailable (see Memory panel)"
                : memoryInfo?.persistent
                ? "Isolation active · encrypted SQLite (Keychain)"
                : inDesktop()
                  ? "Isolation active · encrypted SQLite"
                  : "Browser preview · desktop storage unavailable",
            }
          : n,
      ),
    [memoryInfo],
  );

  useEffect(() => { document.documentElement.dataset.theme = theme; }, [theme]);
  usePointerLight(reduced);
  const modesRef = useRef<HTMLElement>(null);
  useLayoutEffect(() => {
    const place = () => {
      const nav = modesRef.current;
      const b = nav?.querySelector<HTMLElement>('[aria-pressed="true"]');
      const ink = nav?.querySelector<HTMLElement>(".mode-ink");
      if (b && ink) { ink.style.width = `${b.offsetWidth}px`; ink.style.transform = `translateX(${b.offsetLeft}px)`; }
    };
    place();
    void document.fonts?.ready.then(place);
    window.addEventListener("resize", place);
    return () => window.removeEventListener("resize", place);
  }, [mode]);
  useEffect(() => {
    void ipc.systemInfo().then(setSys);
    void ipc.permissions().then(setPerms);
    void ipc.memorySummary().then(setMemoryInfo);
    void ipc.memoriesRecent(12).then((items) => {
      if (items) setMemories(items);
    });
  }, []);
  useEffect(() => {
    if (!autoUpdate || !inDesktop()) return;
    void ipc.checkUpdates().then((u) => { if (u) setUpdateInfo(u); });
  }, [autoUpdate]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") { e.preventDefault(); setPalette((p) => !p); }
      if (e.key === "Escape") { setPalette(false); setNode(null); setAboutOpen(false); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Agents have no model provider yet (M17), so both are honestly offline.
  const agentState: PresenceState = "offline";
  const activeMode = MODES.find((m) => m.id === mode) ?? MODES[0]!;
  const coreFocus = hoverNode ?? node;
  // Real agent activity arrives with the agent runtimes (M16); cursor play on the core is not activity.
  const coreActivity = 0;
  const presence = usePresenceMood({
    draft,
    sentAt,
    greetAt,
    reduced,
    agentEl: (who) => document.querySelector<HTMLElement>(`.presence.${who.toLowerCase()} .portrait-stage`),
    inputEl: () => document.querySelector<HTMLElement>(".talk-field"),
  });
  const [quiet, setQuiet] = useState(true);
  useEffect(() => {
    setQuiet(false);
    if (draft.trim()) return;
    const t = window.setTimeout(() => setQuiet(true), 6000);
    return () => window.clearTimeout(t);
  }, [draft, sentAt]);
  const chatting = usePartnerChat(quiet, reduced);
  const clipWant: ClipName = chatting ? "chat" : "idle";

  return (
    <div className="app" data-mode={mode}>
      <div className="titlebar" data-tauri-drag-region />
      <Header
        onSearch={() => setPalette(true)}
        onAbout={() => setAboutOpen(true)}
        updateAvailable={Boolean(updateInfo?.updateAvailable)}
        theme={theme}
        onTheme={() => setTheme(theme === "dark" ? "light" : "dark")}
      />
      <Sidebar section={section} onSection={(s) => { setSection(s); if (s === "settings") setAboutOpen(true); }} sys={sys} />
      <main className="main">
        <section className="stage" aria-label="Presence" data-theme="dark">
          <Presence
            name="Sharee"
            side="left"
            state={agentState}
            mood={presence.Sharee.mood}
            gaze={presence.Sharee.gaze}
            clip={clipWant}
            reduced={reduced}
            onGreet={() => setGreetAt((g) => ({ ...g, Sharee: performance.now() }))}
          />
          <div className="core" data-active={coreActivity > 0}>
            <NeuralCore motion={motion} reduced={reduced} activity={coreActivity} focus={coreFocus} />
            <svg className="links" viewBox="0 0 600 400" preserveAspectRatio="none" aria-hidden="true">
              <circle className="hud-ring" cx="300" cy="200" r="108" />
              {nodes.map((n) => {
                const active = hoverNode === n.id || node === n.id;
                const sideClass = n.pos.endsWith("l") ? "left" : "right";
                return (
                  <g key={n.id} className={`trace ${sideClass} ${active ? "active" : ""}`}>
                    <path className="trace-base" d={tracePath(n, nodePos[n.id]).d} />
                    <path className="trace-pulse" d={tracePath(n, nodePos[n.id]).d} />
                    <circle className="trace-dot" cx={tracePath(n, nodePos[n.id]).pin[0]} cy={tracePath(n, nodePos[n.id]).pin[1]} r="3" />
                    <circle className="trace-dot" cx={tracePath(n, nodePos[n.id]).hub[0]} cy={tracePath(n, nodePos[n.id]).hub[1]} r="3.5" />
                  </g>
                );
              })}
            </svg>
            <button
              type="button"
              className="core-label"
              aria-label="Collective Intelligence Core (click to send a ripple)"
            >
              <Icon name="wave" size={24} />
              <strong><SplitText text="COLLECTIVE" delay={300} stagger={30} /><SplitText text="INTELLIGENCE" delay={420} stagger={30} /></strong>
              <span><ScrambleText text="SHAREE" delay={700} className="c-sharee" /> × <ScrambleText text="NEXA" delay={760} className="c-nexa" /></span>
              <b className="inf">∞</b>
              <em><RollText value={coreActivity > 0 ? "ACTIVE" : "IDLE"} /></em>
            </button>
            {nodes.map((n) => (
              <button key={n.id} className={`node ${n.pos} ${nodePos[n.id] ? "moved" : ""}`} data-node={n.id} data-spot aria-pressed={node === n.id}
                style={nodePos[n.id] ? { left: `${nodePos[n.id]!.x}%`, top: `${nodePos[n.id]!.y}%`, right: "auto", bottom: "auto" } : undefined}
                title="Drag to move · double-click to put back"
                onPointerDown={(e) => {
                  if (e.button !== 0) return;
                  e.stopPropagation();
                  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
                  nodeDrag.current = { id: n.id, sx: e.clientX, sy: e.clientY, moved: false };
                }}
                onPointerMove={(e) => {
                  const d = nodeDrag.current;
                  if (!d || d.id !== n.id) return;
                  if (!d.moved && Math.hypot(e.clientX - d.sx, e.clientY - d.sy) < 5) return;
                  d.moved = true;
                  const core = (e.currentTarget as HTMLElement).closest(".core")!.getBoundingClientRect();
                  const x = Math.min(92, Math.max(8, ((e.clientX - core.left) / core.width) * 100));
                  const y = Math.min(94, Math.max(6, ((e.clientY - core.top) / core.height) * 100));
                  setNodePos((p) => ({ ...p, [n.id]: { x, y } }));
                }}
                onPointerUp={(e) => {
                  const d = nodeDrag.current;
                  nodeDrag.current = null;
                  if (d && !d.moved && e.detail < 2) setNode(n.id);
                }}
                onDoubleClick={() => { setNode(null); setNodePos((p) => { const q = { ...p }; delete q[n.id]; return q; }); }}
                onPointerEnter={() => setHoverNode(n.id)} onPointerLeave={() => setHoverNode(null)} onFocus={() => setHoverNode(n.id)} onBlur={() => setHoverNode(null)}>
                <Icon name={n.icon} size={15} /><span>{n.id}</span>
              </button>
            ))}
          </div>
          <Presence
            name="Nexa"
            side="right"
            state={agentState}
            mood={presence.Nexa.mood}
            gaze={presence.Nexa.gaze}
            clip={clipWant}
            reduced={reduced}
            onGreet={() => setGreetAt((g) => ({ ...g, Nexa: performance.now() }))}
          />
          <nav className="modes" aria-label="Workspace mode" ref={modesRef}>
            <span className="mode-ink" aria-hidden="true" />
            {MODES.map((m) => (
              <button key={m.id} className="mode" data-spot aria-pressed={m.id === mode} onClick={() => setMode(m.id)}>
                <Icon name={m.icon} size={16} />{m.label}
              </button>
            ))}
          </nav>
        </section>
        <section className="panels" style={{ ["--cols" as string]: activeMode.panels.length }}>
          {activeMode.panels.map((p) => (
            <div key={`${mode}-${p}`} className="panel-slot">
              {p === "focus" && <FocusPanel />}
              {p === "collab" && <CollabPanel />}
              {p === "memory" && (
                <MemoryPanel
                  info={memoryInfo}
                  memories={memories}
                  onForget={(id) => {
                    void ipc.forgetMemory(id).then(() => {
                      setMemories((cur) => cur.filter((x) => x.id !== id));
                      void ipc.memorySummary().then(setMemoryInfo);
                    });
                  }}
                />
              )}
              {p === "awareness" && <AwarenessPanel perms={perms} setPerms={setPerms} sys={sys} />}
            </div>
          ))}
        </section>
        <Dock onDraft={setDraft} onSend={() => setSentAt(performance.now())} />
      </main>
      {node && <Inspector node={nodes.find((n) => n.id === node)!} onClose={() => setNode(null)} />}
      {palette && (
        <CommandPalette
          onClose={() => setPalette(false)}
          onSection={(s) => { setSection(s); setPalette(false); }}
          onMode={(m) => { setMode(m); setPalette(false); }}
          onAbout={() => { setPalette(false); setAboutOpen(true); }}
        />
      )}
      {aboutOpen && (
        <AboutModal
          sys={sys}
          autoUpdate={autoUpdate}
          onAutoUpdate={setAutoUpdate}
          updateInfo={updateInfo}
          onCheckUpdates={() => {
            if (!inDesktop()) {
              setUpdateInfo({
                currentVersion: "0.1.0",
                latestVersion: null,
                updateAvailable: false,
                releaseUrl: "https://github.com/nexonic-technologies/Nexonic-AI/releases",
                websiteUrl: "https://nexonictechnologies.com/",
                message: "Browser preview · launch desktop app for live release checks",
              });
              return;
            }
            void ipc.checkUpdates().then((u) => { if (u) setUpdateInfo(u); });
          }}
          onClose={() => setAboutOpen(false)}
        />
      )}
    </div>
  );
}

/** Feeds the cursor into CSS: --px/--py (−1…1) on the app for parallax, and
 *  --mx/--my (px) on the hovered [data-spot] element for its light. */
function usePointerLight(reduced: boolean) {
  useEffect(() => {
    if (reduced) return;
    const root = document.querySelector<HTMLElement>(".app");
    let raf = 0;
    let ev: PointerEvent | null = null;
    const apply = () => {
      raf = 0;
      if (!ev || !root) return;
      root.style.setProperty("--px", ((ev.clientX / innerWidth) * 2 - 1).toFixed(3));
      root.style.setProperty("--py", ((ev.clientY / innerHeight) * 2 - 1).toFixed(3));
      const spot = (ev.target as Element | null)?.closest?.<HTMLElement>("[data-spot]");
      if (spot) {
        const r = spot.getBoundingClientRect();
        spot.style.setProperty("--mx", `${ev.clientX - r.left}px`);
        spot.style.setProperty("--my", `${ev.clientY - r.top}px`);
      }
    };
    const onMove = (e: PointerEvent) => { ev = e; if (!raf) raf = requestAnimationFrame(apply); };
    window.addEventListener("pointermove", onMove, { passive: true });
    return () => { window.removeEventListener("pointermove", onMove); cancelAnimationFrame(raf); };
  }, [reduced]);
}

function Sidebar({ section, onSection, sys }: { section: Section; onSection: (s: Section) => void; sys: SystemInfo | null }) {
  return (
    <aside className="sidebar">
      <nav aria-label="Main">
        {NAV.map((n) => (
          <button key={n.id} className="nav" data-spot aria-current={section === n.id ? "page" : undefined} onClick={() => onSection(n.id)}>
            <Icon name={n.icon} size={20} />
            <span><b>{n.title}</b><small>{n.sub}</small></span>
          </button>
        ))}
      </nav>
      <div className="dune" aria-hidden="true" />
      <div className="sys-card" data-spot title={sys ? `${sys.chip} · ${formatBytes(sys.memoryBytes)} unified memory · ${sys.cpuCores} cores · ${sys.osName} ${sys.osVersion}` : undefined}>
        <div className="chip"><span className="apple" aria-hidden="true"></span>{sys ? chipShort(sys.chip) : "—"}</div>
        <div>
          <b>Local System</b>
          <small>{sys ? `Nexonic Core v${sys.appVersion}` : "Detecting…"}</small>
          <small className={sys ? "ok" : "muted"}><span className={`dot ${sys ? "" : "off"}`} />{sys ? "Running" : "Not connected"}</small>
        </div>
        <Icon name="chart" size={16} className="bars" />
      </div>
      <button className="agents-chip" aria-label="Agent availability">
        <span className="duo-av"><img src={shareeNeutral} alt="" /><img src={nexaNeutral} alt="" /></span>
        <span><span className="dot off" />Both on standby</span>
        <Icon name="chev" size={14} />
      </button>
    </aside>
  );
}

function chipShort(brand: string): string {
  const m = brand.match(/Apple\s+(M\d+(?:\s+(?:Pro|Max|Ultra))?)/i);
  return m?.[1] ?? brand.split(" ").slice(-1)[0] ?? brand;
}

function Mark() {
  return (
    <span className="nx-mark" aria-hidden="true">
      <svg width="36" height="36" viewBox="0 0 40 40">
        <defs>
          <linearGradient id="nx-hex" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0" stopColor="#4F46E5" />
            <stop offset="0.55" stopColor="#7C3AED" />
            <stop offset="1" stopColor="#00DEC6" />
          </linearGradient>
        </defs>
        <path className="nx-hex-bg" d="M20 2 36 11v18L20 38 4 29V11Z" fill="url(#nx-hex)" />
        <path d="M13.5 27V13l13 14V13" fill="none" stroke="#ffffff" strokeWidth="3.2" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    </span>
  );
}

function Header({
  onSearch,
  onAbout,
  updateAvailable,
  theme,
  onTheme,
}: {
  onSearch: () => void;
  onAbout: () => void;
  updateAvailable: boolean;
  theme: Theme;
  onTheme: () => void;
}) {
  return (
    <header className="header">
      <button className="brand" onClick={onAbout} title="About Nexonic AI & Nexonic Technologies">
        <Mark />
        <div><SplitText as="b" text="NEXONIC" delay={60} stagger={40} /><span>INTELLIGENCE</span></div>
      </button>
      <div className="title">
        <SplitText as="h1" text="My Intelligence Environment" delay={180} stagger={14} />
        <p>{["Two Minds", "One Collective Intelligence", "Built Around You"].map((w, i) => (
          <span key={w} className="tag-word" style={{ animationDelay: `${520 + i * 140}ms` }}>{i > 0 && <i>·</i>}{w}</span>
        ))}</p>
      </div>
      <button className="search" onClick={onSearch}>
        <Icon name="search" /> <span>Search memories, files, tasks…</span> <kbd>⌘ K</kbd>
      </button>
      {!inDesktop() && <span className="preview-chip" title="Browser preview · desktop services unavailable">Preview</span>}
      <button className="icon-btn plain" onClick={onAbout} aria-label="About and updates" title="About Nexonic AI & Check Updates">
        <Icon name="info" size={19} />
        {updateAvailable && <i className="upd-dot" />}
      </button>
      <button className="icon-btn plain" onClick={onTheme} aria-label={`Switch to ${theme === "dark" ? "light" : "dark"} theme`}>
        <Icon name={theme === "dark" ? "sun" : "moon"} size={20} />
      </button>
      <button className="icon-btn plain" aria-label="Notifications (none)"><Icon name="bell" size={20} /></button>
      <button className="avatar" aria-label="Profile"><Icon name="user" size={20} /></button>
    </header>
  );
}

const WAVE = [3, 5, 4, 7, 5, 9, 6, 11, 8, 14, 10, 17, 12, 20, 14, 18, 11, 15, 9, 12, 7, 10, 6, 8, 5, 6, 4, 5, 3, 4, 3, 2];

function Presence({
  name,
  side,
  state,
  mood,
  gaze,
  clip,
  reduced,
  onGreet,
}: {
  name: Agent;
  side: "left" | "right";
  state: PresenceState;
  mood: Mood;
  gaze: () => Gaze;
  clip: ClipName;
  reduced: boolean;
  onGreet: () => void;
}) {
  const video = CLIPS[name];
  const live = video !== null || ATLAS[name] !== null;
  // The waveform only moves for real audio; hover and moods never fake it.
  const liveMotion = state === "speaking" || state === "listening";
  const statusTag = state === "offline" ? "NO MODEL" : "LIVE";
  // With real video the face shows what it shows; the tag only names real states.
  const moodLabel = video
    ? clip === "chat"
      ? "Chatting"
      : mood === "attentive" || mood === "thinking"
        ? MOOD_LABEL[mood]
        : "Present"
    : MOOD_LABEL[mood];


  return (
    <div
      className={`presence ${side} ${name.toLowerCase()} ${live ? "is-live" : ""}`}
      data-mood={mood}
    >
      <div className="aura" />
      <button
        type="button"
        className="portrait-stage"
        onClick={onGreet}
        aria-label={`${name}, ${moodLabel.toLowerCase()}. Click to say hello.`}
      >
        <span className="holo-spot" aria-hidden="true" />
        <span className="holo-ring" aria-hidden="true" />
        {video ? (
          <VideoFace clips={video.clips} face={video.face} fallback={FALLBACK[name]} want={clip} reduced={reduced} label={`${name} portrait`} />
        ) : (
        <LiveFace
          atlas={ATLAS[name]}
          fallback={FALLBACK[name]}
          mood={mood}
          gaze={gaze}
          temperament={TEMPERAMENT[name]}
          reduced={reduced}
          label={`${name} portrait`}
        />
        )}
        <span className="holo-hud" aria-hidden="true">
          <b><RollText value={moodLabel} /></b>
        </span>
      </button>
      <div className="who">
        <SplitText as="h2" className="agent-name" text={name.toUpperCase()} delay={side === "left" ? 620 : 720} stagger={55} />
        <p className="agent-status">
          <span className={`dot ${state === "offline" ? "off" : "on"}`} />
          <RollText value={state === "offline" ? "Standby" : presenceLabel[state]} />
          <i>·</i>
          <ScrambleText text={statusTag} delay={400} className="mono" />
        </p>
        <div className="voice">
          {side === "left" && <button className="mic-sm" disabled aria-label={`${name} voice (microphone off)`}><Icon name="mic" size={16} /></button>}
          <div className="wave" data-live={liveMotion} aria-hidden="true">
            {WAVE.map((h, i) => <i key={i} style={{ height: `${h + 2}px`, ["--wi" as string]: i }} />)}
          </div>
          {side === "right" && <button className="mic-sm" disabled aria-label={`${name} voice (microphone off)`}><Icon name="mic" size={16} /></button>}
        </div>
      </div>
    </div>
  );
}

function Panel({ icon, title, meta, children }: { icon: IconName; title: string; meta?: ReactNode; children: ReactNode }) {
  return (
    <article className="panel" data-spot>
      <header><Icon name={icon} /><SplitText as="h3" text={title} delay={560} stagger={12} />{meta && <span className="meta">{meta}</span>}</header>
      {children}
    </article>
  );
}

function FocusPanel() {
  const [browserTasks, setBrowserTasks] = usePersisted<Task[]>("focus", []);
  const [desktopTasks, setDesktopTasks] = useState<Task[]>([]);
  // Edits are held back until the stored list has loaded, so a fast first
  // edit can never be overwritten by (or overwrite) the saved focus list.
  const [loaded, setLoaded] = useState(false);
  const [draft, setDraft] = useState("");
  const desktop = inDesktop();
  const tasks = desktop ? desktopTasks : browserTasks;

  useEffect(() => {
    if (!desktop) return;
    let active = true;
    void ipc.kvGet("focus").then((raw) => {
      if (!active) return;
      setLoaded(true);
      if (raw) {
        try {
          const parsed = JSON.parse(raw) as Task[];
          if (Array.isArray(parsed)) {
            setDesktopTasks(parsed);
            return;
          }
        } catch {
          /* ignore malformed stored value */
        }
      }
      try {
        const legacy = localStorage.getItem("nexonic:focus");
        if (legacy) {
          const parsed = JSON.parse(legacy) as Task[];
          if (Array.isArray(parsed) && parsed.length > 0) {
            setDesktopTasks(parsed);
            void ipc.kvSet("focus", JSON.stringify(parsed)).then(() => {
              localStorage.removeItem("nexonic:focus");
            });
          }
        }
      } catch {
        /* localStorage unavailable */
      }
    });
    return () => { active = false; };
  }, [desktop]);

  const updateTasks = (updater: (prev: Task[]) => Task[]) => {
    if (!desktop) {
      setBrowserTasks(updater);
      return;
    }
    if (!loaded) return;
    setDesktopTasks(updater);
  };

  // Persist after React commits the change (side effects stay out of updaters).
  const saved = useRef<string | null>(null);
  useEffect(() => {
    if (!desktop || !loaded) return;
    const json = JSON.stringify(desktopTasks);
    if (saved.current === null) { saved.current = json; return; }
    if (json === saved.current) return;
    saved.current = json;
    void ipc.kvSet("focus", json);
  }, [desktop, loaded, desktopTasks]);

  const done = tasks.filter((t) => t.done).length;
  const add = () => {
    const text = draft.trim();
    if (!text) return;
    updateTasks((t) => [...t, { id: crypto.randomUUID(), text, done: false }]);
    setDraft("");
  };
  return (
    <Panel icon="target" title="Today's Focus" meta={tasks.length ? <><CountUp value={done} />&nbsp;of&nbsp;<CountUp value={tasks.length} />&nbsp;completed</> : undefined}>
      {tasks.length > 0 && <div className="progress"><i style={{ width: `${(done / tasks.length) * 100}%` }} /></div>}
      <ul className="tasks">
        {tasks.map((t) => (
          <li key={t.id}>
            <label>
              <input type="checkbox" checked={t.done} onChange={() => updateTasks((all) => all.map((x) => x.id === t.id ? { ...x, done: !x.done } : x))} />
              <span>{t.text}</span>
            </label>
            <button className="ghost" aria-label={`Remove ${t.text}`} onClick={() => updateTasks((all) => all.filter((x) => x.id !== t.id))}><Icon name="x" size={14} /></button>
          </li>
        ))}
      </ul>
      {tasks.length === 0 && <p className="empty">Nothing planned yet. Add what matters today.</p>}
      <form className="add" onSubmit={(e) => { e.preventDefault(); add(); }}>
        <input value={draft} onChange={(e) => setDraft(e.target.value)} placeholder="Add a focus item" aria-label="New focus item" />
        <button type="submit" aria-label="Add"><Icon name="plus" size={16} /></button>
      </form>
    </Panel>
  );
}

function CollabPanel() {
  return (
    <Panel icon="users" title="Agent Collaboration" meta={<><span className="dot off" />Idle</>}>
      <div className="empty-state">
        <div className="duo"><span className="c-sharee">S</span><span className="c-nexa">N</span></div>
        <p>Sharee and Nexa start discussing once a model provider is connected in <b>Intelligence</b>.</p>
        <small>Every message here will be a real exchange, with sources and confidence.</small>
      </div>
    </Panel>
  );
}

function MemoryPanel({
  info,
  memories,
  onForget,
}: {
  info: MemorySummary | null;
  memories: MemoryView[];
  onForget: (id: number) => void;
}) {
  const count = info?.userVisible ?? null;
  return (
    <Panel icon="memory" title="Recent Memory Events" meta={count === null ? "—" : <><CountUp value={count} />&nbsp;stored</>}>
      {memories.length > 0 ? (
        <ul className="tasks mem-list">
          {memories.map((m) => (
            <li key={m.id}>
              <span className={`mem-tag ${m.domain}`}>{m.domain}</span>
              <span className="mem-text" title={m.text}>{m.text}</span>
              <button className="ghost" aria-label={`Forget memory ${m.id}`} onClick={() => onForget(m.id)}>
                <Icon name="x" size={14} />
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <div className="empty-state">
          <p>{count === null ? "Memory engine not reachable in this preview." : info?.storage_error ? "Encrypted storage could not open, so nothing is saved after you quit. Your existing data was not touched." : info?.persistent ? "No memories yet · encrypted SQLite active." : "No memories yet."}</p>
          <small>Private memories for Sharee and Nexa stay separate from shared memory. You can review, correct or delete anything.</small>
        </div>
      )}
    </Panel>
  );
}

const AWARE: Capability[] = ["Camera", "Microphone", "Screen", "Applications", "FilesAndFolders"];
const AWARE_ICON: Record<string, IconName> = { Camera: "camera", Microphone: "mic", Screen: "screen", Applications: "apps", FilesAndFolders: "folder" };

function AwarenessPanel({ perms, setPerms, sys }: { perms: PermissionsView | null; setPerms: (p: PermissionsView | null) => void; sys: SystemInfo | null }) {
  const granted = useMemo(() => new Map(perms?.items.map((i) => [i.capability, i.granted])), [perms]);
  return (
    <Panel icon="radar" title="Awareness & System" meta={perms?.stopped
      ? <button className="panel-stop resume" onClick={() => void ipc.resume().then(setPerms)}>Stopped · Resume</button>
      : <button className="panel-stop" disabled={!perms} title="Emergency stop: revoke everything" aria-label="Emergency stop" onClick={() => void ipc.emergencyStop().then(setPerms)}><Icon name="stop" size={12} /></button>}>
      <ul className="aware">
        {AWARE.map((c) => {
          const on = granted.get(c) ?? false;
          return (
            <li key={c}>
              <Icon name={AWARE_ICON[c]!} size={16} />
              <span>{capabilityLabel[c]}</span>
              <button className={`pill ${on ? "on" : ""}`} role="switch" aria-checked={on} disabled={!perms}
                aria-label={`${capabilityLabel[c]} access`}
                onClick={() => void ipc.setPermission(c, !on).then(setPerms)}>{on ? "Active" : "Off"}</button>
            </li>
          );
        })}
        <li className="local">
          <Icon name="chip" size={16} /><span>Local Processing</span>
          <em><span className={`dot ${sys ? "" : "off"}`} />{sys ? `On (${chipShort(sys.chip)})` : "—"}</em>
          <Icon name="chev" size={14} />
        </li>
      </ul>
    </Panel>
  );
}

function Dock({ onDraft, onSend }: { onDraft: (text: string) => void; onSend: () => void }) {
  const [text, setText] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const hint = useCycle(["Talk to Sharee, Nexa, or both…", "Ask Nexa to plan your morning…", "Ask Sharee to review your project…", "Ask both to debate an idea…"], 4200);
  return (
    <footer className="dock">
      <div className="talk-wrap">
        <form className="talk" onSubmit={(e) => { e.preventDefault(); if (text.trim()) { onSend(); setNote("Connect a model provider in Intelligence to talk to Sharee and Nexa."); } }}>
          <Icon name="wave" size={24} />
          <span className="talk-field">
            <input value={text} onChange={(e) => { setText(e.target.value); onDraft(e.target.value); setNote(null); }} aria-label="Message to Sharee and Nexa" />
            {!text && <span className="talk-hint" aria-hidden="true"><RollText value={hint} /></span>}
          </span>
          <button type="button" className="mic" aria-label="Voice (microphone permission off)" disabled><Icon name="mic" size={22} /></button>
        </form>
        <button className="focus-btn" disabled aria-label="Point at something on screen (arrives with screen awareness)"><Icon name="crosshair" size={22} /></button>
      </div>
      <div className="quick">
        {([["folder", "Open File"], ["search", "Search"], ["spark", "Create"], ["play", "Automate"]] as [IconName, string][]).map(([i, l]) => (
          <button key={l} className="quick-btn" data-spot disabled title="Arrives with its milestone"><Icon name={i} size={20} />{l}</button>
        ))}
      </div>
      {note && <p className="dock-note" role="status">{note}</p>}
    </footer>
  );
}

function Inspector({ node, onClose }: { node: (typeof NODES)[number]; onClose: () => void }) {
  return (
    <aside className="inspector" role="dialog" aria-label={`${node.id} inspector`}>
      <header><Icon name={node.icon} /><h3>{node.id}</h3><button className="ghost" onClick={onClose} aria-label="Close"><Icon name="x" /></button></header>
      <dl>
        <dt>Status</dt><dd>{node.status}</dd>
        <dt>Powered by</dt><dd>{node.owner}</dd>
      </dl>
      <p className="muted">This node lights up only for real events from its service. Idle drift in the core is decorative.</p>
    </aside>
  );
}

function AboutModal({
  sys,
  autoUpdate,
  onAutoUpdate,
  updateInfo,
  onCheckUpdates,
  onClose,
}: {
  sys: SystemInfo | null;
  autoUpdate: boolean;
  onAutoUpdate: (v: boolean) => void;
  updateInfo: UpdateStatus | null;
  onCheckUpdates: () => void;
  onClose: () => void;
}) {
  return (
    <div className="scrim" onClick={onClose}>
      <div className="about-dialog" role="dialog" aria-label="About Nexonic AI" onClick={(e) => e.stopPropagation()}>
        <header>
          <Mark />
          <div>
            <h3>Nexonic AI</h3>
            <small>Built by Nexonic Technologies · v{sys?.appVersion ?? "0.1.0"}</small>
          </div>
          <button className="ghost" onClick={onClose} aria-label="Close"><Icon name="x" /></button>
        </header>
        <p className="about-lead">
          Two independent digital minds — <b>Sharee</b> and <b>Nexa</b> — built around privacy-by-default engineering,
          encrypted local memory, and on-device Apple Silicon intelligence.
        </p>
        <dl className="about-meta">
          <dt>Company</dt><dd>Nexonic Technologies (founded by Shahrukh Khalid)</dd>
          <dt>Website</dt><dd><a href="https://nexonictechnologies.com/" target="_blank" rel="noreferrer">nexonictechnologies.com <Icon name="external" size={12} /></a></dd>
          <dt>Open Source</dt><dd><a href="https://github.com/nexonic-technologies/Nexonic-AI" target="_blank" rel="noreferrer">github.com/nexonic-technologies/Nexonic-AI <Icon name="external" size={12} /></a></dd>
          <dt>Licence</dt><dd>AGPL-3.0-only (character artwork © Nexonic Technologies)</dd>
        </dl>
        <div className="about-updates">
          <div>
            <b>Software Updates</b>
            <small>{updateInfo ? updateInfo.message : "Automatic check looks for signed GitHub releases on launch."}</small>
          </div>
          <div className="about-actions">
            <label className="auto-chk">
              <input type="checkbox" checked={autoUpdate} onChange={(e) => onAutoUpdate(e.target.checked)} />
              <span>Auto-check</span>
            </label>
            <button className="secondary" onClick={onCheckUpdates}>
              <Icon name="refresh" size={14} />Check for Updates
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function CommandPalette({
  onClose,
  onSection,
  onMode,
  onAbout,
}: {
  onClose: () => void;
  onSection: (s: Section) => void;
  onMode: (m: Mode) => void;
  onAbout: () => void;
}) {
  const [q, setQ] = useState("");
  const items = [
    ...NAV.map((n) => ({ key: `s-${n.id}`, label: `Go to ${n.title}`, run: () => onSection(n.id) })),
    ...MODES.map((m) => ({ key: `m-${m.id}`, label: `Switch to ${m.label}`, run: () => onMode(m.id) })),
    { key: "about-updates", label: "About Nexonic AI & Check Updates", run: onAbout },
  ].filter((i) => i.label.toLowerCase().includes(q.toLowerCase()));
  return (
    <div className="scrim" onClick={onClose}>
      <div className="palette" role="dialog" aria-label="Command palette" onClick={(e) => e.stopPropagation()}>
        <input autoFocus value={q} onChange={(e) => setQ(e.target.value)} placeholder="Type a command…" aria-label="Command"
          onKeyDown={(e) => { if (e.key === "Enter" && items[0]) items[0].run(); }} />
        <ul>{items.map((i) => <li key={i.key}><button onClick={i.run}>{i.label}</button></li>)}</ul>
      </div>
    </div>
  );
}
