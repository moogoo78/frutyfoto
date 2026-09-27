use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageDecoder, ImageReader};
use image_hasher::{HashAlg, HasherConfig};

use crate::error::Result;

pub const THUMB_SIZE: u32 = 400;

/// Result of decoding a photo once: its upright dimensions, thumbnail and perceptual hash.
pub struct Processed {
    pub width: u32,
    pub height: u32,
    pub phash: u64,
}

/// Decodes `src`, applies EXIF orientation, writes a JPEG thumbnail to `thumb` and
/// computes a 64-bit DCT perceptual hash.
pub fn process(src: &Path, thumb: &Path) -> Result<Processed> {
    let img = decode_upright(src)?;
    let (width, height) = (img.width(), img.height());
    let small = img.thumbnail(THUMB_SIZE, THUMB_SIZE);
    drop(img);

    if let Some(dir) = thumb.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut out = BufWriter::new(File::create(thumb)?);
    JpegEncoder::new_with_quality(&mut out, 82).encode_image(&small.to_rgb8())?;

    Ok(Processed { width, height, phash: phash(&small) })
}

pub fn decode_upright(src: &Path) -> Result<DynamicImage> {
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
