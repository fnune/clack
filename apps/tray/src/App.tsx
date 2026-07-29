import { useCallback, useEffect, useState } from "react";
import { api, type Diagnosis, type Overlay, type Settings, type Status, type Tray } from "./ipc";

const EMPTY_STATUS: Status = { running: false, paused: false, failure: null };

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
        <p className="muted">{problem ?? "Loading settings."}</p>
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

  return (
    <main>
      <header>
        <h1>Clack</h1>
        <p className="muted">{diagnosis?.summary ?? "Checking what is available."}</p>
      </header>

      {problem !== null && <p className="problem">{problem}</p>}
      {status.failure !== null && <p className="problem">{status.failure}</p>}

      {diagnosis?.canGrantAccess === true && (
        <section>
          <h2>Permission</h2>
          <p className="muted">
            Clack reads keyboards from <code>/dev/input</code>, which needs permission. This
            installs <code>{diagnosis.rulePath}</code>, granting read access to whoever is logged in
            on the active seat, revoked at logout. You will be asked for your password once.
          </p>
          <pre>{diagnosis.ruleText.trim()}</pre>
          <button
            type="button"
            onClick={() =>
              void attempt(async () => {
                setDiagnosis(await api.grantAccess());
              })
            }
          >
            Grant access
          </button>
        </section>
      )}

      <section>
        <div className="row">
          <button
            type="button"
            onClick={() =>
              void attempt(async () => {
                setStatus(status.running ? await api.stop() : await api.start(settings));
              })
            }
          >
            {status.running ? "Stop showing keys" : "Start showing keys"}
          </button>
          <button
            type="button"
            disabled={!status.running}
            onClick={() =>
              void attempt(async () => {
                setStatus(await api.setPaused(!status.paused));
              })
            }
          >
            {status.paused ? "Resume" : "Pause"}
          </button>
        </div>
        <p className="muted">
          {status.running
            ? status.paused
              ? "Paused. Keys are read and discarded, nothing is shown or kept."
              : "Showing every key you press, including keys typed into other windows."
            : "Not running."}
        </p>
      </section>

      <section>
        <h2>Appearance</h2>

        <label>
          Font
          <input
            value={settings.overlay.font}
            onChange={(event) => editOverlay({ font: event.target.value })}
            placeholder="Sans Bold 24"
          />
          <span className="muted">Pango notation: family, style, size.</span>
        </label>

        <label>
          Edge
          <select
            value={settings.overlay.anchor}
            onChange={(event) =>
              editOverlay({ anchor: event.target.value === "top" ? "top" : "bottom" })
            }
          >
            <option value="bottom">Bottom</option>
            <option value="top">Top</option>
          </select>
        </label>

        <label>
          Distance from that edge
          <input
            type="number"
            min={0}
            max={4000}
            value={settings.overlay.margin}
            onChange={(event) => editOverlay({ margin: Number(event.target.value) })}
          />
          <span className="muted">Pixels.</span>
        </label>

        <label>
          Time on screen
          <input
            type="number"
            min={0.2}
            max={30}
            step={0.1}
            value={settings.overlay.timeout_seconds}
            onChange={(event) => editOverlay({ timeout_seconds: Number(event.target.value) })}
          />
          <span className="muted">Seconds after the last key.</span>
        </label>
      </section>

      <section>
        <h2>On launch</h2>

        <label className="inline">
          <input
            type="checkbox"
            checked={settings.tray.start_hidden}
            onChange={(event) => editTray({ start_hidden: event.target.checked })}
          />
          Start in the tray, without this window
        </label>

        <label className="inline">
          <input
            type="checkbox"
            checked={settings.tray.start_overlay}
            onChange={(event) => editTray({ start_overlay: event.target.checked })}
          />
          Start showing keys straight away
        </label>
      </section>

      <footer className="muted">
        Changes to appearance apply the next time you start showing keys. Closing this window leaves
        Clack in the tray.
      </footer>
    </main>
  );
}
