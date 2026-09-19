use omega::{Command, command::Construct};

/// Return the supplied text.
#[derive(Debug)]
pub struct Echo;

impl Construct for Echo {
    type Dependencies = ();

    fn construct((): Self::Dependencies) -> Self {
        Self
    }
}

impl Command for Echo {
    type Input = String;
    type Output = String;

    const ID: &'static str = "{host_name}.echo";
    const DESCRIPTION: &'static str = "Return the supplied text";

    async fn call(&self, text: String) -> omega::Result<String> {
        Ok(text)
    }
}
