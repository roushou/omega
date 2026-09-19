//! Plugin session ownership.
pub(crate) use crate::process::session::SessionLink;
pub use crate::process::session::{REQUEST_TIMEOUT, Request, RequestError};
use omega_proto::PluginName;

/// Deregisters a plugin when its session ends, however it ends.
#[derive(Debug)]
pub struct SessionGuard {
    plugins: super::PluginRegistry,
    plugin: PluginName,
    link: SessionLink,
}

impl SessionGuard {
    pub(super) fn new(
        plugins: super::PluginRegistry,
        plugin: PluginName,
        link: SessionLink,
    ) -> Self {
        Self {
            plugins,
            plugin,
            link,
        }
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        self.plugins.disconnected(&self.plugin, &self.link);
    }
}

impl SessionGuard {
    pub async fn cancelled(&self) {
        self.link.stop.wait().await;
    }
    pub fn is_current(&self) -> bool {
        !self.link.stop.is_triggered()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::PluginRegistry;
    use omega_proto::omega::invoke;

    #[tokio::test]
    async fn outbound_requests_share_a_byte_budget_across_plugins() {
        let plugins = PluginRegistry::detached(crate::hub::Hub::new());
        let first = "first".parse::<PluginName>().unwrap();
        let second = "second".parse::<PluginName>().unwrap();
        let (a, mut ar) = tokio::sync::mpsc::channel(16);
        let (b, mut br) = tokio::sync::mpsc::channel(16);
        let _ag = plugins.connected(&first, a);
        let _bg = plugins.connected(&second, b);
        let op = invoke::Op::SetState(omega_proto::omega::SetState {
            topic: "x".repeat(3 * 1024 * 1024),
            ..Default::default()
        });
        let one = plugins.request(&first, op.clone());
        let two = plugins.request(&second, op.clone());
        tokio::pin!(one, two);
        let (a, b) = tokio::select! {
            biased;
            _ = &mut one => panic!("first was not queued"),
            _ = &mut two => panic!("second was not queued"),
            queued = async { (ar.recv().await.unwrap(), br.recv().await.unwrap()) } => queued,
        };
        assert!(matches!(
            plugins.request(&first, op).await,
            Err(RequestError::Full(_))
        ));
        drop(a);
        drop(b);
        assert_eq!(
            plugins.inner.request_bytes.available_permits(),
            PluginRegistry::REQUEST_BYTES
        );
        let oversized = invoke::Op::SetState(omega_proto::omega::SetState {
            topic: "x".repeat(omega_proto::MAX_FRAME_LEN),
            ..Default::default()
        });
        assert!(matches!(
            plugins.request(&first, oversized).await,
            Err(RequestError::TooLarge(_))
        ));
    }
}
