import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { presenceLabel, type MotionLevel, type PresenceState, type Theme } from "@nexonic/design-system";
import { Icon, type IconName } from "./components/Icon";
import { NeuralCore } from "./components/NeuralCore";
import { CountUp, RollText, ScrambleText, SplitText, useCycle } from "./components/MotionText";
import { capabilityLabel, formatBytes, inDesktop, ipc, type Capability, type MemorySummary, type MemoryView, type PermissionsView, type SystemInfo } from "./lib/ipc";
import { usePersisted, useReducedMotion } from "./lib/store";
import shareeImg from "./assets/characters/sharee.webp";
import nexaImg from "./assets/characters/nexa.webp";

type Section = "presence" | "hub" | "memory" | "evolution" | "activity" | "computer" | "intelligence" | "settings";
type Mode = "world" | "collaborate" | "analyze" | "create" | "automate";
type Node = "Reasoning" | "Memory" | "Learning" | "Perception" | "Planning" | "Action";

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

const NODES: { id: Node; icon: IconName; pos: string; owner: string; status: string }[] = [
  { id: "Reasoning", icon: "spark", pos: "n-tl", owner: "Agent runtime (M16)", status: "Waiting for a model provider" },
  { id: "Perception", icon: "camera", pos: "n-tr", owner: "Awareness (M24–M27)", status: "Camera, microphone and screen are off" },
  { id: "Memory", icon: "memory", pos: "n-ml", owner: "Memory engine (M20)", status: "Isolation active · in-memory store" },
  { id: "Planning", icon: "target", pos: "n-mr", owner: "Agent runtime (M16)", status: "Waiting for a model provider" },
  { id: "Learning", icon: "evolution", pos: "n-bl", owner: "Learning (M36–M39)", status: "Not started" },
  { id: "Action", icon: "play", pos: "n-br", owner: "Computer control (M32)", status: "Automation permission is off" },
];

interface Task { id: string; text: string; done: boolean }

