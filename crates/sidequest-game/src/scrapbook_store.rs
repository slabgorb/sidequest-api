//! Save-scoped filesystem store for scrapbook images (story 37-28).
//!
//! The render daemon writes images into a global pool (`~/.sidequest/renders/`)
//! served via `/api/renders/{filename}`. That URL survives session resume in
//! the `scrapbook_entries` SQLite row, but the file it points to can be
//! cleaned, orphaned by cross-machine save transfer, or overwritten by a
//! later render — manifest survives, bytes don't.
//!
//! [`persist_scrapbook_image`] copies the rendered file into a save-scoped
//! subtree so the image is durable for as long as the save exists, adjacent
//! to the `.db` file on disk. The returned path is the new **disk** location;
//! the caller is responsible for deriving the corresponding `/api/scrapbook/`
//! URL for the payload and database row.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Copy a rendered scrapbook image from the global renders pool into a
/// save-scoped subtree rooted at
/// `{save_dir}/scrapbook/{genre}/{world}/{player}/{filename}`.
///
/// The `filename` is taken verbatim from `src_path`. The intermediate
/// directories are created if absent. If a file already exists at the
/// destination it is overwritten — scrapbook images are content-addressed
/// by the render pipeline, so a repeat copy is idempotent rather than a
/// collision.
///
/// Returns the absolute destination path on success.
///
/// # Errors
///
/// - `src_path` has no file-name component.
/// - `src_path` does not exist, is not readable, or is not a file.
/// - The destination directory cannot be created.
/// - The file copy fails.
///
/// No silent fallbacks (per CLAUDE.md): every failure propagates so the
/// call site can emit a loud OTEL event.
pub fn persist_scrapbook_image(
    save_dir: &Path,
    genre: &str,
    world: &str,
    player: &str,
    src_path: &Path,
) -> io::Result<PathBuf> {
    let filename = src_path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "persist_scrapbook_image: src_path has no file-name component: {}",
                src_path.display()
            ),
        )
    })?;

    let dest_dir = save_dir
        .join("scrapbook")
        .join(genre)
        .join(world)
        .join(player);
    fs::create_dir_all(&dest_dir)?;

    let dest = dest_dir.join(filename);
    fs::copy(src_path, &dest)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persist_copies_file_to_save_scoped_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let save_dir = tmp.path();

        // Simulate the global renders pool.
        let renders = tmp.path().join("renders");
        fs::create_dir_all(&renders).unwrap();
        let src = renders.join("render_abc.png");
        fs::write(&src, b"PNGBYTES").unwrap();

        let dest = persist_scrapbook_image(save_dir, "low_fantasy", "ironhold", "rux", &src)
            .expect("persist ok");

        let expected = save_dir
            .join("scrapbook")
            .join("low_fantasy")
            .join("ironhold")
            .join("rux")
            .join("render_abc.png");
        assert_eq!(dest, expected);
        assert!(dest.exists(), "destination file must exist");
        assert_eq!(fs::read(&dest).unwrap(), b"PNGBYTES");
    }

    #[test]
    fn persist_overwrites_existing_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("render_x.png");
        fs::write(&src, b"NEW").unwrap();

        // Pre-populate the destination with different bytes.
        let dest_dir = tmp.path().join("scrapbook").join("g").join("w").join("p");
        fs::create_dir_all(&dest_dir).unwrap();
        fs::write(dest_dir.join("render_x.png"), b"OLD").unwrap();

        let dest = persist_scrapbook_image(tmp.path(), "g", "w", "p", &src).expect("persist ok");
        assert_eq!(fs::read(&dest).unwrap(), b"NEW");
    }

    #[test]
    fn persist_errors_when_source_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nope.png");
        let err = persist_scrapbook_image(tmp.path(), "g", "w", "p", &missing)
            .expect_err("source does not exist");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }
}
