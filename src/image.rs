use crate::options::Options;
use crate::AppError;
use crate::Metrics;

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{create_dir_all, File};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use exif::{Exif, In, Tag};
use indicatif::ProgressBar;

use log::debug;
use log::error;
use log::info;
use rimage::config::{Codec, EncoderConfig};
use rimage::{
    config::ResizeConfig,
    image::{imageops, DynamicImage, GenericImageView},
};
use rimage::{Decoder, Encoder};

lazy_static::lazy_static! {
    // Match any filename with 3 or 4 digits ending in a w; and `legacy`
    static ref RE: regex::Regex = regex::Regex::new("^\\d{3}w$|^\\d{4}w$|^legacy$").unwrap();

    // Valid extensions for rimage to decode. Should **not** be used for encoding, we want to ouput narrower formats.
    // Extensions can't be trusted to match their encoding but filtering to known good ones is a sensible first pass.
    pub static ref EXTENSIONS: &'static [&'static str] = &[
        "avif",
        "ff", // farbfeld (https://github.com/mcritchlow/farbfeld).
        "bmp",
        "hdr",
        "png",
        "jpg",
        "jpeg",
        "jxl", // jpeg-XL
        "psd", // Photoshop
        "qoi", // https://github.com/phoboslab/qoi
        "webp",
    ];

    // See: https://www.iana.org/assignments/media-types/media-types.xhtml#image)
    pub static ref MIME_TABLE: HashMap<&'static str, &'static str> = {
        let mut hm = HashMap::new();
        hm.insert("avif", "image/avif");
        hm.insert("png", "image/png");
        hm.insert("jpg", "image/jpeg");
        hm.insert("jpeg", "image/jpeg");
        hm.insert("jxl", "image/jxl");
        hm.insert("webp", "image/webp");
        hm
    };
}

/// Process the image provided in the path.
/// Iterate through the sizes and create a scaled image for each
pub fn process_image(
    path: &Path,
    output_directory: &Path,
    options: &Options,
    m: &mut Metrics,
) -> Result<ImageInfo> {
    let exif = get_exif(path)
        .with_context(|| format!("Failed to get EXIF data for {}", &path.to_string_lossy()))?;

    let decoder = Decoder::from_path(path)
        .with_context(|| format!("Failed to create decoder for {}", &path.to_string_lossy()))?;
    let image = decoder
        .decode()
        .with_context(|| format!("Failed to decode {}", &path.to_string_lossy()))?;

    let (width, height) = image.dimensions();

    let image = match exif {
        // TODO: Change this to return a result that's not an error
        // TODO: find some way to mark which images had to be rotated so we can log those out... it's helpful for organising galleries.
        // TODO: Sort those together at the end?
        Some(exif) => fix_if_needed(image, exif),
        None => image,
    };

    let resizes = compute_resize_pairs(width, height, options.sizes.0.clone())
        .ok_or(AppError::ImageTooSmall)
        .with_context(|| {
            format!(
                "{} width is too small to generate images for",
                path.to_string_lossy()
            )
        })?;
    debug!("Reizes for {}: {:?}", &path.to_string_lossy(), &resizes);

    // The largest size is the legacy one
    let max = resizes
        .iter()
        .map(|i| i.width)
        .max()
        .expect("No need to resizes");

    // TODO: Make configurable
    let ext = ".webp";

    let original_file_path =
        create_destination_path(output_directory, path, options, "original", ext).with_context(
            || {
                format!(
                    "Failed to create compute new image path for {}",
                    path.to_string_lossy()
                )
            },
        )?;

    create_dir_all(original_file_path.with_file_name("")).with_context(|| {
        format!(
            "Failed to create directory for {}",
            original_file_path.with_file_name("").to_string_lossy()
        )
    })?;

    if !options.skip_resize {
        // TODO: Performance improvements by doing this with a bufwriter (???)
        let original = File::create(&original_file_path).with_context(|| {
            format!(
                "Failed to create new file at {}",
                &original_file_path.to_string_lossy()
            )
        })?;
        let encoder =
            Encoder::new(original, image.clone()).with_config(EncoderConfig::new(Codec::WebP));
        encoder.encode().with_context(|| {
            format!(
                "Failed to write new encoded version of {}",
                original_file_path.to_string_lossy()
            )
        })?;
    }

    let mut generated_images: Vec<GeneratedImage> = Vec::with_capacity(resizes.len());

    let progress_bar = ProgressBar::new(resizes.len() as u64).with_message("Resizing Images");
    // TODO: Concurrency
    for resize in &resizes {
        let generated_image = scale_and_save(path, output_directory, &image, resize, ext, options)?;
        generated_images.push(generated_image);
        progress_bar.inc(1);
    }
    progress_bar.finish_and_clear();
    m.count += 1;
    m.resized += resizes.len() as u32;

    Ok(ImageInfo::new(
        max,
        path.to_path_buf(),
        ext.to_owned(),
        resizes,
        generated_images,
        GeneratedImage::new(width, height, original_file_path),
    ))
}

