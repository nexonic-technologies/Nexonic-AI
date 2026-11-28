/** Typed bridge to the Rust core. Outside the desktop runtime (plain browser
 *  preview) calls resolve to `null` so the UI can say so instead of faking data. */

import { Channel, invoke } from "@tauri-apps/api/core";

export interface SystemInfo {
  chip: string;
  cpuCores: number;
  memoryBytes: number;
  osName: string;
  osVersion: string;
  arch: string;
  appVersion: string;
}

export interface PermissionsView {
  stopped: boolean;
  items: { capability: Capability; granted: boolean }[];
}

export interface MemorySummary {
  userVisible: number;
  encrypted: boolean;
  persistent: boolean;
  /** Why the encrypted on-disk store could not open (app then runs on a temporary store). */
  storage_error?: string | null;
}

export interface MemoryView {
  id: number;
  domain: "sharee" | "nexa" | "collective";
  kind: string;
  text: string;
  source: string;
  confidence: number;
  neverShareWith: string[];
  createdAt: number;
}

export interface UpdateStatus {
  currentVersion: string;
  latestVersion: string | null;
  updateAvailable: boolean;
  releaseUrl: string;
  websiteUrl: string;
  message: string;
}

export interface ModelEntry {
  id: string;
  name: string;
  isFree: boolean;
  contextLength: number;
}

export interface ProviderStatus {
  provider: string;
  hasKey: boolean;
  keyHint: string | null;
  allowPaid: boolean;
  preferredModel: string | null;
  freeCount: number;
  totalCount: number;
  cooldownCount: number;
  models: ModelEntry[];
}

export interface CompletionResult {
  model: string;
  content: string;
  attempts: number;
}

export type Capability =
  | "Camera"
  | "Microphone"
  | "Screen"
  | "Applications"
  | "WebsiteContext"
  | "FilesAndFolders"
  | "ComputerAutomation"
  | "BackgroundReasoning"
  | "ProactiveVoice"
  | "AutomaticMemoryLearning"
  | "AutomaticSkillAcquisition"
  | "InterAgentCommunication"
  | "CloudContextSharing"
  | "RawRecordingRetention";

export interface LocalBrain {
  name: string;
  licence: string;
  sizeBytes: number;
  installed: boolean;
  supported: boolean;
  loaded: boolean;
  contextTokens: number | null;
}

export interface BrainProvider {
  id: string;
  name: string;
  free: boolean;
  note: string;
  keyUrl: string;
  defaultModel: string;
  model: string;
  hasKey: boolean;
}

export interface BrainStatus {
  /** "local" or a provider id. */
  active: string;
  ready: boolean;
  local: LocalBrain;
  providers: BrainProvider[];
}

export type ChatRole = "user" | "assistant";
export interface ChatTurn {
  role: ChatRole;
  content: string;
}
export type ReplyEvent = { kind: "start"; brain: string; remembered: number } | { kind: "token"; text: string };

export const inDesktop = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  if (!inDesktop()) return null;
  return invoke<T>(cmd, args);
}

export const ipc = {
  systemInfo: () => call<SystemInfo>("system_info"),
  permissions: () => call<PermissionsView>("permissions"),
  setPermission: (capability: Capability, granted: boolean) =>
    call<PermissionsView>("set_permission", { capability, granted }),
  emergencyStop: () => call<PermissionsView>("emergency_stop"),
  resume: () => call<PermissionsView>("resume"),
  memorySummary: () => call<MemorySummary>("memory_summary"),
  memoriesRecent: (limit = 20) => call<MemoryView[]>("memories_recent", { limit }),
  rememberNote: (
    text: string,
    domain: "sharee" | "nexa" | "collective" = "collective",
    kind = "semantic",
    neverShareWith: string[] = [],
  ) => call<MemoryView>("remember_note", { text, domain, kind, neverShareWith }),
  forgetMemory: (id: number) => call<void>("forget_memory", { id }),
  /** Whether conversations are remembered (off by default). */
  memoryConsent: () => call<boolean>("memory_consent"),
  memorySetConsent: (on: boolean) => call<boolean>("memory_set_consent", { on }),
  /** Keeps "remember that …" facts, and the exchange itself when allowed. */
  rememberExchange: (userText: string, replies: { agent: "Sharee" | "Nexa"; text: string }[]) =>
    call<number>("remember_exchange", { userText, replies }),
  kvGet: (key: string) => call<string | null>("kv_get", { key }),
  kvSet: (key: string, value: string) => call<void>("kv_set", { key, value }),
  checkUpdates: () => call<UpdateStatus>("check_updates"),
  providerStatus: () => call<ProviderStatus>("provider_status"),
  saveProviderKey: (apiKey: string) => call<ProviderStatus>("save_provider_key", { apiKey }),
  clearProviderKey: () => call<ProviderStatus>("clear_provider_key"),
  setRouterPolicy: (allowPaid: boolean, preferredModel: string | null) =>
    call<ProviderStatus>("set_router_policy", { allowPaid, preferredModel }),
  refreshModels: () => call<ProviderStatus>("refresh_models"),
  testCompletion: (prompt: string) => call<CompletionResult>("test_completion", { prompt }),
  brainStatus: () => call<BrainStatus>("brain_status"),
  brainSetActive: (id: string) => call<BrainStatus>("brain_set_active", { id }),
  brainSetModel: (id: string, model: string) => call<BrainStatus>("brain_set_model", { id, model }),
  brainSaveKey: (id: string, key: string) => call<BrainStatus>("brain_save_key", { id, key }),
  brainClearKey: (id: string) => call<BrainStatus>("brain_clear_key", { id }),
  brainStop: () => call<void>("brain_stop"),
  /** Streams one reply from Sharee or Nexa; resolves with the full text. */
  agentReply: (agent: "Sharee" | "Nexa", history: ChatTurn[], onEvent: (e: ReplyEvent) => void) => {
    const channel = new Channel<ReplyEvent>();
    channel.onmessage = onEvent;
    return call<string>("agent_reply", { agent, history, onEvent: channel });
  },
};

export const capabilityLabel: Record<Capability, string> = {
  Camera: "Camera",
  Microphone: "Microphone",
  Screen: "Screen",
  Applications: "Applications",
  WebsiteContext: "Website context",
  FilesAndFolders: "Files & Folders",
  ComputerAutomation: "Computer automation",
  BackgroundReasoning: "Background reasoning",
  ProactiveVoice: "Proactive voice",
  AutomaticMemoryLearning: "Memory learning",
  AutomaticSkillAcquisition: "Skill acquisition",
  InterAgentCommunication: "Agent-to-agent talk",
  CloudContextSharing: "Cloud context sharing",
  RawRecordingRetention: "Keep raw recordings",
};

export function formatBytes(n: number): string {
  const gb = n / 1024 ** 3;
  return gb >= 1 ? `${Math.round(gb)} GB` : `${Math.round(n / 1024 ** 2)} MB`;
}
