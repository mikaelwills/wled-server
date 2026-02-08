use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GroupConfig {
    pub id: String,
    pub members: Vec<String>,
    #[serde(default)]
    pub universe: Option<u16>,
}