export function App() {
  const reduced = useReducedMotion();
  const [theme, setTheme] = usePersisted<Theme>("theme", "dark");
  const [motion] = usePersisted<MotionLevel>("motion", "balanced");
  const [section, setSection] = useState<Section>("presence");
  const [mode, setMode] = usePersisted<Mode>("mode", "world");
  const [node, setNode] = useState<Node | null>(null);
  const [hoverNode, setHoverNode] = useState<Node | null>(null);
  const [palette, setPalette] = useState(false);
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
              status: memoryInfo?.persistent
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
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") { e.preventDefault(); setPalette((p) => !p); }
      if (e.key === "Escape") { setPalette(false); setNode(null); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Agents have no model provider yet (M17), so both are honestly offline.
  const agentState: PresenceState = "offline";
  const activeMode = MODES.find((m) => m.id === mode) ?? MODES[0]!;

  return (
    <div className="app" data-mode={mode}>
      <div className="titlebar" data-tauri-drag-region />
      <Header onSearch={() => setPalette(true)} theme={theme} onTheme={() => setTheme(theme === "dark" ? "light" : "dark")} />
      <Sidebar section={section} onSection={setSection} sys={sys} />
      <main className="main">
        <section className="stage" aria-label="Presence" data-theme="dark">
          <Presence name="Sharee" side="left" state={agentState} />
          <div className="core">
            <NeuralCore motion={motion} reduced={reduced} activity={0} focus={hoverNode ?? node} />
            <svg className="links" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
              <path d="M22 13 H30 L38 22" /><path d="M78 13 H70 L62 22" />
              <path d="M16 36 H26 L33 42" /><path d="M84 36 H74 L67 42" />
              <path d="M18 72 H28 L35 64" /><path d="M82 72 H72 L65 64" />
            </svg>
            <div className="core-label">
              <Icon name="wave" size={26} />
              <strong><SplitText text="COLLECTIVE" delay={300} stagger={30} /><SplitText text="INTELLIGENCE" delay={420} stagger={30} /></strong>
              <span><ScrambleText text="SHAREE" delay={700} className="c-sharee" /> × <ScrambleText text="NEXA" delay={760} /></span>
              <b className="inf">∞</b>
              <em><RollText value="IDLE" /></em>
            </div>
            {NODES.map((n) => (
              <button key={n.id} className={`node ${n.pos}`} onClick={() => setNode(n.id)} aria-pressed={node === n.id}
                onPointerEnter={() => setHoverNode(n.id)} onPointerLeave={() => setHoverNode(null)} onFocus={() => setHoverNode(n.id)} onBlur={() => setHoverNode(null)}>
                <Icon name={n.icon} size={16} />{n.id}
              </button>
            ))}
          </div>
          <Presence name="Nexa" side="right" state={agentState} />
          <nav className="modes" aria-label="Workspace mode" ref={modesRef}>
            <span className="mode-ink" aria-hidden="true" />
            {MODES.map((m) => (
              <button key={m.id} className="mode" aria-pressed={m.id === mode} onClick={() => setMode(m.id)}>
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
        <Dock />
      </main>
      {node && <Inspector node={nodes.find((n) => n.id === node)!} onClose={() => setNode(null)} />}
      {palette && <CommandPalette onClose={() => setPalette(false)} onSection={(s) => { setSection(s); setPalette(false); }} onMode={(m) => { setMode(m); setPalette(false); }} />}
      {!inDesktop() && <div className="preview-badge">Browser preview · desktop services unavailable</div>}
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
      <div className="sys-card" title={sys ? `${sys.chip} · ${formatBytes(sys.memoryBytes)} unified memory · ${sys.cpuCores} cores · ${sys.osName} ${sys.osVersion}` : undefined}>
        <div className="chip"><span className="apple" aria-hidden="true"></span>{sys ? chipShort(sys.chip) : "—"}</div>
        <div>
          <b>Local System</b>
          <small>{sys ? `Nexonic Core v${sys.appVersion}` : "Detecting…"}</small>
          <small className={sys ? "ok" : "muted"}><span className={`dot ${sys ? "" : "off"}`} />{sys ? "Running" : "Not connected"}</small>
        </div>
        <Icon name="chart" size={16} className="bars" />
      </div>
      <button className="agents-chip" aria-label="Agent availability">
        <span className="duo-av"><img src={shareeImg} alt="" /><img src={nexaImg} alt="" /></span>
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
    <svg width="34" height="34" viewBox="0 0 34 34" aria-hidden="true">
      <defs>
        <linearGradient id="mk" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="var(--sharee)" /><stop offset="1" stopColor="var(--nexa)" />
        </linearGradient>
      </defs>
      <path d="M6 28V6l11 13V6M17 28V15l11 13V6" fill="none" stroke="url(#mk)" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function Header({ onSearch, theme, onTheme }: { onSearch: () => void; theme: Theme; onTheme: () => void }) {
  return (
    <header className="header">
      <div className="brand">
        <Mark />
        <div><SplitText as="b" text="NEXONIC" delay={60} stagger={40} /><span>INTELLIGENCE</span></div>
      </div>
      <div className="title">
        <SplitText as="h1" text="My Intelligence Environment" delay={180} stagger={14} />
        <p>{["Two Minds", "One Collective Intelligence", "Built Around You"].map((w, i) => (
          <span key={w} className="tag-word" style={{ animationDelay: `${520 + i * 140}ms` }}>{i > 0 && <i>·</i>}{w}</span>
        ))}</p>
      </div>
      <button className="search" onClick={onSearch}>
        <Icon name="search" /> <span>Search memories, files, tasks…</span> <kbd>⌘ K</kbd>
      </button>
      <button className="icon-btn plain" onClick={onTheme} aria-label={`Switch to ${theme === "dark" ? "light" : "dark"} theme`}>
        <Icon name={theme === "dark" ? "sun" : "moon"} size={20} />
      </button>
      <button className="icon-btn plain" aria-label="Notifications (none)"><Icon name="bell" size={22} /></button>
      <button className="avatar" aria-label="Profile"><Icon name="user" size={20} /></button>
    </header>
  );
}

const WAVE = [3, 5, 4, 7, 5, 9, 6, 11, 8, 14, 10, 17, 12, 20, 14, 18, 11, 15, 9, 12, 7, 10, 6, 8, 5, 6, 4, 5, 3, 4, 3, 2];

function Presence({ name, side, state }: { name: "Sharee" | "Nexa"; side: "left" | "right"; state: PresenceState }) {
  return (
    <div className={`presence ${side} ${name.toLowerCase()}`}>
      <div className="aura" />
      <img className="portrait" src={name === "Sharee" ? shareeImg : nexaImg} alt={`${name} portrait`} draggable={false} />
      <div className="who">
        <SplitText as="h2" className="agent-name" text={name.toUpperCase()} delay={side === "left" ? 620 : 720} stagger={55} />
        <p className="agent-status"><span className={`dot ${state === "offline" ? "off" : "on"}`} /><RollText value={state === "offline" ? "Standby" : presenceLabel[state]} /><i>·</i><ScrambleText text={state === "offline" ? "NO MODEL CONNECTED" : "LIVE"} delay={900} className="mono" /></p>
        <div className="voice">
          {side === "left" && <button className="mic-sm" disabled aria-label={`${name} voice (microphone off)`}><Icon name="mic" size={16} /></button>}
          <div className="wave" data-live={state === "speaking" || state === "listening"} aria-hidden="true">
            {WAVE.map((h, i) => <i key={i} style={{ height: `${h + 2}px` }} />)}
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
  const [draft, setDraft] = useState("");
  const desktop = inDesktop();
  const tasks = desktop ? desktopTasks : browserTasks;

  useEffect(() => {
    if (!desktop) return;
    let active = true;
    void ipc.kvGet("focus").then((raw) => {
      if (!active) return;
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
      // One-time migration from legacy unencrypted localStorage into encrypted SQLite kv.
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
    setDesktopTasks((prev) => {
      const next = updater(prev);
      void ipc.kvSet("focus", JSON.stringify(next));
      return next;
    });
  };

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
          <p>{count === null ? "Memory engine not reachable in this preview." : info?.persistent ? "No memories yet · encrypted SQLite active." : "No memories yet."}</p>
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

function Dock() {
  const [text, setText] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const hint = useCycle(["Talk to Sharee, Nexa, or both…", "Ask Nexa to plan your morning…", "Ask Sharee to review your project…", "Ask both to debate an idea…"], 4200);
  return (
    <footer className="dock">
      <div className="talk-wrap">
        <form className="talk" onSubmit={(e) => { e.preventDefault(); if (text.trim()) setNote("Connect a model provider in Intelligence to talk to Sharee and Nexa."); }}>
          <Icon name="wave" size={24} />
          <span className="talk-field">
            <input value={text} onChange={(e) => { setText(e.target.value); setNote(null); }} aria-label="Message to Sharee and Nexa" />
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

function CommandPalette({ onClose, onSection, onMode }: { onClose: () => void; onSection: (s: Section) => void; onMode: (m: Mode) => void }) {
  const [q, setQ] = useState("");
  const items = [
    ...NAV.map((n) => ({ key: `s-${n.id}`, label: `Go to ${n.title}`, run: () => onSection(n.id) })),
    ...MODES.map((m) => ({ key: `m-${m.id}`, label: `Switch to ${m.label}`, run: () => onMode(m.id) })),
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
