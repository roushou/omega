//! A macro-free executable command provider. Register this binary as a command host.
use omega::{Command, host::CommandHost};

struct Echo;
impl omega::command::Construct for Echo {
    type Dependencies = ();
    fn construct((): ()) -> Self {
        Self
    }
}

impl Command for Echo {
    type Input = String;
    type Output = String;
    const ID: &'static str = "example.echo";
    const DESCRIPTION: &'static str = "Return the supplied text";
    async fn call(&self, text: String) -> omega::Result<String> {
        Ok(text)
    }
}

fn main() -> omega::Result<()> {
    CommandHost::new("echo-host", env!("CARGO_PKG_VERSION"))
        .command::<Echo>()
        .run()
}
