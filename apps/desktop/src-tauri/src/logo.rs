//! Clack's mark, drawn rather than shipped as pixels. The tray wants it in the
//! panel's own colour, which is not known until the desktop is asked, and in three
//! states. Cairo is already linked for the overlay, so the geometry lives here
//! once and the files the tray needs are drawn from it at startup.

use std::f64::consts::{FRAC_PI_2, PI};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use cairo::{Context, Format, ImageSurface, LineCap, LineJoin};

/// Three keys of rising height standing on a shared floor, the newest on the
/// right: left edge and height of each, on a 24-unit grid. Width 4.8, pitch 7.4,
/// floor at 20.2. Strokes are centred, so at this weight the keys read 6.4 wide
/// with a full unit of air between them and nothing overlaps.
const KEYS: [(f64, f64); 3] = [(2.2, 5.0), (9.6, 6.2), (17.0, 7.4)];
const GRID: f64 = 24.0;
const WIDTH: f64 = 4.8;
const RADIUS: f64 = 1.5;
const FLOOR: f64 = 20.2;

/// Lighter than the mark is drawn anywhere else, to land where the panel's own
/// icons sit: a panel picks the size it wants and scales whatever it is handed, so
/// a stroke set for a 24-unit grid arrives thicker than it was drawn.
const STROKE: f64 = 1.4;

/// How far the broken-off piece is knocked up and to the right, and how far it
/// tips over. The other half of the key does not move: the fault reads because
/// one piece left an edge the rest of the mark still holds.
const SHIFT: f64 = 0.55;
const TILT: f64 = 5.0 * PI / 180.0;

