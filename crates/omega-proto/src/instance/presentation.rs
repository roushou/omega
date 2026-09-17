use super::{InstanceKey, PlacementId};
use crate::omega::{KeyboardPolicy, Presentation, RendererFeature, presentation};

/// Validated host requirements, with no permissive fallback for unknown kinds.
#[derive(Debug, Clone, PartialEq)]
pub struct PresentationSpec(Presentation);

impl TryFrom<Presentation> for PresentationSpec {
    type Error = PresentationError;

    fn try_from(value: Presentation) -> Result<Self, Self::Error> {
        match value.kind.as_ref().ok_or(PresentationError::Missing)? {
            presentation::Kind::Embedded(embedded) => {
                embedded.placement.parse::<PlacementId>()?;
            }
            presentation::Kind::Popup(popup) => {
                InstanceKey::try_from(
                    popup
                        .anchor
                        .as_ref()
                        .ok_or(PresentationError::MissingAnchor)?,
                )?;
            }
            presentation::Kind::Window(window) => {
                Self::size(window.width, window.height)?;
                if window.min_width == 0
                    || window.min_height == 0
                    || window.min_width > window.width
                    || window.min_height > window.height
                {
                    return Err(PresentationError::Size);
                }
                if window.title.len() > 512
                    || window.title.contains('\0')
                    || window.app_id.is_empty()
                    || window.app_id.len() > 128
                    || !window
                        .app_id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
                {
                    return Err(PresentationError::WindowIdentity);
                }
            }
            presentation::Kind::Overlay(overlay) => {
                Self::size(overlay.width, overlay.height)?;
                if overlay.output.len() > 128 || overlay.output.contains('\0') {
                    return Err(PresentationError::Output);
                }
                match KeyboardPolicy::try_from(overlay.keyboard) {
                    Ok(
                        KeyboardPolicy::None | KeyboardPolicy::OnDemand | KeyboardPolicy::Exclusive,
                    ) => {}
                    Ok(KeyboardPolicy::Unspecified) | Err(_) => {
                        return Err(PresentationError::Keyboard);
                    }
                }
            }
        }
        Ok(Self(value))
    }
}

impl PresentationSpec {
    fn size(width: u32, height: u32) -> Result<(), PresentationError> {
        if !(1..=16384).contains(&width) || !(1..=16384).contains(&height) {
            return Err(PresentationError::Size);
        }
        Ok(())
    }
    pub fn wire(&self) -> &Presentation {
        &self.0
    }
    pub fn feature(&self) -> RendererFeature {
        match self.0.kind.as_ref().expect("validated presentation") {
            presentation::Kind::Embedded(_) => RendererFeature::Embedded,
            presentation::Kind::Popup(_) => RendererFeature::Popups,
            presentation::Kind::Window(_) => RendererFeature::Windows,
            presentation::Kind::Overlay(_) => RendererFeature::Overlays,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PresentationError {
    #[error("presentation kind is required")]
    Missing,
    #[error("popup anchor is required")]
    MissingAnchor,
    #[error(
        "presentation dimensions must be 1..=16384 and minimum dimensions cannot exceed the initial size"
    )]
    Size,
    #[error("window requires a valid application ID and bounded title")]
    WindowIdentity,
    #[error("invalid output name")]
    Output,
    #[error("overlay requires an explicit supported keyboard policy")]
    Keyboard,
    #[error(transparent)]
    Identifier(#[from] crate::IdentError),
}
