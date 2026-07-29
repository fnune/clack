use cairo::{Context, Format, ImageSurface};
use clack_core::{Frame, RunKind, Surface};

use crate::config::{Anchor, Style};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor as LayerAnchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler,
    LayerSurface, LayerSurfaceConfigure,
};
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
    registry_handlers,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_region, wl_shm, wl_surface};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, delegate_noop};
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::{
    Event as FractionalScaleEvent, WpFractionalScaleV1,
};
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;
use wayland_protocols::wp::viewporter::client::wp_viewporter::WpViewporter;

const PADDING_X: i32 = 24;
const PADDING_Y: i32 = 12;
const CORNER_RADIUS: f64 = 12.0;
const BACKGROUND: Rgba = Rgba(0.06, 0.06, 0.08, 0.82);
const TEXT: &str = "#f5f5f5";
const CHORD: &str = "#7dd3fc";
const SPECIAL: &str = "#a1a1aa";
const CONFIGURE_ROUNDTRIPS: usize = 10;

/// `wp_fractional_scale_v1` reports scale in 120ths, so 1.75 arrives as 210.
const SCALE_DENOMINATOR: u32 = 120;

#[derive(Clone, Copy, Debug)]
struct Rgba(f64, f64, f64, f64);

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("connecting to the Wayland compositor failed: {0}")]
    Connect(#[from] wayland_client::ConnectError),
    #[error("reading the compositor's globals failed: {0}")]
    Globals(#[from] wayland_client::globals::GlobalError),
    #[error("the compositor does not support {0}, so the overlay cannot be drawn")]
    Unsupported(&'static str),
    #[error("talking to the compositor failed: {0}")]
    Dispatch(#[from] wayland_client::DispatchError),
    #[error("the compositor never configured the overlay surface")]
    NeverConfigured,
    #[error("drawing failed: {0}")]
    Draw(#[from] cairo::Error),
    #[error("reading the drawn pixels failed: {0}")]
    Pixels(#[from] cairo::BorrowError),
    #[error("allocating a shared memory buffer failed: {0}")]
    Buffer(String),
    #[error("the overlay does not fit on screen")]
    TooLarge,
}

pub struct LayerOverlay {
    queue: EventQueue<State>,
    state: State,
    style: Style,
    size: Option<(u32, u32)>,
    scale: u32,
    mapped: bool,
    shown: Frame,
}

impl LayerOverlay {
    pub fn open(style: Style) -> Result<Self, RenderError> {
        let connection = Connection::connect_to_env()?;
        let (globals, queue) = registry_queue_init(&connection)?;
        let handle = queue.handle();

        let compositor = CompositorState::bind(&globals, &handle)
            .map_err(|_| RenderError::Unsupported("wl_compositor"))?;
        let shell = LayerShell::bind(&globals, &handle)
            .map_err(|_| RenderError::Unsupported("wlr-layer-shell"))?;
        let shm = Shm::bind(&globals, &handle).map_err(|_| RenderError::Unsupported("wl_shm"))?;

        let surface = compositor.create_surface(&handle);
        surface.set_input_region(Some(&compositor.wl_compositor().create_region(&handle, ())));

        let layer =
            shell.create_layer_surface(&handle, surface, Layer::Overlay, Some("clack"), None);

        // Without these the compositor resamples a buffer drawn for the nearest
        // whole scale, which is visible as blurred text on a 1.5 or 1.75 output.
        let viewport = globals
            .bind::<WpViewporter, _, _>(&handle, 1..=1, ())
            .ok()
            .map(|viewporter| viewporter.get_viewport(layer.wl_surface(), &handle, ()));
        let fractional_scale = globals
            .bind::<WpFractionalScaleManagerV1, _, _>(&handle, 1..=1, ())
            .ok()
            .map(|manager| manager.get_fractional_scale(layer.wl_surface(), &handle, ()));

        let pool =
            SlotPool::new(1, &shm).map_err(|error| RenderError::Buffer(error.to_string()))?;

        Ok(Self {
            queue,
            state: State {
                registry: RegistryState::new(&globals),
                output: OutputState::new(&globals, &handle),
                shm,
                pool,
                layer,
                configured: false,
                closed: false,
                scale: SCALE_DENOMINATOR,
                viewport,
                fractional_scale,
            },
            style,
            size: None,
            scale: SCALE_DENOMINATOR,
            mapped: false,
            shown: Frame::default(),
        })
    }

    /// A layer surface only becomes mappable after an empty commit answered by
    /// a configure. Re-sizing a mapped surface needs no such handshake, and
    /// waiting for one hangs: the compositor has nothing new to say.
    ///
    /// Every mapping re-states the anchor and margins. KWin drops them when a
    /// surface unmaps, so a remap without them lands centred on screen.
    fn map(&mut self, size: (u32, u32)) -> Result<(), RenderError> {
        self.state.layer.set_anchor(match self.style.anchor {
            Anchor::Top => LayerAnchor::TOP,
            Anchor::Bottom => LayerAnchor::BOTTOM,
        });
        self.state
            .layer
            .set_margin(self.style.margin, 0, self.style.margin, 0);
        self.state
            .layer
            .set_keyboard_interactivity(KeyboardInteractivity::None);
        self.state.layer.set_exclusive_zone(-1);
        self.apply_size(size);
        self.state.layer.commit();
        self.state.configured = false;

        for _ in 0..CONFIGURE_ROUNDTRIPS {
            self.queue.roundtrip(&mut self.state)?;
            if self.state.configured {
                self.size = Some(size);
                self.mapped = true;
                return Ok(());
            }
        }

        Err(RenderError::NeverConfigured)
    }

    /// With a viewport the buffer is drawn at whatever the output asked for and
    /// mapped down to its logical size. Without one, only whole scales are
    /// available, so the buffer scale is rounded down.
    fn apply_size(&mut self, size: (u32, u32)) {
        self.state.layer.set_size(size.0, size.1);

        match &self.state.viewport {
            Some(viewport) => {
                self.state.layer.wl_surface().set_buffer_scale(1);
                viewport.set_destination(
                    dimension(size.0).unwrap_or(1),
                    dimension(size.1).unwrap_or(1),
                );
            }
            None => {
                let whole = i32::try_from(self.scale / SCALE_DENOMINATOR).unwrap_or(1);
                self.state.layer.wl_surface().set_buffer_scale(whole.max(1));
            }
        }

        self.size = Some(size);
    }

    fn paint(&mut self, frame: &Frame, size: (u32, u32)) -> Result<(), RenderError> {
        let (width, height) = self.in_device_pixels(size)?;
        let mut drawn = draw(
            frame,
            &self.style.font,
            (width, height),
            self.scale_factor(),
        )?;
        let stride = drawn.stride();
        let pixels = drawn.data()?;

        let (buffer, canvas) = self
            .state
            .pool
            .create_buffer(
                dimension(width)?,
                dimension(height)?,
                stride,
                wl_shm::Format::Argb8888,
            )
            .map_err(|error| RenderError::Buffer(error.to_string()))?;

        canvas
            .get_mut(..pixels.len())
            .ok_or(RenderError::TooLarge)?
            .copy_from_slice(&pixels);

        let surface = self.state.layer.wl_surface();
        surface.damage_buffer(0, 0, dimension(width)?, dimension(height)?);
        buffer
            .attach_to(surface)
            .map_err(|error| RenderError::Buffer(error.to_string()))?;
        self.state.layer.commit();

        Ok(())
    }

    /// Clears the overlay by painting nothing, rather than by unmapping. An
    /// unmapped layer surface comes back on the current workspace only, and
    /// without its anchor, so it stops following the user between workspaces.
    fn blank(&mut self) -> Result<(), RenderError> {
        let size = self.size.unwrap_or((1, 1));
        if self.size != Some(size) {
            self.apply_size(size);
        }

        let (width, height) = self.in_device_pixels(size)?;
        let (buffer, canvas) = self
            .state
            .pool
            .create_buffer(
                dimension(width)?,
                dimension(height)?,
                dimension(width.saturating_mul(4))?,
                wl_shm::Format::Argb8888,
            )
            .map_err(|error| RenderError::Buffer(error.to_string()))?;
        canvas.fill(0);

        let surface = self.state.layer.wl_surface();
        surface.damage_buffer(0, 0, dimension(width)?, dimension(height)?);
        buffer
            .attach_to(surface)
            .map_err(|error| RenderError::Buffer(error.to_string()))?;
        self.state.layer.commit();

        Ok(())
    }

    /// The compositor scales a buffer up to the output's scale, so drawing at
    /// scale 1 on a HiDPI screen leaves the text blurred.
    fn in_device_pixels(&self, logical: (u32, u32)) -> Result<(u32, u32), RenderError> {
        let scaled = |side: u32| {
            side.checked_mul(self.scale)
                .map(|product| product.div_ceil(SCALE_DENOMINATOR))
                .ok_or(RenderError::TooLarge)
        };

        Ok((scaled(logical.0)?, scaled(logical.1)?))
    }

    fn scale_factor(&self) -> f64 {
        f64::from(self.scale) / f64::from(SCALE_DENOMINATOR)
    }
}

impl Surface for LayerOverlay {
    type Error = RenderError;

    fn is_closed(&self) -> bool {
        self.state.closed
    }

    fn present(&mut self, frame: &Frame) -> Result<(), Self::Error> {
        if self.scale != self.state.scale {
            self.scale = self.state.scale;
            self.size = None;
            self.shown = Frame::default();
        }

        if *frame == self.shown {
            self.queue.roundtrip(&mut self.state)?;
            return Ok(());
        }

        if frame.is_empty() {
            if self.mapped {
                self.blank()?;
            }
        } else {
            let size = measure(frame, &self.style.font)?;
            if !self.mapped {
                self.map(size)?;
            } else if self.size != Some(size) {
                self.apply_size(size);
            }
            self.paint(frame, size)?;
        }

        self.queue.roundtrip(&mut self.state)?;
        self.shown = frame.clone();
        Ok(())
    }
}

impl std::fmt::Debug for LayerOverlay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LayerOverlay")
            .field("size", &self.size)
            .field("style", &self.style)
            .finish_non_exhaustive()
    }
}

fn measure(frame: &Frame, font: &str) -> Result<(u32, u32), RenderError> {
    let probe = ImageSurface::create(Format::ARgb32, 1, 1)?;
    let context = Context::new(&probe)?;
    let (text_width, text_height) = layout(&context, frame, font).pixel_size();

    let width = text_width.saturating_add(2 * PADDING_X);
    let height = text_height.saturating_add(2 * PADDING_Y);

    Ok((extent(width)?, extent(height)?))
}

fn draw(
    frame: &Frame,
    font: &str,
    device: (u32, u32),
    scale: f64,
) -> Result<ImageSurface, RenderError> {
    let surface = ImageSurface::create(Format::ARgb32, dimension(device.0)?, dimension(device.1)?)?;
    let context = Context::new(&surface)?;

    context.set_operator(cairo::Operator::Source);
    context.set_source_rgba(0.0, 0.0, 0.0, 0.0);
    context.paint()?;
    context.set_operator(cairo::Operator::Over);
    context.scale(scale, scale);

    let logical = |side: u32| f64::from(side) / scale;
    rounded_rectangle(&context, logical(device.0), logical(device.1));
    let Rgba(red, green, blue, alpha) = BACKGROUND;
    context.set_source_rgba(red, green, blue, alpha);
    context.fill()?;

    context.move_to(f64::from(PADDING_X), f64::from(PADDING_Y));
    pangocairo::functions::show_layout(&context, &layout(&context, frame, font));

    drop(context);
    surface.flush();
    Ok(surface)
}

fn layout(context: &Context, frame: &Frame, font: &str) -> pango::Layout {
    let layout = pangocairo::functions::create_layout(context);
    layout.set_font_description(Some(&pango::FontDescription::from_string(font)));
    layout.set_markup(&markup(frame));
    layout
}

fn markup(frame: &Frame) -> String {
    let mut out = String::new();
    let mut previous: Option<RunKind> = None;

    for run in &frame.runs {
        let gap = match previous {
            None => false,
            Some(RunKind::Text) => run.kind != RunKind::Text,
            Some(_) => true,
        };
        if gap {
            out.push(' ');
        }

        let colour = match run.kind {
            RunKind::Text => TEXT,
            RunKind::Chord => CHORD,
            RunKind::Special => SPECIAL,
        };
        let escaped = pango::glib::markup_escape_text(&run.text);
        out.push_str(&format!("<span foreground=\"{colour}\">{escaped}</span>"));
        previous = Some(run.kind);
    }

    out
}

fn rounded_rectangle(context: &Context, width: f64, height: f64) {
    let radius = CORNER_RADIUS.min(width / 2.0).min(height / 2.0);
    let right_angle = std::f64::consts::FRAC_PI_2;

    context.new_sub_path();
    context.arc(width - radius, radius, radius, -right_angle, 0.0);
    context.arc(width - radius, height - radius, radius, 0.0, right_angle);
    context.arc(
        radius,
        height - radius,
        radius,
        right_angle,
        2.0 * right_angle,
    );
    context.arc(radius, radius, radius, 2.0 * right_angle, 3.0 * right_angle);
    context.close_path();
}

fn extent(value: i32) -> Result<u32, RenderError> {
    u32::try_from(value)
        .ok()
        .filter(|pixels| (1..=u32::from(u16::MAX)).contains(pixels))
        .ok_or(RenderError::TooLarge)
}

fn dimension(value: u32) -> Result<i32, RenderError> {
    i32::try_from(value).map_err(|_| RenderError::TooLarge)
}

struct State {
    registry: RegistryState,
    output: OutputState,
    shm: Shm,
    pool: SlotPool,
    layer: LayerSurface,
    configured: bool,
    closed: bool,
    /// In 120ths, whatever the source.
    scale: u32,
    viewport: Option<WpViewport>,
    fractional_scale: Option<WpFractionalScaleV1>,
}

impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.closed = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        _: LayerSurfaceConfigure,
        _: u32,
    ) {
        self.configured = true;
    }
}

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        // Only used when the compositor has no fractional scale protocol, which
        // is why it is stored in the same 120ths as the fractional one.
        if self.fractional_scale.is_none() {
            self.scale = u32::try_from(new_factor.max(1)).unwrap_or(1) * SCALE_DENOMINATOR;
        }
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    registry_handlers![OutputState];
}

delegate_compositor!(State);
delegate_output!(State);
delegate_shm!(State);
delegate_layer!(State);
delegate_registry!(State);
delegate_noop!(State: ignore wl_region::WlRegion);
delegate_noop!(State: ignore WpViewporter);
delegate_noop!(State: ignore WpViewport);
delegate_noop!(State: ignore WpFractionalScaleManagerV1);

impl Dispatch<WpFractionalScaleV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &WpFractionalScaleV1,
        event: FractionalScaleEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let FractionalScaleEvent::PreferredScale { scale } = event {
            state.scale = scale.max(1);
        }
    }
}
