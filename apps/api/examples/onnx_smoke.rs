use api::infrastructure::ml::{onnx_text_detector::OnnxTextDetector, traits::MlService};
use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use std::{io::Cursor, path::Path};

/// Exercise the real native runtime and the shipped model after toolchain updates.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let model = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "models/text_detector.onnx".into());
    anyhow::ensure!(Path::new(&model).is_file(), "ONNX model file is missing");
    let detector = OnnxTextDetector::new(&model, true)?;
    let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(640, 640, Rgb([240, 240, 240])));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, ImageFormat::Png)?;
    detector.detect_text(bytes.get_ref()).await?;
    println!("ONNX model inference succeeded");
    Ok(())
}
