use omega::host::CommandHost;

use super::Echo;

/// Command exports shared by the executable and system document.
pub struct Host;

impl Host {
    pub fn declaration() -> CommandHost {
        CommandHost::new("{host_name}", env!("CARGO_PKG_VERSION")).command::<Echo>()
    }
}
