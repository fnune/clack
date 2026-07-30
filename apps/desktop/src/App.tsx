import { useCallback, useEffect, useState } from "react";
import {
  api,
  type Desktop,
  type Diagnosis,
  type Overlay,
  type Preview,
  type Settings,
  type Status,
} from "./ipc";
import { Notifications, useNotifications } from "./notifications";

const EMPTY_STATUS: Status = { running: false, failure: null };

/// IEC 5010, drawn as geometry rather than a full circle with the gap covered
/// over: the arc simply stops where the stem passes through.
function PowerRing() {
  return (
    <svg className="power" viewBox="0 0 48 48" aria-hidden="true">
      <path d="M 33 14.3 A 14 14 0 1 1 15 14.3" />
      <path d="M 24 7 V 23" />
    </svg>
  );
}

/// Stands in for the ring when there is nothing to switch on. A different shape
/// rather than a paler one, because a dimmer ring is the same control looking
/// slightly off and this is not that. The dot is a round cap on a line going
/// nowhere, so the whole sign stays a stroke and inherits the ring's weight.
function WarningSign() {
  return (
    <svg className="power caution" viewBox="0 0 48 48" aria-hidden="true">
      <path d="M 24 9 L 42 40 H 6 Z" />
      <path d="M 24 21 V 29" />
      <path d="M 24 34.5 h 0.01" />
    </svg>
  );
}

/// The path goes last so that the ellipsis eats it rather than the question, and
/// the title carries what the line could not fit.
function askedInFull(backup: string | null): string {
  return backup === null
    ? "Reset everything? A copy of your current settings is kept."
    : `Reset everything? A copy of your current settings is kept at ${backup}`;
}

/// A number that is being typed is text, not a number: half-written it is empty,
/// or a bare minus, or ends in a point, and `Number` turns the first of those into
/// zero. Committing that put a 0 in the box that every following digit had to
/// follow. So the text stands while it is being edited, only parseable values are
/// committed, and letting go hands the box back to whatever was stored.
function NumberField({
  id,
  value,
  unit,
  min,
  max,
  step,
  onCommit,
}: {
  readonly id: string;
  readonly value: number;
  readonly unit: string;
  readonly min: number;
  readonly max: number;
  readonly step?: number;
  readonly onCommit: (value: number) => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);

  return (
    <span className="field">
      <input
        id={id}
        type="number"
        min={min}
        max={max}
        step={step}
        value={draft ?? String(value)}
        onChange={(event) => {
          const text = event.target.value;
          setDraft(text);

          const parsed = Number(text);
          if (text !== "" && Number.isFinite(parsed)) onCommit(parsed);
        }}
        onBlur={() => setDraft(null)}
      />
      <span className="suffix">{unit}</span>
    </span>
  );
}

/// Pango describes a font as family, style and size in one string; the preview
/// only needs the family.
function pangoFamily(font: string): string {
  const family = font.replace(/\s+(bold|italic|oblique|light|medium|semi-?bold|\d+)$/gi, "").trim();

  return family === "" ? "sans-serif" : family;
}

/// The one thing someone opening this window wants to know: is Clack set up, and
/// is there anything for them to do about it? The headline is short enough for one
/// line of a narrow window; the reasoning is in the title, for anyone who wants
/// it.
function verdict(diagnosis: Diagnosis | null): Verdict {
  if (diagnosis === null) {
    return {
      headline: "Checking what Clack can do here",
      detail: "Looking for a keyboard to read and a way to draw the overlay.",
      ready: false,
    };
  }

  if (diagnosis.usable) {
    return {
      headline: "Clack can read your keyboard",
      detail: "Clack can read your keyboard and draw the overlay above other windows.",
      ready: true,
    };
  }

  if (diagnosis.keyboards === 0) {
    return diagnosis.ruleInstalled
      ? {
          headline: "No keyboard Clack can read",
          detail:
            "Reading keyboards is allowed, but none is readable yet. Log out and back in, or plug one in.",
          ready: false,
        }
      : {
          headline: "Clack needs your permission",
          detail: "Clack needs your permission to read your keyboard.",
          ready: false,
        };
  }

  if (!diagnosis.layerShell) {
    return {
      headline: "This desktop cannot show the overlay",
      detail:
        "This desktop has no layer shell, so the overlay cannot be drawn above other windows.",
      ready: false,
    };
  }

  return {
    headline: "This desktop shared no keyboard layout",
    detail: "This desktop did not share its keyboard layout, so keys cannot be named.",
    ready: false,
  };
}

