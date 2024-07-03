use serde::{Deserialize, Serialize};

use super::fallback_image::FallbackImage;
use super::source::Source;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct HugoData {
    pub name: String,
    pub fallback: FallbackImage,
    pub sources: Vec<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub hqimage: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub original_image: Option<String>,
}
