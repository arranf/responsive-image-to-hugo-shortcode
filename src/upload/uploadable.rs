use std::path::PathBuf;

pub trait Uploadable {
    fn with_s3_path(&self, s3_path: Option<String>) -> Self;
    fn path(&self) -> PathBuf;
}