type Verdict = { headline: string; detail: string; ready: boolean };

export function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [status, setStatus] = useState<Status>(EMPTY_STATUS);
  const [diagnosis, setDiagnosis] = useState<Diagnosis | null>(null);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [loading, setLoading] = useState(true);
  const { notes, notify, dismiss } = useNotifications();
  const [backup, setBackup] = useState<string | null>(null);
  const [confirmingReset, setConfirmingReset] = useState(false);

  const attempt = useCallback(
    async (action: () => Promise<void>) => {
      try {
        await action();
      } catch (error) {
        notify(error instanceof Error ? error.message : String(error), "error");
      }
    },
    [notify],
  );

  useEffect(() => {
    void attempt(async () => {
      const appearance = await api.appearance();

      if (appearance.accent !== null) {
        document.documentElement.style.setProperty("--accent", appearance.accent);
      }
      if (appearance.dark !== null) {
        document.documentElement.dataset.theme = appearance.dark ? "dark" : "light";
      }

      const loaded = await api.loadSettings();
      setSettings(loaded);
      setPreview(await api.preview(loaded));
      setStatus(await api.status());
      setDiagnosis(await api.diagnose());
      setBackup(await api.settingsBackupPath());
    }).finally(() => setLoading(false));
  }, [attempt]);

  useEffect(() => {
    const subscription = api.onStatus(setStatus);
    return () => {
      void subscription.then((unlisten) => {
        unlisten();
      });
    };
  }, []);

  /// A session can fail long after the click that started it, so its failure
  /// arrives with a status event rather than from a call anyone is awaiting.
  useEffect(() => {
    if (status.failure !== null) notify(status.failure, "error");
  }, [status.failure, notify]);

  if (settings === null) {
    return (
      <main>
        <p className="status">{loading ? "Loading." : "Clack could not read its settings."}</p>
        <Notifications notes={notes} onDismiss={dismiss} />
      </main>
    );
  }

  const save = (next: Settings) => {
    setSettings(next);
    void attempt(async () => {
      await api.saveSettings(next);
      setPreview(await api.preview(next));
    });
  };

  const editOverlay = (change: Partial<Overlay>) =>
    save({ ...settings, overlay: { ...settings.overlay, ...change } });

  const editDesktop = (change: Partial<Desktop>) =>
    save({ ...settings, desktop: { ...settings.desktop, ...change } });

  const toggle = () =>
    void attempt(async () => {
      setStatus(status.running ? await api.stop() : await api.start(settings));
    });

  const said = verdict(diagnosis);
  const stuck = diagnosis !== null && !diagnosis.usable;

  return (
    <main>
      <div className={status.running ? "frame running" : "frame"}>
        <button
          className="control"
          type="button"
          role="switch"
          aria-checked={status.running}
          aria-label="Show keystrokes"
          disabled={stuck && !status.running}
          title={stuck ? said.detail : "Show keystrokes"}
          onClick={toggle}
        >
          {stuck && !status.running ? <WarningSign /> : <PowerRing />}
        </button>
      </div>

      <section>
        <h2>Status</h2>
        <div className="list">
          <div className={said.ready ? "row verdict ready" : "row verdict"} title={said.detail}>
            <span className="headline">
              <span className="mark" aria-hidden="true" />
              <span className="said">{said.headline}</span>
            </span>
            <button
              className="linkish"
              type="button"
              disabled={diagnosis === null}
              title={
                diagnosis?.ruleInstalled === true
                  ? "Takes Clack's permission to read your keyboard away again."
                  : "Asks for your password once, so Clack may read your keyboard."
              }
              onClick={() =>
                void attempt(async () => {
                  setDiagnosis(
                    diagnosis?.ruleInstalled === true
                      ? await api.revokeAccess()
                      : await api.grantAccess(),
                  );
                })
              }
            >
              {diagnosis?.ruleInstalled === true ? "Revoke access" : "Allow access"}
            </button>
          </div>
        </div>
      </section>

      <section>
        <h2>Preview</h2>
        <div className="preview">
          <span className="pill" style={{ fontFamily: pangoFamily(settings.overlay.font) }}>
            {preview?.runs.map((run, index) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: a run has no identity beyond where it sits in the line
              <span className={run.kind} key={`${run.kind}-${index}-${run.text}`}>
                {run.text}
              </span>
            ))}
          </span>
        </div>
      </section>

      <section>
        <h2>Appearance</h2>
        <div className="list">
          <label className="row">
            <span>Font</span>
            <span className="value">
              <input
                type="text"
                value={settings.overlay.font}
                onChange={(event) => editOverlay({ font: event.target.value })}
              />
            </span>
          </label>

          <div className="row">
            <span>Edge</span>
            <span className="value">
              <span className="radios">
                {(["bottom", "top"] as const).map((edge) => (
                  <label key={edge}>
                    <input
                      type="radio"
                      name="anchor"
                      checked={settings.overlay.anchor === edge}
                      onChange={() => editOverlay({ anchor: edge })}
                    />
                    {edge === "top" ? "Top" : "Bottom"}
                  </label>
                ))}
              </span>
            </span>
          </div>

          <label className="row" htmlFor="margin">
            <span>Distance from edge</span>
            <span className="value">
              <NumberField
                id="margin"
                value={settings.overlay.margin}
                unit="px"
                min={0}
                max={4000}
                onCommit={(margin) => editOverlay({ margin })}
              />
            </span>
          </label>

          <label className="row" htmlFor="timeout">
            <span>Time on screen</span>
            <span className="value">
              <NumberField
                id="timeout"
                value={settings.overlay.timeout_seconds}
                unit="s"
                min={0.2}
                max={30}
                step={0.1}
                onCommit={(timeout_seconds) => editOverlay({ timeout_seconds })}
              />
            </span>
          </label>
        </div>
      </section>

      <section>
        <h2>Symbols</h2>
        <div className="list">
          <label className="row">
            <span>Draw keys as symbols</span>
            <span className="value">
              <span className="toggle">
                <input
                  type="checkbox"
                  checked={settings.overlay.symbols}
                  onChange={(event) => editOverlay({ symbols: event.target.checked })}
                />
              </span>
            </span>
          </label>

          {preview?.symbols.map((symbol) => (
            <label className="row" key={symbol.keysym}>
              <span>{symbol.label}</span>
              <span className="value symbol">
                <input
                  type="text"
                  value={settings.overlay.symbol_overrides[symbol.keysym] ?? symbol.glyph}
                  onChange={(event) =>
                    editOverlay({
                      symbol_overrides: {
                        ...settings.overlay.symbol_overrides,
                        [symbol.keysym]: event.target.value,
                      },
                    })
                  }
                />
              </span>
            </label>
          ))}
        </div>
      </section>

      <section>
        <h2>On launch</h2>
        <div className="list">
          <label className="row check">
            <span>Start in the tray, without this window</span>
            <input
              type="checkbox"
              checked={settings.desktop.start_hidden}
              onChange={(event) => editDesktop({ start_hidden: event.target.checked })}
            />
          </label>

          <label className="row check">
            <span>Start showing keystrokes straight away</span>
            <input
              type="checkbox"
              checked={settings.desktop.start_overlay}
              onChange={(event) => editDesktop({ start_overlay: event.target.checked })}
            />
          </label>
        </div>
      </section>

      <footer>
        {confirmingReset ? (
          <div className="confirm">
            <p className="ask" title={askedInFull(backup)}>
              Reset everything? A copy is kept at {backup ?? "a backup file"}
            </p>
            <button className="linkish" type="button" onClick={() => setConfirmingReset(false)}>
              Cancel
            </button>
            <button
              className="linkish destructive"
              type="button"
              onClick={() =>
                void attempt(async () => {
                  const defaults = await api.resetSettings();
                  setConfirmingReset(false);
                  setSettings(defaults);
                  setPreview(await api.preview(defaults));
                  notify("Settings reset to defaults.", "success");
                })
              }
            >
              Reset
            </button>
          </div>
        ) : (
          <button className="linkish" type="button" onClick={() => setConfirmingReset(true)}>
            Reset to defaults
          </button>
        )}
      </footer>

      <Notifications notes={notes} onDismiss={dismiss} />
    </main>
  );
}
