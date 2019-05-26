#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub media: String,
    pub sizes: String,
    pub srcset: String,
    pub placeholder: String,
}

impl Source {
    pub fn new(media: String, sizes: String, srcset: String, placeholder: String) -> Self {
        Source {
            media: media,
            sizes: sizes,
            srcset: srcset,
            placeholder: placeholder,
        }
    }
}
