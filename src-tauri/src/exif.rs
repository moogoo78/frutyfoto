use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use chrono::{NaiveDate, NaiveDateTime};
use exif::{Exif, In, Tag, Value};

#[derive(Debug, Default, Clone)]
pub struct ExifData {
    pub taken_at: Option<NaiveDateTime>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub iso: Option<i64>,
    pub f_number: Option<f64>,
    pub exposure: Option<String>,
    pub focal_len: Option<f64>,
    pub gps_lat: Option<f64>,
    pub gps_lon: Option<f64>,
    pub orientation: Option<i64>,
}

/// Reads EXIF from any supported container. Missing or broken EXIF yields defaults.
pub fn read(path: &Path) -> ExifData {
    let Ok(file) = File::open(path) else { return ExifData::default() };
    let Ok(exif) = exif::Reader::new().read_from_container(&mut BufReader::new(file)) else {
        return ExifData::default();
    };
    from_exif(&exif)
}

fn from_exif(exif: &Exif) -> ExifData {
    let taken_at = [Tag::DateTimeOriginal, Tag::DateTimeDigitized, Tag::DateTime]
        .into_iter()
        .find_map(|t| datetime(exif, t));
    ExifData {
        taken_at,
        camera_make: ascii(exif, Tag::Make),
        camera_model: ascii(exif, Tag::Model),
        lens: ascii(exif, Tag::LensModel),
        iso: uint(exif, Tag::PhotographicSensitivity).map(i64::from),
        f_number: rational(exif, Tag::FNumber, 0),
        exposure: exposure(exif),
        focal_len: rational(exif, Tag::FocalLength, 0),
        gps_lat: gps(exif, Tag::GPSLatitude, Tag::GPSLatitudeRef, "S"),
        gps_lon: gps(exif, Tag::GPSLongitude, Tag::GPSLongitudeRef, "W"),
        orientation: uint(exif, Tag::Orientation).map(i64::from),
    }
}

fn ascii(exif: &Exif, tag: Tag) -> Option<String> {
    match &exif.get_field(tag, In::PRIMARY)?.value {
        Value::Ascii(v) if !v.is_empty() => {
            let s = String::from_utf8_lossy(&v[0]);
            let s = s.trim_matches(char::from(0)).trim();
            (!s.is_empty()).then(|| s.to_string())
        }
        _ => None,
    }
}

fn uint(exif: &Exif, tag: Tag) -> Option<u32> {
    exif.get_field(tag, In::PRIMARY)?.value.get_uint(0)
}

fn rational(exif: &Exif, tag: Tag, idx: usize) -> Option<f64> {
    match &exif.get_field(tag, In::PRIMARY)?.value {
        Value::Rational(v) => v.get(idx).filter(|r| r.denom != 0).map(|r| r.to_f64()),
        _ => None,
    }
}

fn datetime(exif: &Exif, tag: Tag) -> Option<NaiveDateTime> {
    let Value::Ascii(v) = &exif.get_field(tag, In::PRIMARY)?.value else { return None };
    let dt = exif::DateTime::from_ascii(v.first()?).ok()?;
    NaiveDate::from_ymd_opt(dt.year.into(), dt.month.into(), dt.day.into())?
        .and_hms_opt(dt.hour.into(), dt.minute.into(), dt.second.into())
}

fn exposure(exif: &Exif) -> Option<String> {
    let Value::Rational(v) = &exif.get_field(Tag::ExposureTime, In::PRIMARY)?.value else {
        return None;
    };
    let r = v.first().filter(|r| r.num != 0 && r.denom != 0)?;
    let secs = r.to_f64();
    Some(if secs < 1.0 {
        format!("1/{}", (1.0 / secs).round())
    } else {
        format!("{secs}s")
    })
}

fn gps(exif: &Exif, tag: Tag, ref_tag: Tag, negative_ref: &str) -> Option<f64> {
    let deg = rational(exif, tag, 0)?;
    let min = rational(exif, tag, 1).unwrap_or(0.0);
    let sec = rational(exif, tag, 2).unwrap_or(0.0);
    let value = deg + min / 60.0 + sec / 3600.0;
    let negative = ascii(exif, ref_tag).is_some_and(|r| r.eq_ignore_ascii_case(negative_ref));
    Some(if negative { -value } else { value })
}
