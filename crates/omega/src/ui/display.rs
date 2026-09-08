//! Things that show a reading without taking one.

use crate::ui::node::Node;
use crate::ui::style::styled;
use crate::units::Percent;

/// A filled bar.
///
/// Shows a proportion; it does not take one. A bar the user can drag is a
/// [`Slider`], and the difference is whether the unit hears about it.
///
/// [`Slider`]: crate::ui::Slider
#[derive(Debug, Clone)]
pub struct Progress {
    node: Node,
}

impl Progress {
    pub fn new(filled: Percent) -> Self {
        Self {
            node: Node::new("progress").fraction("value", filled.fraction()),
        }
    }
}

styled!(Progress);

/// A series, drawn small.
///
/// What a reading has been doing, rather than what it is: the ping history
/// beside a latency figure, the throughput behind a transfer rate. A
/// [`Progress`] answers "how much"; this answers "and before that".
///
/// ```
/// # use omega::Graph;
/// # let latencies = vec![14.0, 19.0, 12.0, 44.0, 15.0];
/// Graph::new(latencies);
/// ```
///
/// **Give it a range when the scale is fixed.** Left to itself it scales to
/// the highest and lowest points it was given, which is right for latency and
/// wrong for a percentage — an idle machine's three-percent noise would fill
/// the frame and look like a machine on fire:
///
/// ```
/// # use omega::Graph;
/// # let cpu = vec![3.0, 4.0, 2.0];
/// Graph::new(cpu).range(0.0, 100.0);
/// ```
///
/// The daemon publishes no history of its own: a unit keeps the points it
/// wants in its own keyspace and hands them here.
#[derive(Debug, Clone)]
pub struct Graph {
    node: Node,
}

impl Graph {
    /// Oldest first, so the newest point is the right-hand edge — which is
    /// where an eye goes for "now".
    pub fn new(points: impl IntoIterator<Item = f64>) -> Self {
        Self {
            node: Node::new("graph").fractions("points", points.into_iter().collect()),
        }
    }

    /// Pin the scale rather than let the data set it.
    ///
    /// A range that is not a range — high at or below low — is ignored and
    /// the graph scales itself, because a frame of zero height is not a
    /// drawing anybody wanted.
    pub fn range(mut self, low: f64, high: f64) -> Self {
        self.node = self.node.fraction("low", low).fraction("high", high);
        self
    }
}

styled!(Graph);

/// A picture.
///
/// Album art, a QR code, an avatar. Sized by the [`width`] and [`height`]
/// every node has; given neither, it is drawn at its own size.
///
/// **A local file or a `data:` URI, and nothing else.** A shell that fetched
/// whatever a unit named would be making network requests on that unit's
/// behalf, which no capability granted and which would leak that the desktop
/// is displaying something. A source that is neither is dropped, and the
/// node draws nothing rather than reaching out.
///
/// ```
/// # use omega::Image;
/// Image::new("/home/me/.cache/art.png");
/// ```
///
/// [`width`]: Image::width
/// [`height`]: Image::height
#[derive(Debug, Clone)]
pub struct Image {
    node: Node,
}

impl Image {
    pub fn new(source: impl Into<String>) -> Self {
        let source = source.into();
        let mut node = Node::new("image");
        if Self::is_local(&source) {
            node = node.text_prop("source", source);
        }
        Self { node }
    }

    /// Whether a source is one the shell may read without reaching out.
    ///
    /// `file:` and an absolute path are the same thing said two ways; `data:`
    /// carries the bytes itself. Everything else — `http`, `https`, and any
    /// scheme invented later — is a request, and a unit that wanted to make
    /// one has `CAPABILITY_NETWORK` and its own process to make it in.
    fn is_local(source: &str) -> bool {
        source.starts_with('/') || source.starts_with("file:/") || source.starts_with("data:")
    }
}

styled!(Image);
