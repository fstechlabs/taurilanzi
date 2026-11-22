use std::collections::BTreeMap;
use std::io::{Cursor, Write};

use serde_json::{Map, Value, json};
use zip::CompressionMethod;
use zip::write::{SimpleFileOptions, ZipWriter};

use super::protocol::{HEADER_SIZE, REPORT_SIZE};
use super::{COLUMNS, KEY_COUNT, WIDE_KEY};

const FORBIDDEN_BOUNDARY_BYTES: [u8; 2] = [0x00, 0x7c];
const MAX_PADDING: usize = 4096;
/// Each character moves both copies of the name, in the entry and in the directory at the end.
const MAX_NAME_PADDING: usize = 200;

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("no padding makes the archive safe to stream")]
    Unsafe,
}

/// Which keys an archive covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Every key; keys without an image are cleared.
    Full,
    /// Only the keys present; the rest keep what the dock already shows.
    Partial,
}

pub fn key_id(index: u8) -> String {
    format!("{}_{}", index % COLUMNS, index / COLUMNS)
}

/// Builds the page ZIP from PNG images keyed by key index (`WIDE_KEY` for the wide key).
///
/// File names change on every call (`upload_id`) so the dock never reuses a stale image.
pub fn build(
    images: &BTreeMap<u8, Vec<u8>>,
    scope: Scope,
    upload_id: u64,
) -> Result<Vec<u8>, ArchiveError> {
    let mut manifest = Map::new();
    let mut files = Vec::new();

    let indices: Vec<u8> = match scope {
        Scope::Full => (0..KEY_COUNT).chain([WIDE_KEY]).collect(),
        Scope::Partial => images.keys().copied().collect(),
    };
    for index in indices {
        let entry = match images.get(&index) {
            Some(png) => {
                let name = format!("Images/{upload_id}_{index}.png");
                files.push((name.clone(), png.as_slice()));
                json!({ "State": 0, "ViewParam": [{ "Icon": name }] })
            }
            None => json!({ "State": 0, "ViewParam": [{ "Font": "", "Text": "" }] }),
        };
        manifest.insert(key_id(index), entry);
    }
    let manifest = serde_json::to_vec(&Value::Object(manifest)).expect("manifest serializes");

    for padding in 0..MAX_PADDING {
        let zip = write_zip(&files, &manifest, padding)?;
        if is_stream_safe(&zip) {
            return Ok(zip);
        }
    }
    Err(ArchiveError::Unsafe)
}

/// The screensaver ZIP: a single PNG of the whole panel, as the firmware expects. With no room
/// for a padding entry, the length of the file name shifts the bytes instead.
pub fn screensaver(png: &[u8]) -> Result<Vec<u8>, ArchiveError> {
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for padding in 0..MAX_NAME_PADDING {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file(format!("screensaver{}.png", "_".repeat(padding)), deflated)?;
        zip.write_all(png)?;
        let zip = zip.finish()?.into_inner();
        if is_stream_safe(&zip) {
            return Ok(zip);
        }
    }
    Err(ArchiveError::Unsafe)
}

/// The firmware parses the ZIP while it streams and silently drops keys when a
/// continuation report starts with 0x00 or 0x7C.
pub fn is_stream_safe(zip: &[u8]) -> bool {
    (REPORT_SIZE..HEADER_SIZE + zip.len())
        .step_by(REPORT_SIZE)
        .all(|offset| !FORBIDDEN_BOUNDARY_BYTES.contains(&zip[offset - HEADER_SIZE]))
}

