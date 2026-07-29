import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Anchor = "top" | "bottom";

export type Settings = {
  font: string;
  margin: number;
  timeoutSeconds: number;
  anchor: Anchor;
  startHidden: boolean;
  startOverlay: boolean;
};

export type Status = {
  running: boolean;
  paused: boolean;
  failure: string | null;
};

export type Diagnosis = {
  keyboards: number;
  denied: number;
  layerShell: boolean;
  keymap: boolean;
  usable: boolean;
  summary: string;
  canGrantAccess: boolean;
  rulePath: string;
  ruleText: string;
};

const STATUS_EVENT = "clack://status";

class ShapeError extends Error {
  constructor(field: string, value: unknown) {
    super(`the backend sent an unexpected value for ${field}: ${JSON.stringify(value)}`);
    this.name = "ShapeError";
  }
}

function fields(value: unknown): Record<string, unknown> {
  if (typeof value !== "object" || value === null) {
    throw new ShapeError("the whole response", value);
  }
  return value as Record<string, unknown>;
}

function text(source: Record<string, unknown>, name: string): string {
  const value = source[name];
  if (typeof value !== "string") throw new ShapeError(name, value);
  return value;
}

function count(source: Record<string, unknown>, name: string): number {
  const value = source[name];
  if (typeof value !== "number" || !Number.isFinite(value)) throw new ShapeError(name, value);
  return value;
}

function flag(source: Record<string, unknown>, name: string): boolean {
  const value = source[name];
  if (typeof value !== "boolean") throw new ShapeError(name, value);
  return value;
}

function anchor(source: Record<string, unknown>, name: string): Anchor {
  const value = source[name];
  if (value !== "top" && value !== "bottom") throw new ShapeError(name, value);
  return value;
}

function optionalText(source: Record<string, unknown>, name: string): string | null {
  const value = source[name];
  if (value === null || value === undefined) return null;
  if (typeof value !== "string") throw new ShapeError(name, value);
  return value;
}

export function parseSettings(value: unknown): Settings {
  const source = fields(value);
  return {
    font: text(source, "font"),
    margin: count(source, "margin"),
    timeoutSeconds: count(source, "timeoutSeconds"),
    anchor: anchor(source, "anchor"),
    startHidden: flag(source, "startHidden"),
    startOverlay: flag(source, "startOverlay"),
  };
}

export function parseStatus(value: unknown): Status {
  const source = fields(value);
  return {
    running: flag(source, "running"),
    paused: flag(source, "paused"),
    failure: optionalText(source, "failure"),
  };
}

export function parseDiagnosis(value: unknown): Diagnosis {
  const source = fields(value);
  return {
    keyboards: count(source, "keyboards"),
    denied: count(source, "denied"),
    layerShell: flag(source, "layerShell"),
    keymap: flag(source, "keymap"),
    usable: flag(source, "usable"),
    summary: text(source, "summary"),
    canGrantAccess: flag(source, "canGrantAccess"),
    rulePath: text(source, "rulePath"),
    ruleText: text(source, "ruleText"),
  };
}

export const api = {
  loadSettings: async (): Promise<Settings> => parseSettings(await invoke("load_settings")),
  saveSettings: async (settings: Settings): Promise<void> => {
    await invoke("save_settings", { settings });
  },
  start: async (settings: Settings): Promise<Status> =>
    parseStatus(await invoke("start", { settings })),
  stop: async (): Promise<Status> => parseStatus(await invoke("stop")),
  setPaused: async (paused: boolean): Promise<Status> =>
    parseStatus(await invoke("set_paused", { paused })),
  status: async (): Promise<Status> => parseStatus(await invoke("status")),
  diagnose: async (): Promise<Diagnosis> => parseDiagnosis(await invoke("diagnose")),
  grantAccess: async (): Promise<Diagnosis> => parseDiagnosis(await invoke("grant_access")),
  onStatus: (handle: (status: Status) => void) =>
    listen(STATUS_EVENT, (event) => {
      handle(parseStatus(event.payload));
    }),
};
