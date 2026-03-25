//! Shared utility functions.

/// Convert a name to a URL-safe slug for lookup (lowercase, spaces → hyphens).
pub(crate) fn slugify(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}