#[derive(Debug, thiserror::Error)]
pub enum LogoError {
    #[error("the mark could not be drawn: {0}")]
    Draw(#[from] cairo::Error),
    #[error("the mark could not be read back: {0}")]
    Read(#[from] cairo::BorrowError),
    #[error("the mark could not be written to {1}: {0}")]
    Write(png::EncodingError, String),
}

/// What Clack is doing, said in the only vocabulary a monochrome icon has. The
/// three keys never move: showing keys fills the newest one, and an error breaks
/// it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mood {
    Idle,
    Showing,
    Stuck,
}

/// Full strength, every key, every state. The trail's decay is carried by opacity
/// on a plate and in the window, but a panel paints its icons at one colour and
/// does not dim them: anything short of full here reads as a smudge beside the
/// bell next to it. What separates off from on is the newest key filling in.
const WEIGHT: f64 = 1.0;

/// A tray icon is one colour and the panel decides which. Whether the desktop
/// asked for a dark palette is the only signal there is, so it stands in for the
/// panel's own foreground.
#[derive(Clone, Copy, Debug)]
pub enum Ink {
    Light,
    Dark,
}

impl Ink {
    fn channels(self) -> [u8; 3] {
        match self {
            Self::Light => [0xe9, 0xea, 0xec],
            Self::Dark => [0x1a, 0x1c, 0x1e],
        }
    }
}

/// Straight, non-premultiplied RGBA, which is what a tray icon is handed as.
/// Only coverage varies across the mark, so it is drawn into an alpha-only
/// surface and the colour is added on the way out.
pub fn draw(mood: Mood, size: u32, ink: Ink) -> Result<Vec<u8>, LogoError> {
    let side = i32::try_from(size).unwrap_or(i32::MAX);
    let mut surface = ImageSurface::create(Format::A8, side, side)?;

    {
        let context = Context::new(&surface)?;
        let scale = f64::from(size) / GRID;
        context.scale(scale, scale);
        context.set_line_width(STROKE);
        context.set_line_cap(LineCap::Round);
        context.set_line_join(LineJoin::Round);

        context.set_source_rgba(0.0, 0.0, 0.0, WEIGHT);

        let last = KEYS.len().saturating_sub(1);
        for (index, (left, height)) in KEYS.into_iter().enumerate() {
            match mood {
                Mood::Stuck if index == last => snapped(&context, left, height)?,
                // The one key that carries the state: a panel is looked at, not
                // compared against a second copy of itself, so on and off have to
                // differ in something an eye lands on rather than in weight.
                Mood::Showing if index == last => {
                    whole(&context, left, height);
                    context.fill_preserve()?;
                    context.stroke()?;
                }
                _ => {
                    whole(&context, left, height);
                    context.stroke()?;
                }
            }
        }
    }

    surface.flush();
    let stride = usize::try_from(surface.stride()).unwrap_or(0);
    let width = usize::try_from(surface.width()).unwrap_or(0);
    let [red, green, blue] = ink.channels();
    let coverage = surface.data()?;

    Ok(coverage
        .chunks_exact(stride)
        .flat_map(|row| {
            row.iter()
                .take(width)
                .flat_map(|alpha| [red, green, blue, *alpha])
        })
        .collect())
}

/// Writes the same pixels to a file, as PNG.
///
/// The indicator hands the panel a *path* to its icon when it first appears and
/// never hands it another one, so that first path is the only thing the panel ever
/// reads. Whatever the mark becomes has to end up there.
pub fn write(path: &Path, pixels: &[u8], size: u32) -> Result<(), LogoError> {
    let name = || path.display().to_string();
    let file = File::create(path).map_err(|error| LogoError::Write(error.into(), name()))?;

    let mut encoder = png::Encoder::new(BufWriter::new(file), size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(pixels))
        .map_err(|error| LogoError::Write(error, name()))
}

fn whole(context: &Context, left: f64, height: f64) {
    let top = FLOOR - height;
    let right = left + WIDTH;

    context.new_sub_path();
    context.arc(right - RADIUS, top + RADIUS, RADIUS, -FRAC_PI_2, 0.0);
    context.arc(right - RADIUS, FLOOR - RADIUS, RADIUS, 0.0, FRAC_PI_2);
    context.arc(left + RADIUS, FLOOR - RADIUS, RADIUS, FRAC_PI_2, PI);
    context.arc(left + RADIUS, top + RADIUS, RADIUS, PI, PI + FRAC_PI_2);
    context.close_path();
}

/// The key snapped along a diagonal: the bottom-left bracket stays exactly where
/// the whole key's edges are, and only the top-right piece is displaced.
fn snapped(context: &Context, left: f64, height: f64) -> Result<(), cairo::Error> {
    let top = FLOOR - height;
    let right = left + WIDTH;
    let reach = (height - RADIUS * 2.0) / 2.0;

    context.move_to(right - RADIUS, FLOOR);
    context.line_to(left + RADIUS, FLOOR);
    context.arc(left + RADIUS, FLOOR - RADIUS, RADIUS, FRAC_PI_2, PI);
    context.line_to(left, FLOOR - RADIUS - reach);
    context.stroke()?;

    let pivot = (left + WIDTH * 0.65, top + height * 0.3);
    context.save()?;
    context.translate(SHIFT, -SHIFT);
    context.translate(pivot.0, pivot.1);
    context.rotate(TILT);
    context.translate(-pivot.0, -pivot.1);
    context.move_to(left + RADIUS, top);
    context.line_to(right - RADIUS, top);
    context.arc(right - RADIUS, top + RADIUS, RADIUS, -FRAC_PI_2, 0.0);
    context.line_to(right, top + RADIUS + reach);
    context.stroke()?;
    context.restore()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: u32 = 44;

    fn pixels(mood: Mood) -> Vec<u8> {
        draw(mood, SIZE, Ink::Light).unwrap()
    }

    fn coverage(mood: Mood) -> u64 {
        pixels(mood)
            .chunks_exact(4)
            .filter_map(|pixel| pixel.last().copied())
            .map(u64::from)
            .sum()
    }

    #[test]
    fn showing_keys_is_the_brightest_the_mark_gets() {
        assert!(coverage(Mood::Idle) < coverage(Mood::Showing));
        assert!(coverage(Mood::Stuck) < coverage(Mood::Showing));
    }

    #[test]
    fn a_broken_key_is_not_a_whole_one() {
        assert_ne!(pixels(Mood::Stuck), pixels(Mood::Idle));
    }

    /// The panel is handed one colour with the shape in the alpha channel. A
    /// stray colour would mean the surface was read as anything but coverage.
    #[test]
    fn the_mark_is_one_colour() {
        let ink = Ink::Dark.channels();
        let drawn = draw(Mood::Showing, SIZE, Ink::Dark).unwrap();

        assert_eq!(drawn.len(), usize::try_from(SIZE * SIZE * 4).unwrap());
        assert!(drawn.chunks_exact(4).all(|pixel| pixel.starts_with(&ink)));
    }
}
