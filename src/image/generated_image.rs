use std::path::PathBuf;

use crate::upload::uploadable::Uploadable;

#[derive(Debug, Clone)]
pub struct GeneratedImage {
    pub width: usize,
    pub height: usize,
    pub path: PathBuf,
    pub s3_path: Option<String>,
}

impl GeneratedImage {
    pub fn new(width: usize, height: usize, path: PathBuf) -> Self {
        Self {
            width,
            height,
            path,
            s3_path: None,
        }
    }
}

impl Uploadable for GeneratedImage {
    fn path(&self) -> PathBuf {
        self.path.to_path_buf()
    }

    fn with_s3_path(&self, s3_path: Option<String>) -> Self {
        Self {
            s3_path,
            ..self.clone()
        }
    }
}
