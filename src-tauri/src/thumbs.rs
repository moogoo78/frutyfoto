use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::sync::Once;

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageDecoder, ImageReader};
use image_hasher::{HashAlg, HasherConfig};

use crate::error::Result;

pub const THUMB_SIZE: u32 = 400;
/// Long edge of full-size previews (only made for formats the webview can't show).
pub const PREVIEW_SIZE: u32 = 2560;

/// Result of decoding a photo once: its upright dimensions, thumbnail and perceptual hash.
pub struct Processed {
    pub width: u32,
    pub height: u32,
    pub phash: u64,
}

/// Decodes `src`, applies orientation, writes a JPEG thumbnail to `thumb` (and a
/// full-size JPEG to `preview` if given) and computes a 64-bit DCT perceptual hash.
pub fn process(src: &Path, thumb: &Path, preview: Option<&Path>) -> Result<Processed> {
    let img = decode_upright(src)?;
    let (width, height) = (img.width(), img.height());
    if let Some(preview) = preview {
        write_preview(&img, preview)?;
    }
    let small = img.thumbnail(THUMB_SIZE, THUMB_SIZE);
    drop(img);
    write_jpeg(&small, thumb, 82)?;

    Ok(Processed { width, height, phash: phash(&small) })
}

/// Writes a JPEG of `img` scaled down to at most `PREVIEW_SIZE` on the long edge.
pub fn write_preview(img: &DynamicImage, path: &Path) -> Result<()> {
    if img.width().max(img.height()) > PREVIEW_SIZE {
        let big = img.resize(PREVIEW_SIZE, PREVIEW_SIZE, image::imageops::FilterType::Lanczos3);
        write_jpeg(&big, path, 88)
    } else {
        write_jpeg(img, path, 88)
    }
}

fn write_jpeg(img: &DynamicImage, path: &Path, quality: u8) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut out = BufWriter::new(File::create(path)?);
    JpegEncoder::new_with_quality(&mut out, quality).encode_image(&img.to_rgb8())?;
    Ok(())
}

/// Decodes an image upright. HEIF/HEIC goes through libheif, which applies the
/// container's rotation/mirroring itself (its decoder reports no EXIF orientation).
pub fn decode_upright(src: &Path) -> Result<DynamicImage> {
    static HEIF_HOOKS: Once = Once::new();
    HEIF_HOOKS.call_once(libheif_rs::integration::image::register_all_decoding_hooks);

    let mut decoder = ImageReader::open(src)?.with_guessed_format()?.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(img)
}

pub fn phash(img: &DynamicImage) -> u64 {
    let hasher = HasherConfig::new()
        .hash_size(8, 8)
        .preproc_dct()
        .hash_alg(HashAlg::Mean)
        .to_hasher();
    let hash = hasher.hash_image(img);
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&hash.as_bytes()[..8]);
    u64::from_be_bytes(bytes)
}
