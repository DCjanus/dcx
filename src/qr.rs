use std::io::Write;
use std::path::Path;

use anyhow::{Context, bail};
use qrcode::{
    QrCode,
    render::{svg, unicode},
};

use crate::AnyResult;

/// 生成带标准留白的二维码；文件格式由扩展名决定。
pub fn generate(
    text: &str,
    output: Option<&Path>,
    terminal: bool,
    mut writer: impl Write,
) -> AnyResult {
    if text.is_empty() {
        bail!("QR code input must not be empty");
    }
    let code = QrCode::new(text.as_bytes()).context("failed to encode QR code")?;
    if let Some(path) = output {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if extension.eq_ignore_ascii_case("svg") {
            let image = code.render::<svg::Color>().module_dimensions(8, 8).build();
            std::fs::write(path, image)
                .with_context(|| format!("failed to write {}", path.display()))?;
        } else if extension.eq_ignore_ascii_case("png") {
            code.render::<image::Luma<u8>>()
                .module_dimensions(8, 8)
                .build()
                .save_with_format(path, image::ImageFormat::Png)
                .with_context(|| format!("failed to write {}", path.display()))?;
        } else {
            bail!("output file must have a .png or .svg extension");
        }
    } else {
        let image = code.render::<unicode::Dense1x2>().build();
        for line in image.lines() {
            // 明确设置前景和背景，兼容深色与浅色终端；重定向时保持纯文本。
            if terminal {
                writeln!(writer, "\x1b[30;107m{line}\x1b[0m")?;
            } else {
                writeln!(writer, "{line}")?;
            }
        }
        writer.flush()?;
    }
    Ok(())
}
