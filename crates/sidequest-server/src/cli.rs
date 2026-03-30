//! Command-line argument definitions for the SideQuest server.

use std::path::{Path, PathBuf};

use clap::Parser;

/// Command-line arguments for the SideQuest server.
#[derive(Parser, Debug)]
#[command(name = "sidequest-server")]
pub struct Args {
    /// Port to bind the server to.
    #[arg(long, default_value = "8765")]
    port: u16,

    /// Path to the genre packs directory.
    #[arg(long)]
    genre_packs_path: PathBuf,

    /// Directory for save files. Defaults to ~/.sidequest/saves.
    #[arg(long)]
    save_dir: Option<PathBuf>,

    /// Disable TTS voice synthesis (narration text is still sent, just no audio).
    #[arg(long, default_value = "false")]
    no_tts: bool,
}

impl Args {
    /// The port to bind to.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Path to genre packs directory.
    pub fn genre_packs_path(&self) -> &Path {
        &self.genre_packs_path
    }

    /// Optional save directory.
    pub fn save_dir(&self) -> Option<&Path> {
        self.save_dir.as_deref()
    }

    /// Whether TTS is disabled.
    pub fn no_tts(&self) -> bool {
        self.no_tts
    }
}
