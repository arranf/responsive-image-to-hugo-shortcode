use std::path::PathBuf;

use crate::upload::uploadable::Uploadable;

/// The original file without any modifications
#[derive(Debug, Clone)]
pub struct OriginalImage {
    pub path: PathBuf,
    pub s3_path: Option<String>,
}

impl OriginalImage {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            s3_path: None,
        }
    }
}

impl Uploadable for OriginalImage {
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
