import { useCallback, useEffect, useRef, useState } from "react";

export type NoteKind = "error" | "success";

export type Note = {
  id: number;
  message: string;
  kind: NoteKind;
  /// How long it has left, in milliseconds, or `null` to stay until dismissed.
  life: number | null;
};

const LIFE: Record<NoteKind, number | null> = {
  /// Something went wrong is worth reading twice, so it waits to be dismissed.
  error: null,
  success: 3000,
};

export function useNotifications() {
  const [notes, setNotes] = useState<Note[]>([]);
  const nextId = useRef(0);

  const dismiss = useCallback((id: number) => {
    setNotes((current) => current.filter((note) => note.id !== id));
  }, []);

  const notify = useCallback((message: string, kind: NoteKind) => {
    const id = nextId.current++;

    // The same failure twice in a row is one fact, not two.
    setNotes((current) => [
      ...current.filter((note) => note.message !== message),
      { id, message, kind, life: LIFE[kind] },
    ]);
  }, []);

  return { notes, notify, dismiss };
}

export function Notifications({
  notes,
  onDismiss,
}: {
  readonly notes: Note[];
  readonly onDismiss: (id: number) => void;
}) {
  if (notes.length === 0) return null;

  return (
    <div className="notes" aria-live="polite">
      {notes.map((note) => (
        <Notification key={note.id} note={note} onDismiss={onDismiss} />
      ))}
    </div>
  );
}

function Notification({
  note,
  onDismiss,
}: {
  readonly note: Note;
  readonly onDismiss: (id: number) => void;
}) {
  useEffect(() => {
    if (note.life === null) return;

    const timer = window.setTimeout(() => onDismiss(note.id), note.life);
    return () => {
      window.clearTimeout(timer);
    };
  }, [note.id, note.life, onDismiss]);

  return (
    <div className={`note ${note.kind}`} role={note.kind === "error" ? "alert" : "status"}>
      <span className="what">{note.message}</span>
      <button
        className="close"
        type="button"
        aria-label="Dismiss"
        onClick={() => onDismiss(note.id)}
      >
        ✕
      </button>
    </div>
  );
}
