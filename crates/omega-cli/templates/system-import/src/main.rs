mod shell_import;

fn main() -> omega_document::Result<()> {
    omega_document::Document::new().with(shell_import::shell()?)?.emit()?;
    Ok(())
}