fn get_exif(path: &Path) -> Result<Option<Exif>> {
    let file = std::fs::File::open(path)?;
    let mut bufreader = std::io::BufReader::new(&file);
    let exifreader = exif::Reader::new();
    let exif = exifreader.read_from_container(&mut bufreader);

    match exif {
        Ok(exif) => Ok(Some(exif)),
        Err(exif::Error::NotFound(_)) => Ok(None),
        Err(a) => anyhow::Result::Err(a.into()),
    }
}

// TODO: Manage transforms better using matrix transforms
// https://users.rust-lang.org/t/rotating-orienting-a-jpg-with-the-image-library/62276
#[derive(Debug)]
enum NoFixNeededReason {
    AleadyCorrect,
    NoOrientationTag,
    InvalidOrientationTagValue(Option<u32>),
}

pub fn fix_if_needed(image: DynamicImage, exif: Exif) -> DynamicImage {
    let exif_field = exif.get_field(Tag::Orientation, In::PRIMARY);

    if exif_field.is_none() {
        log_reason_for_no_orientation_fix(NoFixNeededReason::NoOrientationTag);
        return image;
    }

    let orientation = match exif_field.unwrap().value.get_uint(0) {
        Some(1) => Err(NoFixNeededReason::AleadyCorrect),
        Some(value @ 2..=8) => Ok(value),
        other => Err(NoFixNeededReason::InvalidOrientationTagValue(other)),
    };

    match orientation {
        Ok(orientation_tag) => fix_orientation(image, orientation_tag),
        Err(reason) => {
            log_reason_for_no_orientation_fix(reason);
            image
        }
    }
}

fn log_reason_for_no_orientation_fix(reason: NoFixNeededReason) {
    use NoFixNeededReason::*;

    match reason {
        AleadyCorrect | NoOrientationTag => debug!("{:?}", reason),
        InvalidOrientationTagValue(_) => error!("{:?}", reason),
    };
}

// Naive implementation until I figure out how to use transformation matrices with the image crate.
fn fix_orientation(mut image: DynamicImage, orientation: u32) -> DynamicImage {
    if orientation > 8 {
        return image;
    }

    if orientation >= 5 {
        image = image.rotate90();
        imageops::flip_horizontal_in_place(&mut image);
    }

    if orientation == 3 || orientation == 4 || orientation == 7 || orientation == 8 {
        imageops::rotate180_in_place(&mut image);
    }

    if orientation % 2 == 0 {
        imageops::flip_horizontal_in_place(&mut image);
    }

    image
}

///  Resize the image provided by path and save the resulting new image onto output_directory
pub fn scale_and_save(
    path: &Path,
    output_directory: &Path,
    image: &DynamicImage,
    resize: &Resize,
    ext: &str,
    options: &Options,
) -> Result<GeneratedImage> {
    // The new path from names, sizes and file ext
    let image_path = create_destination_path(
        output_directory,
        path,
        options,
        &[resize.width.to_string(), "w".to_owned()].join(""),
        ext,
    )?;

    let generated_image = GeneratedImage::new(resize.width, resize.height, image_path.clone());

    if !options.skip_resize {
        let file = File::create(image_path).expect("Failed to create file");
        let resize_config = ResizeConfig::new(rimage::config::ResizeType::Lanczos3)
            .with_width(resize.width.try_into().unwrap())
            .with_height(resize.height.try_into().unwrap());
        let encoder = Encoder::new(file, image.clone()).with_config(
            EncoderConfig::new(Codec::WebP)
                .with_resize(resize_config)
                .with_quality(85.0)?,
        );

        encoder.encode()?;
    }

    Ok(generated_image)
}

fn create_destination_path(
    output_directory: &Path,
    image_path: &Path,
    options: &Options,
    suffix: &str,
    ext: &str,
) -> Result<PathBuf> {
    let file_name = image_path
        .file_stem()
        .and_then(OsStr::to_str)
        .with_context(|| {
            format!(
                "Could not get file stem from {}",
                image_path.to_string_lossy()
            )
        })?;
    let img_path = path_from_array(&[
        &output_directory.to_str().expect("Unable to get path"),
        &options.name.replace(' ', "-"),
        &file_name.replace(' ', "-"),
        &([file_name, "-", suffix, ".", ext]
            .join("")
            .replace("..", ".")
            .replace(' ', "-")),
    ]);
    Ok(img_path)
}

