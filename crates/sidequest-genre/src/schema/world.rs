use serde::{Deserialize, Serialize};
/// World-tier content — populated in Task B3.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldContent {}
