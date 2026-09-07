//! Raw debug client: connect and print every frame as canonical JSON.
//!
//! This is not a unit — it has no token — so the daemon must be started with
//! `omega daemon --allow-debug-clients` for it to be admitted at all, and it
//! is granted nothing beyond watching state.

use omega_proto::{Handshake, Refusal, Socket};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket = std::env::args()
        .nth(1)
        .map(Socket::at)
        .unwrap_or_else(Socket::resolve);

    let mut transport = socket.connect().await?;
    transport
        .send(Handshake::hello(
            "debug-client",
            &Handshake::token_from_env(),
        ))
        .await?;

    while let Some(frame) = transport.recv().await? {
        if let Some(refusal) = Refusal::of(&frame) {
            eprintln!("refused: {refusal}");
            return Ok(());
        }
        println!("{}", serde_json::to_string(&frame)?);
    }
    Ok(())
}
