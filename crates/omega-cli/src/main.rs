use omega_cli::cli::Cli;
use omega_cli::ui::Ui;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    Cli::init_tracing(cli.verbosity());
    let mut ui = Ui::stdio();

    if let Err(err) = cli.dispatch(&mut ui).await {
        ui.error(&err);
        std::process::exit(1);
    }
}
