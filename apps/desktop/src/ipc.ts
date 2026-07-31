import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Anchor = "top" | "bottom";

export type Overlay = {
  font: string;
  margin: number;
  anchor: Anchor;
  timeout_seconds: number;
  symbols: boolean;
  symbol_overrides: Record<string, string>;
};

export type Desktop = {
  start_hidden: boolean;
  start_overlay: boolean;
  decorations: boolean;
};

export type PreviewRun = {
  text: string;
  kind: "text" | "chord" | "special";
};

export type SymbolEntry = {
  keysym: string;
  label: string;
  glyph: string;
};

export type Preview = {
  runs: PreviewRun[];
  symbols: SymbolEntry[];
};

export type Settings = {
  overlay: Overlay;
  desktop: Desktop;
};

export type Status = {
  running: boolean;
  failure: string | null;
};

export type SettingsFile = {
  path: string;
  writable: boolean;
};

export type Diagnosis = {
  keyboards: number;
  denied: number;
  layerShell: boolean;
  keymap: boolean;
  usable: boolean;
  summary: string;
  ruleInstalled: boolean;
};

export type Appearance = {
  accent: string | null;
  dark: boolean | null;
};

const STATUS_EVENT = "clack://status";
const APPEARANCE_EVENT = "clack://appearance";

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
  const overlay = fields(source.overlay);
  const desktop = fields(source.desktop);

  return {
    overlay: {
      font: text(overlay, "font"),
      margin: count(overlay, "margin"),
      anchor: anchor(overlay, "anchor"),
      timeout_seconds: count(overlay, "timeout_seconds"),
      symbols: flag(overlay, "symbols"),
      symbol_overrides: table(overlay, "symbol_overrides"),
    },
    desktop: {
      start_hidden: flag(desktop, "start_hidden"),
      start_overlay: flag(desktop, "start_overlay"),
      decorations: flag(desktop, "decorations"),
    },
  };
}

function table(source: Record<string, unknown>, name: string): Record<string, string> {
  const value = fields(source[name]);
  const entries = Object.entries(value);

  if (entries.some(([, glyph]) => typeof glyph !== "string")) {
    throw new ShapeError(name, value);
  }

  return Object.fromEntries(entries) as Record<string, string>;
}

export function parsePreview(value: unknown): Preview {
  const source = fields(value);
  const runs = source.runs;
  const symbols = source.symbols;

  if (!Array.isArray(runs) || !Array.isArray(symbols)) {
    throw new ShapeError("runs or symbols", source);
  }

  return {
    runs: runs.map((run) => {
      const entry = fields(run);
      const kind = text(entry, "kind");
      if (kind !== "text" && kind !== "chord" && kind !== "special") {
        throw new ShapeError("kind", kind);
      }
      return { text: text(entry, "text"), kind };
    }),
    symbols: symbols.map((symbol) => {
      const entry = fields(symbol);
      return {
        keysym: text(entry, "keysym"),
        label: text(entry, "label"),
        glyph: text(entry, "glyph"),
      };
    }),
  };
}

export function parseStatus(value: unknown): Status {
  const source = fields(value);
  return {
    running: flag(source, "running"),
    failure: optionalText(source, "failure"),
  };
}

export function parseSettingsFile(value: unknown): SettingsFile {
  const source = fields(value);
  return {
    path: text(source, "path"),
    writable: flag(source, "writable"),
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
    ruleInstalled: flag(source, "ruleInstalled"),
  };
}

function optionalFlag(source: Record<string, unknown>, name: string): boolean | null {
  const value = source[name];
  if (value === null || value === undefined) return null;
  if (typeof value !== "boolean") throw new ShapeError(name, value);
  return value;
}

export function parseAppearance(value: unknown): Appearance {
  const source = fields(value);
  return {
    accent: optionalText(source, "accent"),
    dark: optionalFlag(source, "dark"),
  };
}

export const api = {
  appearance: async (): Promise<Appearance> => parseAppearance(await invoke("desktop_appearance")),
  loadSettings: async (): Promise<Settings> => parseSettings(await invoke("load_settings")),
  saveSettings: async (settings: Settings): Promise<void> => {
    await invoke("save_settings", { settings });
  },
  resetSettings: async (): Promise<Settings> => parseSettings(await invoke("reset_settings")),
  settingsFile: async (): Promise<SettingsFile> => parseSettingsFile(await invoke("settings_file")),
  ready: async (): Promise<void> => {
    await invoke("ready");
  },
  settingsBackupPath: async (): Promise<string> => {
    const path = await invoke("settings_backup_path");
    if (typeof path !== "string") throw new ShapeError("settings_backup_path", path);
    return path;
  },
  start: async (settings: Settings): Promise<Status> =>
    parseStatus(await invoke("start", { settings })),
  stop: async (): Promise<Status> => parseStatus(await invoke("stop")),
  status: async (): Promise<Status> => parseStatus(await invoke("status")),
  diagnose: async (): Promise<Diagnosis> => parseDiagnosis(await invoke("diagnose")),
  preview: async (settings: Settings): Promise<Preview> =>
    parsePreview(await invoke("preview", { settings })),
  grantAccess: async (): Promise<Diagnosis> => parseDiagnosis(await invoke("grant_access")),
  revokeAccess: async (): Promise<Diagnosis> => parseDiagnosis(await invoke("revoke_access")),
  onStatus: (handle: (status: Status) => void) =>
    listen(STATUS_EVENT, (event) => {
      handle(parseStatus(event.payload));
    }),
  onAppearance: (handle: (appearance: Appearance) => void) =>
    listen(APPEARANCE_EVENT, (event) => {
      handle(parseAppearance(event.payload));
    }),
};
