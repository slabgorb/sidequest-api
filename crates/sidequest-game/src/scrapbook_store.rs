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

    // -----------------------------------------------------------------
    // Rework RED (round-trip 1, Reviewer finding): path-traversal guards.
    //
    // `genre`, `world`, `player` are path components that flow from
    // session context (player_name_for_save, genre_slug, world_slug).
    // Although these are internally generated in current code, they key
    // the save DB subtree and must be hardened against `..`, embedded
    // separators, and empty values — treating persisted/composable path
    // inputs as untrusted is the correct default.
    //
    // The fix should reject any segment equal to ".." or ".", containing
    // "/" or "\\", or empty. Canonicalization after construction followed
    // by `dest.starts_with(save_dir.join("scrapbook"))` is also an
    // acceptable implementation — the tests below only assert that the
    // *behavior* rejects the malicious inputs, not how.
    // -----------------------------------------------------------------

    #[test]
    fn persist_rejects_parent_dir_in_player_segment() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("render_a.png");
        fs::write(&src, b"X").unwrap();
        let err = persist_scrapbook_image(tmp.path(), "g", "w", "..", &src).expect_err(
            "player segment equal to `..` MUST be rejected — otherwise the \
             destination escapes save_dir/scrapbook/g/w/",
        );
        assert_eq!(
            err.kind(),
            io::ErrorKind::InvalidInput,
            "rejection should surface as InvalidInput (bad argument), not NotFound or Other"
        );
    }

    #[test]
    fn persist_rejects_parent_dir_in_genre_segment() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("render_b.png");
        fs::write(&src, b"X").unwrap();
        let err = persist_scrapbook_image(tmp.path(), "..", "w", "p", &src)
            .expect_err("genre segment `..` MUST be rejected");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn persist_rejects_embedded_separator_in_world_segment() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("render_c.png");
        fs::write(&src, b"X").unwrap();
        // A world slug containing '/' would silently create an extra nested
        // directory — not catastrophic like `..`, but still a silent data
        // layout violation that makes the save tree unpredictable.
        let err = persist_scrapbook_image(tmp.path(), "g", "a/b", "p", &src).expect_err(
            "world segment containing `/` MUST be rejected — it silently creates \
             nested directories that violate the documented tree layout",
        );
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn persist_rejects_empty_segments() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("render_d.png");
        fs::write(&src, b"X").unwrap();
        let err = persist_scrapbook_image(tmp.path(), "", "w", "p", &src).expect_err(
            "empty genre segment MUST be rejected — silently writing to \
             save_dir/scrapbook//w/p/... yields a broken path shape",
        );
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn persist_rejects_single_dot_segments() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("render_e.png");
        fs::write(&src, b"X").unwrap();
        // `.` is harmless in most path resolvers but is a canonical-form
        // violation: `save_dir/scrapbook/./w/p/foo.png` renders identical to
        // `save_dir/scrapbook/w/p/foo.png` after canonicalization. Reject to
        // keep the on-disk layout stable and predictable.
        let err = persist_scrapbook_image(tmp.path(), ".", "w", "p", &src)
            .expect_err("`.` segment MUST be rejected to keep the save layout canonical");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn persist_accepts_normal_slug_shaped_segments() {
        // Regression guard: the traversal rejections above must not reject
        // anything legitimate. `low_fantasy`, `ironhold`, `Rux` — ASCII
        // letters, digits, underscore, hyphen, and mixed case should all
        // pass through unchanged.
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("render_f.png");
        fs::write(&src, b"OK").unwrap();
        let dest = persist_scrapbook_image(tmp.path(), "low_fantasy_1", "iron-hold", "Rux", &src)
            .expect("normal slug-shaped segments must be accepted");
        assert!(dest.exists());
    }
}