fn write_zip(
    files: &[(String, &[u8])],
    manifest: &[u8],
    padding: usize,
) -> Result<Vec<u8>, ArchiveError> {
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));

    // Leading padding shifts every later entry until no report boundary hits a forbidden byte.
    zip.start_file("pad.txt", stored)?;
    zip.write_all(&vec![b'p'; padding])?;
    zip.add_directory("Images/", stored)?;
    for (name, data) in files {
        zip.start_file(name.as_str(), deflated)?;
        zip.write_all(data)?;
    }
    zip.start_file("manifest.json", deflated)?;
    zip.write_all(manifest)?;
    // Public projects report the firmware may lose the last entry, so it is a throwaway.
    zip.start_file("end.txt", stored)?;

    Ok(zip.finish()?.into_inner())
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    fn noisy_png(seed: u8, len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
            .collect()
    }

    fn read_entries(zip: &[u8]) -> Vec<String> {
        let archive = zip::ZipArchive::new(Cursor::new(zip)).unwrap();
        (0..archive.len())
            .map(|i| archive.name_for_index(i).unwrap().unwrap().into_owned())
            .collect()
    }

    fn read_manifest(zip: &[u8]) -> Value {
        let mut archive = zip::ZipArchive::new(Cursor::new(zip)).unwrap();
        let mut text = String::new();
        archive
            .by_name("manifest.json")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn the_screensaver_is_one_safe_entry() {
        for seed in 0..20 {
            let zip = screensaver(&noisy_png(seed, 40_000)).unwrap();
            assert!(is_stream_safe(&zip));
            let entries = read_entries(&zip);
            assert_eq!(entries.len(), 1);
            assert!(entries[0].starts_with("screensaver") && entries[0].ends_with(".png"));
        }
    }

    #[test]
    fn key_ids_follow_column_row() {
        assert_eq!(key_id(0), "0_0");
        assert_eq!(key_id(7), "2_1");
        assert_eq!(key_id(12), "2_2");
        assert_eq!(key_id(WIDE_KEY), "3_2");
    }

    #[test]
    fn full_page_lists_every_key_and_clears_missing_ones() {
        let images = BTreeMap::from([(0, noisy_png(1, 900)), (WIDE_KEY, noisy_png(2, 1500))]);
        let zip = build(&images, Scope::Full, 7).unwrap();
        let manifest = read_manifest(&zip);
        assert_eq!(manifest.as_object().unwrap().len(), KEY_COUNT as usize + 1);
        assert_eq!(manifest["0_0"]["ViewParam"][0]["Icon"], "Images/7_0.png");
        assert_eq!(manifest["3_2"]["ViewParam"][0]["Icon"], "Images/7_13.png");
        assert_eq!(manifest["1_0"]["ViewParam"][0]["Text"], "");
    }

    #[test]
    fn partial_page_only_lists_given_keys() {
        let images = BTreeMap::from([(5, noisy_png(3, 800))]);
        let manifest = read_manifest(&build(&images, Scope::Partial, 1).unwrap());
        assert_eq!(
            manifest.as_object().unwrap().keys().collect::<Vec<_>>(),
            ["0_1"]
        );
    }

    #[test]
    fn padding_comes_first_and_throwaway_last() {
        let images = BTreeMap::from([(3, noisy_png(4, 500))]);
        let entries = read_entries(&build(&images, Scope::Partial, 1).unwrap());
        assert_eq!(entries.first().unwrap(), "pad.txt");
        assert_eq!(entries.last().unwrap(), "end.txt");
    }

    #[test]
    fn archives_are_always_safe_to_stream() {
        for seed in 0..40u8 {
            let images: BTreeMap<u8, Vec<u8>> = (0..KEY_COUNT)
                .map(|i| {
                    (
                        i,
                        noisy_png(
                            seed.wrapping_add(i),
                            700 + seed as usize * 97 + i as usize * 13,
                        ),
                    )
                })
                .collect();
            let zip = build(&images, Scope::Full, seed as u64).unwrap();
            assert!(is_stream_safe(&zip), "seed {seed}");
        }
    }

    #[test]
    fn detects_forbidden_byte_on_a_boundary() {
        let mut zip = vec![1u8; 3000];
        assert!(is_stream_safe(&zip));
        zip[REPORT_SIZE - HEADER_SIZE] = 0x7c;
        assert!(!is_stream_safe(&zip));
    }
}
