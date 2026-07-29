import { useCallback, useEffect, useState } from "react";
import { api, type Diagnosis, type Overlay, type Settings, type Status, type Tray } from "./ipc";

const EMPTY_STATUS: Status = { running: false, failure: null };

function describeKeyboards(diagnosis: Diagnosis | null): string {
  if (diagnosis === null) return "Checking";
  if (diagnosis.keyboards > 0) return `${diagnosis.keyboards} readable`;
  if (diagnosis.denied > 0) return `None, ${diagnosis.denied} denied`;
  return "None found";
}

export function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [status, setStatus] = useState<Status>(EMPTY_STATUS);
  const [diagnosis, setDiagnosis] = useState<Diagnosis | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  const attempt = useCallback(async (action: () => Promise<void>) => {
    try {
      setProblem(null);
      await action();
    } catch (error) {
      setProblem(error instanceof Error ? error.message : String(error));
    }
  }, []);

  useEffect(() => {
    void attempt(async () => {
      const appearance = await api.appearance();

      if (appearance.accent !== null) {
        document.documentElement.style.setProperty("--accent", appearance.accent);
      }
      if (appearance.dark !== null) {
        document.documentElement.dataset.theme = appearance.dark ? "dark" : "light";
      }

      setSettings(await api.loadSettings());
      setStatus(await api.status());
      setDiagnosis(await api.diagnose());
    });
  }, [attempt]);

  useEffect(() => {
    const subscription = api.onStatus(setStatus);
    return () => {
      void subscription.then((unlisten) => {
        unlisten();
      });
    };
  }, []);

  if (settings === null) {
    return (
      <main>
        <p className={problem === null ? "status" : "problem"}>{problem ?? "Loading."}</p>
      </main>
    );
  }

  const save = (next: Settings) => {
    setSettings(next);
    void attempt(() => api.saveSettings(next));
  };

  const editOverlay = (change: Partial<Overlay>) =>
    save({ ...settings, overlay: { ...settings.overlay, ...change } });

  const editTray = (change: Partial<Tray>) =>
    save({ ...settings, tray: { ...settings.tray, ...change } });

  const toggle = () =>
    void attempt(async () => {
      setStatus(status.running ? await api.stop() : await api.start(settings));
    });

  return (
    <main>
      {problem !== null && <p className="problem">{problem}</p>}
      {status.failure !== null && <p className="problem">{status.failure}</p>}

      <div className={status.running ? "frame running" : "frame"}>
        <button
          className="control"
          type="button"
          role="switch"
          aria-checked={status.running}
          onClick={toggle}
        >
          <span className="name">Show keystrokes</span>
          <span className="switch" aria-hidden="true" />
        </button>
      </div>

      <section>
        <h2>Access</h2>
        <div className="list">
          <div className="row">
            <span>Keyboards</span>
            <span className="value reading">{describeKeyboards(diagnosis)}</span>
          </div>
          <div className="row">
            <span>Overlay</span>
            <span className="value reading">
              {diagnosis === null
                ? "Checking"
                : diagnosis.layerShell
                  ? "Layer shell"
                  : "Not supported here"}
            </span>
          </div>
          <div className="row">
            <span>Keymap</span>
            <span className="value reading">
              {diagnosis === null
                ? "Checking"
                : diagnosis.keymap
                  ? "From the compositor"
                  : "Unavailable"}
            </span>
          </div>
        </div>

        {diagnosis?.canGrantAccess === true && (
          <>
            <p className="status">
              Clack reads keyboards from <code>/dev/input</code>, which needs permission. This
              installs <code>{diagnosis.rulePath}</code>, granting read access to whoever is logged
              in on the active seat, revoked at logout. You will be asked for your password once.
            </p>
            <button
              className="quiet"
              type="button"
              onClick={() =>
                void attempt(async () => {
                  setDiagnosis(await api.grantAccess());
                })
              }
            >
              Grant access
            </button>
          </>
        )}
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

          <label className="row">
            <span>Distance from edge</span>
            <span className="value">
              <span className="field">
                <input
                  type="number"
                  min={0}
                  max={4000}
                  value={settings.overlay.margin}
                  style={{ width: `${String(settings.overlay.margin).length}ch` }}
                  onChange={(event) => editOverlay({ margin: Number(event.target.value) })}
                />
                <span className="suffix">px</span>
              </span>
            </span>
          </label>

          <label className="row">
            <span>Time on screen</span>
            <span className="value">
              <span className="field">
                <input
                  type="number"
                  min={0.2}
                  max={30}
                  step={0.1}
                  value={settings.overlay.timeout_seconds}
                  style={{ width: `${String(settings.overlay.timeout_seconds).length}ch` }}
                  onChange={(event) => editOverlay({ timeout_seconds: Number(event.target.value) })}
                />
                <span className="suffix">s</span>
              </span>
            </span>
          </label>
        </div>
      </section>

      <section>
        <h2>On launch</h2>
        <div className="list">
          <label className="row check">
            <span>Start in the tray, without this window</span>
            <input
              type="checkbox"
              checked={settings.tray.start_hidden}
              onChange={(event) => editTray({ start_hidden: event.target.checked })}
            />
          </label>

          <label className="row check">
            <span>Start showing keystrokes straight away</span>
            <input
              type="checkbox"
              checked={settings.tray.start_overlay}
              onChange={(event) => editTray({ start_overlay: event.target.checked })}
            />
          </label>
        </div>
      </section>
    </main>
  );
}
