use api::infrastructure::ml::image_analyzer::ImageAnalyzer;
use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use std::{io::Cursor, path::Path, sync::Arc};

/// Exercise the real native runtime and the shipped model after toolchain updates.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let model = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "models/text_detector.onnx".into());
    anyhow::ensure!(Path::new(&model).is_file(), "ONNX model file is missing");
    let images = Arc::new(api::infrastructure::images::ImageProcessor::new(
        api::limits::Limits::bundled()?.images,
    ));
    let detector = ImageAnalyzer::new(&model, true, images)?;
    let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(640, 640, Rgb([240, 240, 240])));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, ImageFormat::Png)?;
    detector.check_model(bytes.into_inner().into()).await?;
    println!("ONNX model inference succeeded");
    Ok(())
}