#[derive(Debug, Clone)]
pub struct Resize {
    pub width: u32,
    pub height: u32,
}

impl Resize {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

/// Return an array of sizes of large and small images based on the provided max width
fn compute_resize_pairs(
    image_width: u32,
    image_height: u32,
    sizes: Vec<u32>,
) -> Option<Vec<Resize>> {
    // This function makes extensive use of casting.
    // The casting works here as we are always going from an "equivalent sized" unsigned integer to float. (u32 -> f64)
    // When we "downcast" we always do that with floats which have sensible mechanisms for downcasting.
    // See: https://stackoverflow.com/questions/72247741/how-to-convert-a-f64-to-a-f32

    let aspect = (f64::from(image_width) / f64::from(image_height)) as f32;
    let sizes_smaller_than_current_image: Vec<Resize> = sizes
        .into_iter()
        .filter_map(|resize_width| {
            if image_width < resize_width {
                return None;
            }

            let resize_height = (resize_width as f64 / aspect as f64) as u32;
            let resize = Resize::new(resize_width, resize_height);
            Some(resize)
        })
        .collect();

    if sizes_smaller_than_current_image.is_empty() {
        None
    } else {
        Some(sizes_smaller_than_current_image)
    }
}

#[derive(Debug, Clone)]
pub struct GeneratedImage {
    pub width: u32,
    pub height: u32,
    pub path: PathBuf,
    pub s3_path: Option<String>,
}

impl GeneratedImage {
    pub fn new(width: u32, height: u32, path: PathBuf) -> Self {
        Self {
            width,
            height,
            path,
            s3_path: None,
        }
    }

    pub fn with_s3_path(&self, s3_path: Option<String>) -> Self {
        Self {
            s3_path,
            ..self.clone()
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageInfo {
    /// The largest (non original) file generated
    pub max_width: u32,
    /// The path to the input file
    pub input_path: PathBuf,
    /// The extension of the generated files
    pub ext: String,
    /// The resized image widths and heights
    pub resizes: Vec<Resize>,
    // The resized (+any other post processing) images
    pub generated_images: Vec<GeneratedImage>,
    /// The image at full resolution converted to a lossless format
    pub full_size_image: GeneratedImage,
}

impl ImageInfo {
    fn new(
        max_width: u32,
        input_path: PathBuf,
        ext: String,
        resizes: Vec<Resize>,
        generated_images: Vec<GeneratedImage>,
        full_size_image: GeneratedImage,
    ) -> Self {
        Self {
            max_width,
            input_path,
            ext,
            resizes,
            generated_images,
            full_size_image,
        }
    }

    pub fn with_generated_images(&self, generated_images: Vec<GeneratedImage>) -> Self {
        Self {
            generated_images,
            ..self.clone()
        }
    }

    pub fn with_full_size_image(&self, full_size_image: GeneratedImage) -> Self {
        Self {
            full_size_image,
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

/// Digest or consume a path. Check extension for image type (jpg, png, tif or others specified). In addition,
/// Skips any filename matching `^\\d{3}w$|^\\d{4}w$|^legacy$`
/// If matching the above concerns, then process the iamge.
/// Moves on without error if there is no match.
pub fn digest_path(
    path: &Path,
    output_directory: &Path,
    options: &Options,
    m: &mut Metrics,
) -> Result<Option<ImageInfo>> {
    // File written to bail as early as possible to avoid more syscalls than needed

    let extension = match path.extension().and_then(OsStr::to_str) {
        Some(extension) => extension.to_lowercase(),
        None => {
            return Ok(None);
        }
    };

    if !(EXTENSIONS.contains(&extension.as_str())) {
        info!("Skipping {}. Extension not valid.", &path.to_string_lossy());
        return Ok(None);
    }

    let metadata = path
        .metadata()
        .with_context(|| format!("Failed to get metadata for {}", path.to_string_lossy()))?;
    let file_size_in_kbs = metadata.len();

    if file_size_in_kbs < 100 {
        info!("Skipping {}. File size to smmall.", &path.to_string_lossy());
        return Ok(None);
    }

    let file_name = path.file_stem().and_then(OsStr::to_str).unwrap();

    // Make sure were not converting a previously converted image. Matching the filename
    if RE.is_match(file_name) {
        info!(
            "Skipping {} because of filename pattern.",
            &path.to_string_lossy()
        );
        return Ok(None);
    }

    let image_info = process_image(path, output_directory, options, m)?;
    Ok(Some(image_info))
}

/// Creates a std::path::Path from an array of &str
#[inline]
fn path_from_array(array: &[&str]) -> PathBuf {
    let mut pb = PathBuf::new();
    for s in array {
        pb.push(s);
    }
    pb
}
