//! Payload tipado para arrastrar assets dentro del editor (E18).
//!
//! Se mantiene además el payload de texto antiguo para no romper los consumidores
//! que todavía lo usan, pero los destinos nuevos leen el tipo `AssetDrag`.

use egui::{Context, DragAndDrop};

use crate::assets::{ASSET_PAYLOAD_PREFIX, AssetKind};

/// Asset arrastrado desde el content browser, con tipo resuelto en el origen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AssetDrag {
    pub(crate) path: String,
    pub(crate) kind: AssetKind,
}

impl AssetDrag {
    pub(crate) fn new(path: impl Into<String>, kind: AssetKind) -> Self {
        Self {
            path: path.into(),
            kind,
        }
    }

    pub(crate) fn from_text(text: &str) -> Option<Self> {
        let path = legacy_path(text)?;
        let kind = AssetKind::from_path(path)?;
        Some(Self::new(path, kind))
    }
}

fn legacy_path(text: &str) -> Option<&str> {
    text.strip_prefix(ASSET_PAYLOAD_PREFIX)
        .filter(|path| !path.is_empty())
}

/// Publica el payload tipado. `egui` guarda un solo payload por frame, así que
/// no se publica también el texto heredado.
pub(crate) fn set_asset(ctx: &Context, drag: AssetDrag) {
    DragAndDrop::set_payload(ctx, drag);
}

/// ¿Hay un payload de asset disponible sin consumirlo?
#[must_use]
pub(crate) fn has_asset(ctx: &Context) -> bool {
    DragAndDrop::has_payload_of_type::<AssetDrag>(ctx)
        || DragAndDrop::payload::<String>(ctx)
            .is_some_and(|text| AssetDrag::from_text(text.as_str()).is_some())
}

/// Consume el payload tipado, o lo reconstruye desde el texto heredado.
#[must_use]
pub(crate) fn take_asset(ctx: &Context) -> Option<AssetDrag> {
    if DragAndDrop::has_payload_of_type::<AssetDrag>(ctx) {
        let payload = DragAndDrop::take_payload::<AssetDrag>(ctx)?;
        return Some(payload.as_ref().clone());
    }
    // `take_payload` consume incluso si el tipo no coincide: primero se mira.
    let text = DragAndDrop::payload::<String>(ctx)?;
    let drag = AssetDrag::from_text(text.as_str())?;
    DragAndDrop::clear_payload(ctx);
    Some(drag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_payload_survives_a_round_trip() {
        let ctx = Context::default();
        let drag = AssetDrag::new("sprites/hero.png", AssetKind::Image);
        set_asset(&ctx, drag.clone());
        assert!(has_asset(&ctx));
        assert_eq!(take_asset(&ctx), Some(drag));
        assert!(!has_asset(&ctx));
    }

    #[test]
    fn legacy_text_payload_is_accepted_and_bad_text_rejected() {
        let ctx = Context::default();
        DragAndDrop::set_payload(&ctx, "lunar-asset:audio/jump.wav".to_owned());
        assert_eq!(
            take_asset(&ctx),
            Some(AssetDrag::new("audio/jump.wav", AssetKind::Audio))
        );

        let ctx = Context::default();
        DragAndDrop::set_payload(&ctx, "otro:valor".to_owned());
        assert_eq!(take_asset(&ctx), None);
    }

    #[test]
    fn peeking_does_not_consume_the_payload() {
        let ctx = Context::default();
        DragAndDrop::set_payload(&ctx, AssetDrag::new("ship.glb", AssetKind::Mesh));
        assert!(has_asset(&ctx));
        assert!(has_asset(&ctx));
        assert_eq!(
            take_asset(&ctx).map(|drag| drag.path),
            Some("ship.glb".to_owned())
        );
    }
}
