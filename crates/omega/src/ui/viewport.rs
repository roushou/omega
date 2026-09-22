//! A clipped, zoomable, pannable view of its content.

use omega_proto::omega::Value;
use omega_proto::{IntoValue, Values};

use crate::ui::Bind;
use crate::ui::Fit;
use crate::ui::node::Node;
use crate::ui::style::styled;
use crate::{Args, Error, Input};

/// One viewport gesture, with the transform it produced. A wheel or pinch
/// changes [`zoom`](Self::zoom) about a pointer anchor; a drag changes the
/// offset by [`dx`](Self::dx) and [`dy`](Self::dy).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportGesture {
    /// Magnification over the fitted size after the gesture.
    pub zoom: f64,
    /// Horizontal pan from the centred position, in viewport pixels.
    pub offset_x: f64,
    /// Vertical pan from the centred position, in viewport pixels.
    pub offset_y: f64,
    /// Gesture origin x in viewport pixels.
    pub x: f64,
    /// Gesture origin y in viewport pixels.
    pub y: f64,
    /// Horizontal pan applied by this drag event.
    pub dx: f64,
    /// Vertical pan applied by this drag event.
    pub dy: f64,
}

impl Default for ViewportGesture {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
            x: 0.0,
            y: 0.0,
            dx: 0.0,
            dy: 0.0,
        }
    }
}

impl ViewportGesture {
    fn number(values: &Values, name: &str) -> Option<f64> {
        values
            .get::<f64>(name)
            .or_else(|| values.get::<i64>(name).map(|held| held as f64))
    }
}

impl Input for ViewportGesture {
    fn decode(args: Args) -> Result<Self, Error> {
        if args.len() != 1 {
            return Err(Error::invalid("expected one viewport gesture"));
        }
        let values: Values = args
            .get(0)
            .ok_or_else(|| Error::invalid("expected a viewport gesture map"))?;
        Ok(Self {
            zoom: Self::number(&values, "zoom").unwrap_or(1.0),
            offset_x: Self::number(&values, "offset_x").unwrap_or(0.0),
            offset_y: Self::number(&values, "offset_y").unwrap_or(0.0),
            x: Self::number(&values, "x").unwrap_or(0.0),
            y: Self::number(&values, "y").unwrap_or(0.0),
            dx: Self::number(&values, "dx").unwrap_or(0.0),
            dy: Self::number(&values, "dy").unwrap_or(0.0),
        })
    }

    fn encode(self) -> Vec<Value> {
        vec![
            Values::new()
                .with("zoom", self.zoom)
                .with("offset_x", self.offset_x)
                .with("offset_y", self.offset_y)
                .with("x", self.x)
                .with("y", self.y)
                .with("dx", self.dx)
                .with("dy", self.dy)
                .into_value(),
        ]
    }
}

/// A clipped, zoomable, pannable view of its content.
///
/// ```no_run
/// use omega::ui::{Fit, Image, Viewport};
/// # fn view(events: &omega::surface::Events<()>) -> omega::View {
/// Viewport::new()
///     .fit(Fit::Contain)
///     .on_wheel(events.on(|_: omega::ui::ViewportGesture| ()))
///     .child(Image::new("/home/me/photo.png").fit(Fit::None))
///     .into()
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Viewport {
    node: Node,
}

impl Viewport {
    pub fn new() -> Self {
        Self {
            node: Node::new("viewport"),
        }
    }

    /// Append content. The viewport measures its natural size to fit it.
    pub fn child(mut self, child: impl Into<crate::View>) -> Self {
        self.node = self.node.child(child);
        self
    }

    /// How the content fits the viewport before [`zoom`](Self::zoom).
    pub fn fit(mut self, fit: Fit) -> Self {
        self.node = self.node.text_prop("fit", fit.as_str());
        self
    }

    /// Magnification over the fitted size. `1.0` fits.
    pub fn zoom(mut self, zoom: f64) -> Self {
        self.node = self.node.fraction("zoom", zoom);
        self
    }

    /// Pan in viewport pixels from the centred position.
    pub fn offset(mut self, x: f64, y: f64) -> Self {
        self.node = self.node.fraction("offset_x", x).fraction("offset_y", y);
        self
    }

    /// The command revision. The renderer adopts [`zoom`](Self::zoom) and
    /// [`offset`](Self::offset) only when this value changes, so a live gesture
    /// is never overwritten by a render.
    pub fn revision(mut self, revision: u32) -> Self {
        self.node = self.node.number("revision", revision);
        self
    }

    /// Receive pointer-anchored wheel zoom.
    pub fn on_wheel(mut self, wheel: impl Into<Bind<ViewportGesture>>) -> Self {
        self.node = self.node.on("wheel", wheel);
        self
    }

    /// Receive pan deltas.
    pub fn on_drag(mut self, drag: impl Into<Bind<ViewportGesture>>) -> Self {
        self.node = self.node.on("drag", drag);
        self
    }

    /// Receive trackpad pinch zoom.
    pub fn on_pinch(mut self, pinch: impl Into<Bind<ViewportGesture>>) -> Self {
        self.node = self.node.on("pinch", pinch);
        self
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self::new()
    }
}

styled!(Viewport);
