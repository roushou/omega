//! Binding: a stale socket is cleared, a live one is not stolen.

use std::path::PathBuf;

use omega_proto::Socket;

struct TempPath(PathBuf);

impl TempPath {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(std::env::temp_dir().join(format!("omega-sock-{tag}-{nanos}.sock")))
    }

    fn socket(&self) -> Socket {
        Socket::at(self.0.clone())
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[tokio::test]
async fn a_stale_socket_file_is_replaced() {
    let path = TempPath::new("stale");
    let socket = path.socket();

    // A crashed process leaves the file behind with nobody listening.
    drop(socket.bind().unwrap());
    assert!(socket.path().exists());
    assert!(!socket.is_live());

    socket
        .bind()
        .expect("a stale socket must not block a restart");
}

#[tokio::test]
async fn a_live_socket_is_never_stolen() {
    let path = TempPath::new("live");
    let socket = path.socket();
    let _listener = socket.bind().unwrap();

    assert!(socket.is_live());

    let err = socket
        .bind()
        .expect_err("a second daemon must not take over");
    assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);

    // The original is still serving.
    assert!(socket.connect().await.is_ok());
}
