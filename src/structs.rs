use crate::options::Options;

use super::Resize;

use std::path::PathBuf;

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

pub trait Uploadable {
    fn with_s3_path(&self, s3_path: Option<String>) -> Self;
    fn path(&self) -> PathBuf;
}

// TOODO: Record exif
#[derive(Debug, Clone)]
pub struct ImageInfo {
    /// The largest (non original) file generated
    pub max_width: usize,
    /// The path to the input file
    pub input_path: PathBuf,
    /// The extension of the generated files
    pub ext: String,
    /// The resized image widths and heights
    pub resizes: Vec<Resize>,
    // The resized (+any other post processing) images
    pub generated_images: Vec<GeneratedImage>,
    /// The image at full resolution converted to a specified format
    pub full_size_reencoded_image: GeneratedImage,
    /// The untouched original image
    pub original_image: OriginalImage,
}

impl ImageInfo {
    pub(crate) fn new(
        max_width: usize,
        input_path: PathBuf,
        ext: String,
        resizes: Vec<Resize>,
        generated_images: Vec<GeneratedImage>,
        full_size_reencoded_image: GeneratedImage,
        original_image: OriginalImage,
    ) -> Self {
        Self {
            max_width,
            input_path,
            ext,
            resizes,
            generated_images,
            full_size_reencoded_image,
            original_image,
        }
    }

    pub fn with_generated_images(&self, generated_images: Vec<GeneratedImage>) -> Self {
        Self {
            generated_images,
            ..self.clone()
        }
    }

    pub fn with_full_size_reencoded_image(
        &self,
        full_size_reencoded_image: GeneratedImage,
    ) -> Self {
        Self {
            full_size_reencoded_image,
            ..self.clone()
        }
    }

    pub fn with_original_image(&self, original_image: OriginalImage) -> Self {
        Self {
            original_image,
            ..self.clone()
        }
    }

    pub fn get_hugo_data_key(&self, options: &Options) -> String {
        [
            &options.name,
            self.input_path.file_name().unwrap().to_str().unwrap(),
        ]
        .join("-")
    }
}
