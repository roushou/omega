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
    drop(std::os::unix::net::UnixListener::bind(socket.path()).unwrap());
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

#[tokio::test]
async fn binding_preserves_regular_files_and_symlinks() {
    let path = TempPath::new("regular");
    std::fs::write(&path.0, b"valuable contents").unwrap();
    assert_eq!(
        path.socket().bind().unwrap_err().kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(std::fs::read(&path.0).unwrap(), b"valuable contents");
    let link = TempPath::new("symlink");
    std::os::unix::fs::symlink(&path.0, &link.0).unwrap();
    assert_eq!(
        link.socket().bind().unwrap_err().kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert!(std::fs::symlink_metadata(&link.0).unwrap().is_symlink());
    assert_eq!(std::fs::read(&path.0).unwrap(), b"valuable contents");
}

#[tokio::test]
async fn listener_drop_cleans_only_its_own_endpoint() {
    let path = TempPath::new("ownership");
    let listener = path.socket().bind().unwrap();
    drop(listener);
    assert!(!path.0.exists());
    let listener = path.socket().bind().unwrap();
    std::fs::remove_file(&path.0).unwrap();
    let replacement = path.socket().bind().unwrap();
    drop(listener);
    assert!(path.socket().connect_stream().await.is_ok());
    drop(replacement);
    assert!(!path.0.exists());
    let listener = path.socket().bind().unwrap();
    std::fs::remove_file(&path.0).unwrap();
    std::fs::write(&path.0, b"replacement").unwrap();
    drop(listener);
    assert_eq!(std::fs::read(&path.0).unwrap(), b"replacement");
}

#[tokio::test(start_paused = true)]
async fn a_full_backlog_is_live_and_connection_attempts_have_a_deadline() {
    let path = TempPath::new("backlog");
    let _listener = path.socket().bind().unwrap();
    let mut connections = Vec::new();
    loop {
        let connection =
            socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None).unwrap();
        connection.set_nonblocking(true).unwrap();
        match connection.connect(&socket2::SockAddr::unix(&path.0).unwrap()) {
            Ok(()) => connections.push(connection),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) => panic!("cannot fill backlog: {error}"),
        }
        assert!(connections.len() < 65536);
    }
    assert!(path.socket().is_live());
    assert_eq!(
        path.socket().bind().unwrap_err().kind(),
        std::io::ErrorKind::AddrInUse
    );
    let error = path.socket().connect_stream().await.unwrap_err();
    // Linux may refuse immediately with EAGAIN; an asynchronous pending connect
    // must instead terminate at its deadline.
    assert!(matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    ));
}
