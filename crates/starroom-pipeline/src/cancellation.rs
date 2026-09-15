//! Request-scoped cooperative cancellation for synchronous Native graph execution.
//! Export has no token unless its caller explicitly supplies one. Nested scopes restore their
//! caller's token, and a cancelled worker cannot contaminate a later request on the same thread.
use crate::PipelineError;
use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

thread_local! {
    static TOKEN: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

pub fn with_cancellation<T>(token: Arc<AtomicBool>, render: impl FnOnce() -> T) -> T {
    struct Restore(Option<Arc<AtomicBool>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            TOKEN.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(TOKEN.with(|slot| slot.replace(Some(token))));
    render()
}

pub fn checkpoint() -> Result<(), PipelineError> {
    if TOKEN.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|token| token.load(Ordering::Acquire))
    }) {
        Err(PipelineError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_is_explicit_scoped_and_does_not_leak_into_export() {
        let token = Arc::new(AtomicBool::new(false));
        with_cancellation(token.clone(), || {
            assert!(checkpoint().is_ok());
            token.store(true, Ordering::Release);
            assert!(matches!(checkpoint(), Err(PipelineError::Cancelled)));
            with_cancellation(Arc::new(AtomicBool::new(false)), || {
                assert!(checkpoint().is_ok())
            });
            assert!(matches!(checkpoint(), Err(PipelineError::Cancelled)));
        });
        assert!(checkpoint().is_ok());
    }

    #[test]
    fn cancelled_production_graph_does_not_change_next_preview_export_parity() {
        use crate::{
            RenderSettings, render_source_export_to_srgb8, render_source_preview_to_srgb8,
        };
        use starroom_imageio::{DecodedRenderedImage, DecodedSourceImage, RenderedFormat};
        let source = DecodedSourceImage::Rendered(DecodedRenderedImage {
            width: 2,
            height: 2,
            format: RenderedFormat::Png,
            rgba: [0.2, 0.3, 0.4, 1.0].repeat(4),
            embedded_icc: None,
            exif: None,
        });
        let settings = RenderSettings::default();
        let cancelled = with_cancellation(Arc::new(AtomicBool::new(true)), || {
            render_source_preview_to_srgb8(&source, &settings)
        });
        assert!(matches!(cancelled, Err(PipelineError::Cancelled)));
        let preview = with_cancellation(Arc::new(AtomicBool::new(false)), || {
            render_source_preview_to_srgb8(&source, &settings)
        })
        .unwrap();
        let export = render_source_export_to_srgb8(&source, &settings).unwrap();
        assert_eq!(preview.data, export.data);
    }
}
