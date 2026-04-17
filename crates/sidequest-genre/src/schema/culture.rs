use serde::{Deserialize, Serialize};
/// Culture-tier content — populated in Task B4. Placeholder id default empty.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CultureContent {}
