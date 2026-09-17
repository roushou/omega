//! Operator requests authenticated by the daemon owner's peer UID.
//! Operator connections do not use plugin spawn tokens.

use omega_proto::omega::{
    Act, Action, AdoptPlugin, InvokePlugin, Value, action, invoke, result, value,
};
use omega_proto::{Client, ClientError, CommandAnswer, PluginName, Socket, SurfaceId};

#[derive(Debug, thiserror::Error)]
pub enum OperatorError {
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

    pub async fn present(
        &self,
        request: omega_proto::omega::CreateInstance,
    ) -> Result<omega_proto::omega::InstanceList, OperatorError> {
        match self.invoke(invoke::Op::CreateInstance(request)).await? {
            result::Outcome::Instances(instances) => Ok(instances),
            _ => Err(OperatorError::Unexpected(
                "CreateInstance",
                "instance snapshot",
            )),
        }
    }

    pub async fn deployment(&self) -> Result<omega_proto::omega::DeploymentStatus, OperatorError> {
        match self
            .invoke(invoke::Op::GetDeployment(
                omega_proto::omega::GetDeployment {},
            ))
            .await?
        {
            result::Outcome::Deployment(status) => Ok(status),
            _ => Err(OperatorError::Unexpected(
                "GetDeployment",
                "deployment status",
            )),
        }
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

    /// Ask the daemon to cycle a plugin's process.
    pub async fn restart(&self, plugin_name: &PluginName) -> Result<(), OperatorError> {
        self.invoke(invoke::Op::RestartPlugin(
            omega_proto::omega::RestartPlugin {
                plugin: plugin_name.to_string(),
            },
        ))
        .await
        .map(|_| ())
    }

    /// Call a plugin's command surface, and hand back whatever it answered.
    pub async fn run(
        &self,
        plugin_name: &PluginName,
        command_id: &SurfaceId,
        args: Vec<Value>,
    ) -> Result<Option<Value>, OperatorError> {
        let outcome = self
            .invoke(invoke::Op::Act(Act {
                action: Some(Action {
                    kind: Some(action::Kind::InvokePlugin(InvokePlugin {
                        plugin: plugin_name.to_string(),
                        command: command_id.to_string(),
                        args,
                    })),
                }),
            }))
            .await?;

        Ok(
            match CommandAnswer::try_from(outcome).map_err(ClientError::from)? {
                CommandAnswer::Value(value) => Some(value),
                CommandAnswer::Acknowledged => None,
            },
        )
    }

    /// Open a persistent operator connection for session-scoped operations.
    pub async fn attach(&self) -> Result<Attached, OperatorError> {
        let (client, _welcome) = Client::connect(&self.socket, "", "").await?;
        Ok(Attached { client })
    }

    /// One request, on a connection that lasts exactly as long as it does.
    async fn invoke(&self, op: invoke::Op) -> Result<result::Outcome, OperatorError> {
        // No token: the CLI is not a plugin, and saying so is the point.
        let (mut client, _welcome) = Client::connect(&self.socket, "", "").await?;

        let stream = client.allocate();
        client.invoke(stream, op).await?;
        Ok(client.answer(stream).await?)
    }
}

/// Persistent operator session. Closing it releases adopted plugin identities.
#[derive(Debug)]
pub struct Attached {
    client: Client,
}

impl Attached {
    /// Adopt a plugin and return its spawn token, valid until this connection closes.
    pub async fn adopt(&mut self, plugin_name: &PluginName) -> Result<String, OperatorError> {
        let outcome = self
            .request(invoke::Op::AdoptPlugin(AdoptPlugin {
                plugin: plugin_name.to_string(),
            }))
            .await?;

        match outcome {
            result::Outcome::Value(Value {
                kind: Some(value::Kind::StringValue(token)),
            }) => Ok(token),
            _ => Err(OperatorError::Unexpected("AdoptPlugin", "a token")),
        }
    }

    /// Read incoming frames and answer keepalives until disconnected.
    /// Run concurrently with work that requires the operator session to remain open.
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
