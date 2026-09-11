//! Talking to the daemon as its owner.
//!
//! The CLI is not a unit: it holds no spawn token and asks for none of a
//! unit's powers. What it is, is the user who owns the daemon — and the
//! daemon decides that from the connection's uid, not from anything said
//! here.

use omega_proto::omega::{
    Act, Action, AdoptUnit, InvokeUnit, Value, action, invoke, result, value,
};
use omega_proto::{Client, ClientError, Socket};

#[derive(Debug, thiserror::Error)]
pub enum OperatorError {
    /// Everything that can go wrong between a peer and the daemon is the
    /// client's to describe; the CLI only decides how to say it.
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error("the daemon answered {0} with something other than {1}")]
    Unexpected(&'static str, &'static str),
}

/// One request to the daemon, as the operator.
#[derive(Debug)]
pub struct Operator {
    socket: Socket,
}

impl Operator {
    pub fn new() -> Self {
        Self {
            socket: Socket::resolve(),
        }
    }

    pub fn at(socket: Socket) -> Self {
        Self { socket }
    }

    pub async fn daemon_version(&self) -> Result<String, OperatorError> {
        let (_client, welcome) = Client::connect(&self.socket, "", "").await?;
        Ok(welcome.daemon_version)
    }

    pub async fn apply_shell(&self, overwrite: bool) -> Result<(), OperatorError> {
        self.invoke(invoke::Op::ApplyShell(omega_proto::omega::ApplyShell {
            overwrite,
        }))
        .await
        .map(|_| ())
    }

    /// Ask the daemon to cycle a unit's process.
    pub async fn restart(&self, unit: &str) -> Result<(), OperatorError> {
        self.invoke(invoke::Op::RestartUnit(omega_proto::omega::RestartUnit {
            unit: unit.to_string(),
        }))
        .await
        .map(|_| ())
    }

    /// Call a unit's command surface, and hand back whatever it answered.
    pub async fn run(
        &self,
        unit: &str,
        command: &str,
        args: Vec<Value>,
    ) -> Result<Option<Value>, OperatorError> {
        let outcome = self
            .invoke(invoke::Op::Act(Act {
                action: Some(Action {
                    kind: Some(action::Kind::InvokeUnit(InvokeUnit {
                        unit: unit.to_string(),
                        command: command.to_string(),
                        args,
                    })),
                }),
            }))
            .await?;

        Ok(match outcome {
            result::Outcome::Value(value) => Some(value),
            _ => None,
        })
    }

    /// A connection held open rather than spent on one request.
    ///
    /// Some things the daemon does for an operator last as long as the asking
    /// connection does — adopting a unit is one — so the connection has to be
    /// something the caller keeps.
    pub async fn attach(&self) -> Result<Attached, OperatorError> {
        let (client, _welcome) = Client::connect(&self.socket, "", "").await?;
        Ok(Attached { client })
    }

    /// One request, on a connection that lasts exactly as long as it does.
    async fn invoke(&self, op: invoke::Op) -> Result<result::Outcome, OperatorError> {
        // No token: the CLI is not a unit, and saying so is the point.
        let (mut client, _welcome) = Client::connect(&self.socket, "", "").await?;

        let stream = client.allocate();
        client.invoke(stream, op).await?;
        Ok(client.answer(stream).await?)
    }
}

/// An operator connection the caller is holding on to.
///
/// What it authorizes ends when it does, which is the point: close the
/// terminal and the daemon takes its units back.
#[derive(Debug)]
pub struct Attached {
    client: Client,
}

impl Attached {
    /// Take a unit's place, and get the token a process of our own connects
    /// with. Valid until this connection closes.
    pub async fn adopt(&mut self, unit: &str) -> Result<String, OperatorError> {
        let outcome = self
            .request(invoke::Op::AdoptUnit(AdoptUnit {
                unit: unit.to_string(),
            }))
            .await?;

        match outcome {
            result::Outcome::Value(Value {
                kind: Some(value::Kind::StringValue(token)),
            }) => Ok(token),
            _ => Err(OperatorError::Unexpected("AdoptUnit", "a token")),
        }
    }

    /// Answer the daemon's keepalives until it goes away.
    ///
    /// A held connection nobody reads is a connection the daemon closes: it
    /// pings, hears nothing, and concludes the peer is wedged. Reading is how
    /// the connection stays alive, so a caller holding one runs this
    /// alongside whatever it is holding it for.
    pub async fn hold(&mut self) -> Result<(), OperatorError> {
        while self.client.recv().await?.is_some() {}
        Ok(())
    }

    async fn request(&mut self, op: invoke::Op) -> Result<result::Outcome, OperatorError> {
        let stream = self.client.allocate();
        self.client.invoke(stream, op).await?;
        Ok(self.client.answer(stream).await?)
    }
}

impl Default for Operator {
    fn default() -> Self {
        Self::new()
    }
}
