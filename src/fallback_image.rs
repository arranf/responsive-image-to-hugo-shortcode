#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct FallbackImage {
    pub src: String,
    pub sizes: String,
    pub srcset: String,
    pub placeholder: String,
}

impl FallbackImage {
    pub fn new(src: String, sizes: String, srcset: String, placeholder: String) -> Self {
        Self {
            src,
            sizes,
            srcset,
            placeholder,
        }
    }
}
