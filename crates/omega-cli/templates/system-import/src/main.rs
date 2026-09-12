mod shell_import;

fn main() -> omega_document::Result<()> {
    omega_document::Document::new().shell(shell_import::shell()?)?.emit()?;
    Ok(())
}
