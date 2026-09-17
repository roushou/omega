use super::PreviewCmd;
use crate::ui::{Step, Ui};
use anyhow::Context;
use omega_host::{
    AtomicFile, Layout,
    process::{OutputLimits, Process},
};
use omega_proto::omega::PreviewSnapshot;
use sha2::{Digest, Sha256};
use std::{io::Cursor, path::Path, time::Duration};
use tokio::process::Command;

#[derive(Debug)]
pub(super) struct Capture;
impl Capture {
    pub(super) async fn finish(
        cmd: &PreviewCmd,
        source: &Path,
        snapshot: &PreviewSnapshot,
        renderer_pid: u32,
        ui: &mut Ui,
    ) -> anyhow::Result<()> {
        let destination = cmd.capture.as_ref().expect("capture mode");
        if let Some(baseline) = &cmd.baseline {
            let actual = Self::identity(destination)?;
            let expected = Self::identity(baseline)?;
            anyhow::ensure!(
                actual != expected,
                "capture and baseline must be different files"
            );
        }
        let bytes = std::fs::read(source)?;
        let image = Self::decode(&bytes)?;
        anyhow::ensure!(
            image.0 == cmd.width && image.1 == cmd.height,
            "capture dimensions differ from the requested viewport"
        );
        let mut version = Command::new("quickshell");
        version.arg("--version");
        let version = Self::probe(version, "quickshell --version").await?;

        let mut font = Command::new("fc-match");
        font.args(["--format=%{file}", "DejaVu Sans"]);
        let font_path = Self::probe(font, "fc-match for DejaVu Sans").await?;
        let font_bytes = std::fs::read(&font_path)
            .with_context(|| format!("cannot read capture font {font_path}"))?;
        let font_hash = format!("{:x}", Sha256::digest(font_bytes));
        let metadata = serde_json::json!({
            "format": 1, "raster_libraries": Self::raster_libraries(renderer_pid)?, "renderer": env!("CARGO_PKG_VERSION"),
            "quickshell": version.trim(),
            "font_sha256": font_hash, "font": "DejaVu Sans", "backend": "software", "platform": "offscreen",
            "scale": 1, "dpi": 96, "width": cmd.width, "height": cmd.height,
            "theme": cmd.theme, "case": snapshot.selected,
        });
        AtomicFile::at(destination).write(&bytes)?;
        AtomicFile::at(Layout::preview_metadata(destination))
            .write(&serde_json::to_vec_pretty(&metadata)?)?;
        ui.step(Step::Created, destination.display());
        if let Some(baseline) = &cmd.baseline {
            if cmd.update_baseline {
                AtomicFile::at(baseline).write(&bytes)?;
                AtomicFile::at(Layout::preview_metadata(baseline))
                    .write(&serde_json::to_vec_pretty(&metadata)?)?;
                ui.step(Step::Created, format!("baseline {}", baseline.display()));
            } else {
                let before: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(Layout::preview_metadata(baseline))?)?;
                anyhow::ensure!(
                    before == metadata,
                    "capture environment differs from the baseline; compare metadata before updating it"
                );
                let expected = Self::decode(&std::fs::read(baseline)?)?;
                anyhow::ensure!(
                    (image.0, image.1) == (expected.0, expected.1),
                    "baseline dimensions differ"
                );
                let changed = image
                    .2
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(expected.2.as_chunks::<4>().0.iter())
                    .filter(|(a, b)| a != b)
                    .count();
                if changed != 0 {
                    let mut difference = Vec::with_capacity(image.2.len());
                    for (actual, expected) in image
                        .2
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .zip(expected.2.as_chunks::<4>().0.iter())
                    {
                        difference.extend_from_slice(if actual == expected {
                            &[0, 0, 0, 255]
                        } else {
                            &[255, 0, 128, 255]
                        });
                    }
                    let path = Layout::preview_difference(destination);
                    AtomicFile::at(&path).write(&Self::encode(image.0, image.1, &difference)?)?;
                    anyhow::bail!(
                        "{changed} pixels differ; comparison artifact: {}",
                        path.display()
                    );
                }
                ui.step(Step::Checked, "pixels match the baseline");
            }
        }
        Ok(())
    }

    async fn probe(command: Command, description: &str) -> anyhow::Result<String> {
        let output = Process::new(command)
            .timeout(Duration::from_secs(10))
            .capture(OutputLimits {
                stdout: 64 * 1024,
                stderr: 64 * 1024,
            })
            .await
            .with_context(|| format!("cannot query capture environment: {description}"))?;
        anyhow::ensure!(
            output.status.success(),
            "capture environment query {description} failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
        let value = String::from_utf8(output.stdout).with_context(|| {
            format!("capture environment query {description} returned invalid UTF-8")
        })?;
        anyhow::ensure!(
            !value.trim().is_empty(),
            "capture environment query {description} returned an empty response"
        );
        Ok(value)
    }

    fn raster_libraries(pid: u32) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
        let maps = std::fs::read_to_string(Layout::preview_process_maps(pid))?;
        let mut paths = std::collections::BTreeSet::new();
        for line in maps.lines() {
            let Some(start) = line.find('/') else {
                continue;
            };
            let path = Path::new(&line[start..]);
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if ["libQt6", "libfreetype", "libfontconfig", "libharfbuzz"]
                .iter()
                .any(|prefix| name.starts_with(prefix))
                || name.ends_with(".ttf")
                || name.ends_with(".otf")
            {
                paths.insert(path.to_path_buf());
            }
        }
        anyhow::ensure!(
            !paths.is_empty(),
            "cannot identify the preview raster environment"
        );
        paths
            .into_iter()
            .map(|path| {
                let digest = Sha256::digest(std::fs::read(&path)?);
                Ok((
                    path.file_name().unwrap().to_string_lossy().into_owned(),
                    format!("{digest:x}"),
                ))
            })
            .collect()
    }
    fn identity(path: &Path) -> anyhow::Result<std::path::PathBuf> {
        if path.exists() {
            return Ok(std::fs::canonicalize(path)?);
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        match std::fs::canonicalize(parent) {
            Ok(parent) => Ok(parent.join(
                path.file_name()
                    .ok_or_else(|| anyhow::anyhow!("image path has no filename"))?,
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(std::path::absolute(path)?)
            }
            Err(error) => Err(error.into()),
        }
    }
    fn decode(bytes: &[u8]) -> anyhow::Result<(u32, u32, Vec<u8>)> {
        let mut decoder = png::Decoder::new(Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info()?;
        anyhow::ensure!(
            reader.info().width <= 4096 && reader.info().height <= 4096,
            "image exceeds capture dimensions"
        );
        let mut buffer = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buffer)?;
        let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
        match info.color_type {
            png::ColorType::Rgba => rgba.extend_from_slice(&buffer[..info.buffer_size()]),
            png::ColorType::Rgb => {
                for pixel in buffer[..info.buffer_size()].as_chunks::<3>().0 {
                    rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
                }
            }
            png::ColorType::Grayscale => {
                for value in &buffer[..info.buffer_size()] {
                    rgba.extend_from_slice(&[*value, *value, *value, 255]);
                }
            }
            png::ColorType::GrayscaleAlpha => {
                for pixel in buffer[..info.buffer_size()].as_chunks::<2>().0 {
                    rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
                }
            }
            png::ColorType::Indexed => anyhow::bail!("PNG palette was not expanded"),
        }
        Ok((info.width, info.height, rgba))
    }
    fn encode(width: u32, height: u32, bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
        let mut output = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut output, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header()?.write_image_data(bytes)?;
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega_host::process::{Error, Stream};

    struct Probe;

    impl Probe {
        fn command(script: &str) -> Command {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", script]);
            command
        }
    }

    #[tokio::test]
    async fn environment_queries_preserve_paths_and_reject_failed_or_invalid_answers() {
        let path = " /font directory/font.ttf ";
        let mut command = Probe::command("printf '%s' \"$1\"");
        command.args(["probe", path]);
        assert_eq!(Capture::probe(command, "font").await.unwrap(), path);

        for (script, expected) in [
            (
                "printf 'plausible version'; printf 'version failed' >&2; exit 9",
                "version failed",
            ),
            ("printf '  \\n'", "empty response"),
            ("printf '\\377'", "invalid UTF-8"),
        ] {
            let error = Capture::probe(Probe::command(script), "quickshell --version")
                .await
                .unwrap_err();
            let diagnostic = format!("{error:#}");
            assert!(diagnostic.contains("quickshell --version"), "{diagnostic}");
            assert!(diagnostic.contains(expected), "{diagnostic}");
        }
    }

    #[tokio::test]
    async fn environment_queries_bound_both_streams_and_preserve_execution_errors() {
        for (script, expected) in [
            ("head -c 65537 /dev/zero", Stream::Stdout),
            ("head -c 65537 /dev/zero >&2", Stream::Stderr),
        ] {
            let error = Capture::probe(Probe::command(script), "font")
                .await
                .unwrap_err();
            assert!(
                matches!(error.downcast_ref::<Error>(), Some(Error::OutputLimit { stream, limit: 65536 }) if *stream == expected)
            );
        }
        let error = Capture::probe(Command::new("/dev/null/missing"), "font")
            .await
            .unwrap_err();
        assert!(matches!(
            error.downcast_ref::<Error>(),
            Some(Error::Spawn(_))
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn environment_queries_have_a_ten_second_timeout() {
        let error = Capture::probe(Probe::command("exec /bin/sleep 60"), "font")
            .await
            .unwrap_err();
        assert!(
            matches!(error.downcast_ref::<Error>(), Some(Error::Timeout { duration }) if *duration == Duration::from_secs(10))
        );
    }
}
