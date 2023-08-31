use crate::fallback_image::FallbackImage;
use crate::source::Source;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Data {
    pub name: String,
    pub fallback: FallbackImage,
    pub sources: Vec<Source>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)] 
    pub hqimage: Option<String>
}
