use serde::{Deserialize, Serialize};
/// Genre-tier content — populated in Task B2.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenreContent {}
