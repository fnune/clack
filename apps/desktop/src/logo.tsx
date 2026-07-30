/// Clack's mark: three keys of rising height standing on a shared floor, the
/// newest on the right, the way the overlay stands on the bottom of the screen.
/// The same geometry the tray icon is drawn from in `logo.rs`, so the window and
/// the panel cannot end up showing different logos.
const KEYS = [
  { left: 2.2, height: 5 },
  { left: 9.6, height: 6.2 },
  { left: 17, height: 7.4 },
] as const;

const WIDTH = 4.8;
const RADIUS = 1.5;
const FLOOR = 20.2;

/// How far the broken-off piece is knocked up and to the right, and how far it
/// tips over. The rest of the key keeps the edges it had.
const SHIFT = 0.55;
const TILT = 5;

export type Mood = "idle" | "showing" | "stuck";

const WEIGHTS: Record<Mood, readonly [number, number, number]> = {
  idle: [0.3, 0.3, 0.3],
  showing: [0.55, 0.75, 1],
  stuck: [0.3, 0.3, 1],
};

/// The key snapped along a diagonal: the bottom-left bracket sits exactly where
/// the whole key's edges are, and only the top-right piece has moved.
function Snapped({ left, height, opacity }: { left: number; height: number; opacity: number }) {
  const top = FLOOR - height;
  const right = left + WIDTH;
  const reach = (height - RADIUS * 2) / 2;
  const pivot = `${left + WIDTH * 0.65} ${top + height * 0.3}`;

  return (
    <g opacity={opacity}>
      <path
        d={`M${right - RADIUS} ${FLOOR}h${-(WIDTH - RADIUS * 2)}a${RADIUS} ${RADIUS} 0 0 1 ${-RADIUS} ${-RADIUS}v${-reach}`}
      />
      <path
        transform={`translate(${SHIFT} ${-SHIFT}) rotate(${TILT} ${pivot})`}
        d={`M${left + RADIUS} ${top}h${WIDTH - RADIUS * 2}a${RADIUS} ${RADIUS} 0 0 1 ${RADIUS} ${RADIUS}v${reach}`}
      />
    </g>
  );
}

export function Logo({ mood }: { readonly mood: Mood }) {
  const weights = WEIGHTS[mood];

  return (
    <svg className="logo" viewBox="0 0 24 24" aria-hidden="true">
      {KEYS.map(({ left, height }, index) => {
        const opacity = weights[index] ?? 1;

        return mood === "stuck" && index === KEYS.length - 1 ? (
          <Snapped key={left} left={left} height={height} opacity={opacity} />
        ) : (
          <rect
            key={left}
            x={left}
            y={FLOOR - height}
            width={WIDTH}
            height={height}
            rx={RADIUS}
            opacity={opacity}
          />
        );
      })}
    </svg>
  );
}
