use egui::ColorImage;
use std::path::{Path, PathBuf};

/// First subdirectory of `root` whose file name contains `guid`
/// (a loose contains-match, mirroring the C# original).
pub fn find_save_dir(root: &Path, guid: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(root).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.is_dir() && entry.file_name().to_string_lossy().contains(guid) {
            return Some(path);
        }
    }
    None
}

/// First `*.webp` file in `dir`, sorted by name for determinism.
pub fn first_webp(dir: &Path) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("webp")))
        .collect();
    files.sort();
    files.into_iter().next()
}

/// Decode an image file (webp thumbnails) into an egui texture source.
pub fn decode_to_color_image(path: &Path) -> Result<ColorImage, String> {
    let img = image::open(path).map_err(|e| format!("failed to decode image: {e}"))?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
    Ok(ColorImage::from_rgba_unmultiplied(
        [w, h],
        rgba.as_raw().as_slice(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageEncoder, RgbaImage};
    use std::fs;
    use std::io::BufWriter;
    use std::path::{Path, PathBuf};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let p = std::env::temp_dir().join(format!(
                "reclaimhonour-test-{}-{name}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const GUID: &str = "11111111-1111-1111-1111-111111111111";

    #[test]
    fn finds_dir_whose_name_contains_guid() {
        let t = TempDir::new("contains");
        let hit = t.path().join(format!("story-{GUID}-meta"));
        fs::create_dir_all(&hit).unwrap();
        let miss = t.path().join("99999999-9999-9999-9999-999999999999");
        fs::create_dir_all(&miss).unwrap();

        let found = find_save_dir(t.path(), GUID);
        assert_eq!(found, Some(hit));
    }

    #[test]
    fn returns_none_when_no_dir_matches() {
        let t = TempDir::new("nomatch");
        assert_eq!(find_save_dir(t.path(), GUID), None);
    }

    #[test]
    fn returns_none_for_missing_root() {
        let t = TempDir::new("missingroot");
        let gone = t.path().join("nope");
        assert_eq!(find_save_dir(&gone, GUID), None);
    }

    #[test]
    fn finds_first_webp() {
        let t = TempDir::new("firstwebp");
        fs::write(t.path().join("a.txt"), b"x").unwrap();
        fs::write(t.path().join("z.webp"), b"x").unwrap();
        fs::write(t.path().join("a.webp"), b"x").unwrap();
        assert_eq!(first_webp(t.path()), Some(t.path().join("a.webp")));
    }

    #[test]
    fn no_webp_returns_none() {
        let t = TempDir::new("nowebp");
        fs::write(t.path().join("a.txt"), b"x").unwrap();
        assert_eq!(first_webp(t.path()), None);
    }

    #[test]
    fn decodes_a_real_webp() {
        let t = TempDir::new("decode");
        let webp_path = write_test_webp(t.path());
        let img = decode_to_color_image(&webp_path).expect("should decode");
        assert_eq!(img.size, [4, 2]);
        assert_eq!(img.pixels.len(), 8);
    }

    #[test]
    fn decode_rejects_non_image() {
        let t = TempDir::new("dode");
        let p = t.path().join("x.webp");
        fs::write(&p, b"not an image").unwrap();
        assert!(decode_to_color_image(&p).is_err());
    }

    fn write_test_webp(dir: &Path) -> PathBuf {
        let img = RgbaImage::from_raw(
            4,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255, 0, 0, 255, 255,
                0, 255, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255,
            ],
        )
        .unwrap();
        let p = dir.join("out.webp");
        let f = BufWriter::new(fs::File::create(&p).unwrap());
        image::codecs::webp::WebPEncoder::new_lossless(f)
            .write_image(
                img.as_raw(),
                img.width(),
                img.height(),
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        p
    }
}