//! Story 40-1 RED: README.md contains a canonical, compile-tested recipe.
//!
//! Fails today because `crates/sidequest-test-support/README.md` does not
//! exist. Dev's GREEN phase writes a README with:
//!
//! 1. A ```rust doctest-compatible code block demonstrating all three APIs
//!    (`ClaudeLike`, `SpanCaptureLayer`, `MockClaudeClient`) in a single
//!    end-to-end wiring test.
//! 2. A `#![doc = include_str!("../README.md")]` attribute on `src/lib.rs`
//!    so `cargo test --doc -p sidequest-test-support` runs the example.
//!
//! Why not just run `cargo test --doc`? Because a missing README, or a
//! README without a `rust` fence, produces no doctests — `cargo test --doc`
//! passes vacuously. We need an active check that the recipe is present AND
//! exercises all three APIs, not just that no doctests failed.

use std::fs;
use std::path::PathBuf;

fn readme_path() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("README.md")
}

#[test]
fn readme_exists() {
    assert!(
        readme_path().exists(),
        "README.md must exist at {:?} — it is the canonical harness recipe per story 40-1 AC",
        readme_path()
    );
}

#[test]
fn readme_has_rust_code_block() {
    let content = fs::read_to_string(readme_path())
        .expect("README.md must be readable (see readme_exists)");
    assert!(
        content.contains("```rust"),
        "README must have a ```rust code fence — the recipe is worthless if it isn't a runnable example"
    );
}

#[test]
fn readme_example_covers_all_three_apis() {
    let content = fs::read_to_string(readme_path())
        .expect("README.md must be readable (see readme_exists)");
    // Extract the first ```rust ... ``` block and assert it references all
    // three APIs. A recipe that only shows SpanCaptureLayer, for example,
    // fails the onboarding purpose of 40-1.
    let rust_block_start = content
        .find("```rust")
        .expect("README must have a ```rust code fence");
    let after_fence = &content[rust_block_start + "```rust".len()..];
    let rust_block_end = after_fence
        .find("```")
        .expect("rust code fence must be closed");
    let example = &after_fence[..rust_block_end];

    assert!(
        example.contains("ClaudeLike"),
        "first rust example must reference ClaudeLike"
    );
    assert!(
        example.contains("SpanCaptureLayer"),
        "first rust example must reference SpanCaptureLayer"
    );
    assert!(
        example.contains("MockClaudeClient"),
        "first rust example must reference MockClaudeClient"
    );
}

#[test]
fn lib_rs_includes_readme_as_crate_doc() {
    // The doc-include pattern is what makes `cargo test --doc` actually run
    // the README's rust example. Without it, the example is prose, not a test.
    let lib_rs_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs");
    let lib_rs = fs::read_to_string(&lib_rs_path).expect("src/lib.rs must exist");
    assert!(
        lib_rs.contains("include_str!(\"../README.md\")")
            || lib_rs.contains("include_str!(\"./README.md\")")
            || lib_rs.contains("#![doc = include_str!"),
        "src/lib.rs must include README.md as crate-level documentation so `cargo test --doc` runs the recipe. Found: {}",
        lib_rs.lines().filter(|l| l.starts_with("#![doc") || l.contains("include_str!")).collect::<Vec<_>>().join("\n")
    );
}
