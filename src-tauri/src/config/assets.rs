use std::collections::HashSet;
use std::fs::{self, File};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

use crate::render::{ImageData, ImageFormat};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundFormat {
    Mp3,
    Wav,
    M4a,
    Flac,
    Ogg,
}

impl SoundFormat {
    pub fn from_mime(mime: &str) -> Option<Self> {
        match mime.split(';').next()?.trim() {
            "audio/mpeg" | "audio/mp3" => Some(Self::Mp3),
            "audio/wav" | "audio/x-wav" | "audio/wave" => Some(Self::Wav),
            "audio/mp4" | "audio/x-m4a" | "audio/aac" => Some(Self::M4a),
            "audio/flac" | "audio/x-flac" => Some(Self::Flac),
            "audio/ogg" => Some(Self::Ogg),
            _ => None,
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Wav => "wav",
            Self::M4a => "m4a",
            Self::Flac => "flac",
            Self::Ogg => "ogg",
        }
    }
}

enum Kind {
    Image(ImageFormat),
    Sound,
}

/// Images and sounds stored by content hash, so the same file is kept once and ids travel
/// between machines.
#[derive(Clone)]
pub struct AssetStore {
    dir: PathBuf,
}

impl AssetStore {
    pub fn open(dir: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    /// Stores the image and returns its id: `<sha256>.<extension>`.
    pub fn put(&self, format: ImageFormat, bytes: &[u8]) -> io::Result<String> {
        self.store(extension(format), bytes)
    }

    pub fn put_sound(&self, format: SoundFormat, bytes: &[u8]) -> io::Result<String> {
        self.store(format.extension(), bytes)
    }

    /// Where a stored sound is, for the player to read it.
    pub fn sound_path(&self, id: &str) -> io::Result<PathBuf> {
        match parse_id(id) {
            Some(Kind::Sound) => Ok(self.dir.join(id)),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a sound id",
            )),
        }
    }

    fn store(&self, extension: &str, bytes: &[u8]) -> io::Result<String> {
        let id = format!("{}.{extension}", hex::encode(Sha256::digest(bytes)));
        let path = self.dir.join(&id);
        if path.exists() {
            // Uploading again counts as new, so `prune` does not take it from an open editor.
            File::options()
                .write(true)
                .open(&path)?
                .set_modified(SystemTime::now())?;
        } else {
            let tmp = self.dir.join(format!(".{id}.tmp"));
            fs::write(&tmp, bytes)?;
            fs::rename(&tmp, &path)?;
        }
        Ok(id)
    }

    /// Deletes the files that are not in `keep` and are older than `min_age`. Recent ones stay
    /// because the editor uploads images before the key that uses them is saved.
    pub fn prune(&self, keep: &HashSet<&str>, min_age: Duration) -> io::Result<usize> {
        let mut removed = 0;
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(id) = name.to_str().filter(|id| parse_id(id).is_some()) else {
                continue;
            };
            if keep.contains(id) {
                continue;
            }
            let age = entry.metadata()?.modified()?.elapsed().unwrap_or_default();
            if age >= min_age {
                fs::remove_file(entry.path())?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    pub fn get(&self, id: &str) -> io::Result<ImageData> {
        let Some(Kind::Image(format)) = parse_id(id) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not an image id",
            ));
        };
        let bytes = fs::read(self.dir.join(id))?;
        Ok(ImageData {
            format,
            bytes: Arc::from(bytes),
        })
    }
}

fn extension(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Webp => "webp",
        ImageFormat::Gif => "gif",
        ImageFormat::Svg => "svg",
    }
}

/// Only `<64 hex chars>.<known extension>` is accepted, which also rules out path traversal.
fn parse_id(id: &str) -> Option<Kind> {
    let (hash, ext) = id.split_once('.')?;
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match ext {
        "png" => Some(Kind::Image(ImageFormat::Png)),
        "jpg" => Some(Kind::Image(ImageFormat::Jpeg)),
        "webp" => Some(Kind::Image(ImageFormat::Webp)),
        "gif" => Some(Kind::Image(ImageFormat::Gif)),
        "svg" => Some(Kind::Image(ImageFormat::Svg)),
        "mp3" | "wav" | "m4a" | "flac" | "ogg" => Some(Kind::Sound),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_content_gives_same_id() {
        let dir = tempfile::tempdir().unwrap();
        let store = AssetStore::open(dir.path().to_path_buf()).unwrap();
        let a = store.put(ImageFormat::Svg, b"<svg/>").unwrap();
        let b = store.put(ImageFormat::Svg, b"<svg/>").unwrap();
        assert_eq!(a, b);
        assert!(a.ends_with(".svg"));
        let image = store.get(&a).unwrap();
        assert_eq!(&*image.bytes, b"<svg/>");
        assert_eq!(image.format, ImageFormat::Svg);
    }

    #[test]
    fn prunes_only_old_unused_images() {
        let dir = tempfile::tempdir().unwrap();
        let store = AssetStore::open(dir.path().to_path_buf()).unwrap();
        let used = store.put(ImageFormat::Svg, b"<svg id=\"used\"/>").unwrap();
        let unused = store
            .put(ImageFormat::Svg, b"<svg id=\"unused\"/>")
            .unwrap();
        fs::write(dir.path().join("notes.txt"), "not an asset").unwrap();
        let keep = HashSet::from([used.as_str()]);

        assert_eq!(store.prune(&keep, Duration::from_secs(3600)).unwrap(), 0);
        assert_eq!(store.prune(&keep, Duration::ZERO).unwrap(), 1);
        assert!(store.get(&used).is_ok());
        assert!(store.get(&unused).is_err());
        assert!(dir.path().join("notes.txt").exists());
    }

    #[test]
    fn keeps_sounds_apart_from_images() {
        let dir = tempfile::tempdir().unwrap();
        let store = AssetStore::open(dir.path().to_path_buf()).unwrap();
        let sound = store.put_sound(SoundFormat::Wav, b"RIFF").unwrap();
        assert!(sound.ends_with(".wav"));
        assert!(store.sound_path(&sound).unwrap().exists());
        assert!(store.get(&sound).is_err(), "a sound is not an image");
        let image = store.put(ImageFormat::Svg, b"<svg/>").unwrap();
        assert!(store.sound_path(&image).is_err());
    }

    #[test]
    fn rejects_ids_outside_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = AssetStore::open(dir.path().to_path_buf()).unwrap();
        assert!(store.get("../config.json").is_err());
        assert!(store.get("abc.png").is_err());
    }
}
