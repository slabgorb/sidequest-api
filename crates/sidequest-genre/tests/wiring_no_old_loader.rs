//! Enforcement test for the Layered Content Model's "no coexistence" invariant
//! (design doc line 399): when an axis migrates to the new resolver, the old
//! loader must be deleted in the same PR.
//!
//! For archetypes, the old loader was `fn resolve_archetype` in
//! `src/archetype_resolve.rs`. After Phase F of the Layered Content Model
//! plan, that file is gone and the function lives in `src/archetype/shim.rs`.
//! This test greps the workspace to ensure the old standalone module is not
//! re-introduced.

use std::process::Command;

#[test]
fn old_archetype_resolve_module_is_removed() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    // Grep from the crate root down to the nearest workspace root — that is
    // two levels up from this crate (crates/sidequest-genre → sidequest-api).
    let workspace_root = std::path::Path::new(manifest_dir)
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root should exist");

    let out = Command::new("git")
        .arg("grep")
        .arg("-l")
        .arg("archetype_resolve")
        .arg("--")
        .arg("crates")
        .current_dir(&workspace_root)
        .output()
        .expect("git grep must run");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let hits: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        // This test file itself mentions the string in comments — allow it.
        .filter(|l| !l.ends_with("tests/wiring_no_old_loader.rs"))
        .collect();

    assert!(
        hits.is_empty(),
        "`archetype_resolve` references still present in:\n{}",
        hits.join("\n")
    );
}
