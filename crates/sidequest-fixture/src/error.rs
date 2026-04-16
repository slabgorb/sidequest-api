use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    #[error("fixture file not found: {0}")]
    NotFound(PathBuf),

    #[error("fixture I/O error reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("YAML parse error in {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("character deserialization failed: {0}")]
    Character(#[from] serde_json::Error),

    #[error("unknown world '{world}' for genre '{genre}' — check genre pack worlds/")]
    UnknownWorld { genre: String, world: String },

    #[error(
        "unknown confrontation type '{confrontation_type}' for genre '{genre}' — \
         check rules.yaml confrontations list"
    )]
    UnknownConfrontationType {
        genre: String,
        confrontation_type: String,
    },

    #[error("persistence error: {0}")]
    Persistence(#[from] sidequest_game::persistence::PersistError),

    #[error("fixture error: {0}")]
    Other(String),
}
