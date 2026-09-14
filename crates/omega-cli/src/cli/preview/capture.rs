use super::PreviewCmd;
use crate::ui::{Step, Ui};
use omega_host::{AtomicFile, Layout};
use omega_proto::omega::PreviewSnapshot;
use sha2::{Digest, Sha256};
use std::{io::Cursor, path::Path};

#[derive(Debug)]
pub(super) struct Capture;
impl Capture {
    pub(super) fn finish(
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
        let version = std::process::Command::new("quickshell")
            .arg("--version")
            .output()?;
        let font = std::process::Command::new("fc-match")
            .args(["--format=%{file}", "DejaVu Sans"])
            .output()?;
        anyhow::ensure!(font.status.success(), "cannot determine capture font");
        let font_path = String::from_utf8(font.stdout)?;
        let font_hash = format!("{:x}", Sha256::digest(std::fs::read(font_path)?));
        let metadata = serde_json::json!({
            "format": 1, "raster_libraries": Self::raster_libraries(renderer_pid)?, "renderer": env!("CARGO_PKG_VERSION"),
            "quickshell": String::from_utf8_lossy(&version.stdout).trim(),
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
