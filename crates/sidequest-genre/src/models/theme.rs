//! UI theme colors and typography from `theme.yaml`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// UI theme colors and typography.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GenreTheme {
    /// Primary color hex.
    pub primary: String,
    /// Secondary color hex.
    pub secondary: String,
    /// Accent color hex.
    pub accent: String,
    /// Background color hex.
    pub background: String,
    /// Surface color hex.
    pub surface: String,
    /// Text color hex.
    pub text: String,
    /// Border style name.
    pub border_style: String,
    /// Web font family.
    pub web_font_family: String,
    /// Section break (dinkus) configuration.
    pub dinkus: Dinkus,
    /// Session opener configuration.
    pub session_opener: SessionOpener,
}

impl GenreTheme {
    /// Generate the base CSS for the client from theme.yaml fields.
    ///
    /// Produces `:root` CSS variables, body styles, and base component classes.
    /// Genre-specific `client_theme.css` overrides should be appended after this.
    pub fn generate_css(&self) -> String {
        let dinkus_glyph = self
            .dinkus
            .glyph
            .get("light")
            .map(|s| s.as_str())
            .unwrap_or("◇");

        let font = if self.web_font_family.contains(',') {
            self.web_font_family.clone()
        } else {
            format!(
                "'{}', Georgia, 'Times New Roman', serif",
                self.web_font_family
            )
        };

        format!(
            r#":root {{
  --primary: {primary};
  --secondary: {secondary};
  --accent: {accent};
  --background: {background};
  --surface: {surface};
  --text: {text};
  --dinkus-glyph: '{dinkus_glyph}';
}}

body {{
  background-color: var(--background);
  color: var(--text);
  font-family: {font};
  margin: 0 auto;
  padding: 16px;
  max-width: 720px;
  line-height: 1.6;
}}

.narration-block {{ margin-bottom: 1em; }}

.whisper {{
  border-left: 3px solid var(--accent);
  padding-left: 12px;
  font-style: italic;
  opacity: 0.85;
}}

.scene-image {{ max-width: 100%; border-radius: 4px; margin: 8px 0; }}

.drop-cap-block img {{ float: left; width: 80px; height: 80px; margin: 0 12px 4px 0; }}

.visually-hidden {{
  position: absolute;
  width: 1px; height: 1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
}}

.dinkus {{
  text-align: center;
  margin: 24px 0;
  opacity: 0.6;
  letter-spacing: 0.3em;
  font-size: 1.2em;
  user-select: none;
}}

.pull-quote {{
  text-align: center;
  font-size: 1.2em;
  font-style: italic;
  border-left: 4px solid var(--accent);
  margin: 1.5em auto;
  padding: 0.8em 1.2em;
  max-width: 85%;
}}

.session-opener {{
  font-size: 1.15em;
  border-bottom: 2px solid var(--accent);
  margin: 0 0 32px 0;
  padding: 16px 0 24px 0;
}}
"#,
            primary = self.primary,
            secondary = self.secondary,
            accent = self.accent,
            background = self.background,
            surface = self.surface,
            text = self.text,
            dinkus_glyph = dinkus_glyph,
            font = font,
        )
    }
}

/// Section break (dinkus) glyphs.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Dinkus {
    /// Whether dinkus is enabled.
    pub enabled: bool,
    /// Minimum paragraphs between dinkus.
    pub cooldown: u32,
    /// Default weight level.
    pub default_weight: String,
    /// Glyph strings keyed by weight (light, medium, heavy).
    pub glyph: HashMap<String, String>,
}

/// Session opener configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionOpener {
    /// Whether session openers are enabled.
    pub enabled: bool,
}
