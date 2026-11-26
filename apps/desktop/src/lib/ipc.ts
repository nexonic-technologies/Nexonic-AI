/** Typed bridge to the Rust core. Outside the desktop runtime (plain browser
 *  preview) calls resolve to `null` so the UI can say so instead of faking data. */

import { invoke } from "@tauri-apps/api/core";

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
  kvGet: (key: string) => call<string | null>("kv_get", { key }),
  kvSet: (key: string, value: string) => call<void>("kv_set", { key, value }),
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
