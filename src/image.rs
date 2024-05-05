use crate::options::Options;
use crate::structs::GeneratedImage;
use crate::structs::ImageInfo;
use crate::structs::OriginalImage;
use crate::AppError;
use crate::Metrics;

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{create_dir_all, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indicatif::ProgressBar;

use load_image::export::imgref::ImgVecKind;
use load_image::export::rgb::ComponentSlice;
use load_image::Loader;
use log::debug;

use log::info;

use rimage::codecs::jpegli::JpegliEncoder;
use rimage::codecs::jpegli::JpegliOptions;
use zune_core::colorspace::ColorSpace;
use zune_image::image::Image;
use zune_image::traits::EncoderTrait;
use zune_image::traits::OperationsTrait;

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

fn zune_image_from_loaded_image(image: load_image::Image) -> Image {
    // TODO: Metadata
    match image.into_imgvec() {
        ImgVecKind::RGB8(pixels) => {
            let (pixels, width, height) = pixels.into_contiguous_buf();
            zune_image::image::Image::from_u8(
                &pixels
                    .into_iter()
                    .flat_map(|px| px.as_slice().to_owned())
                    .collect::<Vec<u8>>(),
                width,
                height,
                ColorSpace::RGB,
            )
        }
        ImgVecKind::RGBA8(pixels) => {
            let (pixels, width, height) = pixels.into_contiguous_buf();
            zune_image::image::Image::from_u8(
                &pixels
                    .into_iter()
                    .flat_map(|px| px.as_slice().to_owned())
                    .collect::<Vec<u8>>(),
                width,
                height,
                ColorSpace::RGBA,
            )
        }
        ImgVecKind::RGB16(pixels) => {
            let (pixels, width, height) = pixels.into_contiguous_buf();
            zune_image::image::Image::from_u16(
                &pixels
                    .into_iter()
                    .flat_map(|px| px.as_slice().to_owned())
                    .collect::<Vec<u16>>(),
                width,
                height,
                ColorSpace::RGB,
            )
        }
        ImgVecKind::RGBA16(pixels) => {
            let (pixels, width, height) = pixels.into_contiguous_buf();
            zune_image::image::Image::from_u16(
                &pixels
                    .into_iter()
                    .flat_map(|px| px.as_slice().to_owned())
                    .collect::<Vec<u16>>(),
                width,
                height,
                ColorSpace::RGBA,
            )
        }
        ImgVecKind::GRAY8(pixels) => {
            let (pixels, width, height) = pixels.into_contiguous_buf();
            zune_image::image::Image::from_u8(
                &pixels
                    .into_iter()
                    .flat_map(|px| px.as_slice().to_owned())
                    .collect::<Vec<u8>>(),
                width,
                height,
                ColorSpace::Luma,
            )
        }
        ImgVecKind::GRAY16(pixels) => {
            let (pixels, width, height) = pixels.into_contiguous_buf();
            zune_image::image::Image::from_u16(
                &pixels
                    .into_iter()
                    .flat_map(|px| px.as_slice().to_owned())
                    .collect::<Vec<u16>>(),
                width,
                height,
                ColorSpace::Luma,
            )
        }
        ImgVecKind::GRAYA8(pixels) => {
            let (pixels, width, height) = pixels.into_contiguous_buf();
            zune_image::image::Image::from_u8(
                &pixels
                    .into_iter()
                    .flat_map(|px| px.as_slice().to_owned())
                    .collect::<Vec<u8>>(),
                width,
                height,
                ColorSpace::LumaA,
            )
        }
        ImgVecKind::GRAYA16(pixels) => {
            let (pixels, width, height) = pixels.into_contiguous_buf();
            zune_image::image::Image::from_u16(
                &pixels
                    .into_iter()
                    .flat_map(|px| px.as_slice().to_owned())
                    .collect::<Vec<u16>>(),
                width,
                height,
                ColorSpace::RGBA,
            )
        }
    }
}

/// Process the image provided in the path.
/// Iterate through the sizes and create a scaled image for each
pub fn process_image(
    input_file: &Path,
    output_directory: &Path,
    options: &Options,
    m: &mut Metrics,
) -> Result<ImageInfo> {
    // TODO: With context
    let image = Loader::new()
        .metadata(true)
        .load_path(input_file)
        .with_context(|| format!("Failed to load image {}", &input_file.to_string_lossy()))?;
    let width = image.width;
    let height = image.height;
    let image = zune_image_from_loaded_image(image);

    let resizes = compute_resize_pairs(width, height, options.sizes.0.clone())
        .ok_or(AppError::ImageTooSmall)
        .with_context(|| {
            format!(
                "{} width is too small to generate images for",
                input_file.to_string_lossy()
            )
        })?;
    debug!(
        "Reizes for {}: {:?}",
        &input_file.to_string_lossy(),
        &resizes
    );

    // The largest resized size
    let max = resizes
        .iter()
        .map(|i| i.width)
        .max()
        .expect("No need to resizes");

    // TODO: Make configurable
    let ext = ".jpeg";

    let full_size_reencoded_path =
        create_destination_path(output_directory, input_file, options, "original", ext)
            .with_context(|| {
                format!(
                    "Failed to create compute new image path for {}",
                    input_file.to_string_lossy()
                )
            })?;

    create_dir_all(full_size_reencoded_path.with_file_name("")).with_context(|| {
        format!(
            "Failed to create directory for {}",
            full_size_reencoded_path
                .with_file_name("")
                .to_string_lossy()
        )
    })?;

    if !options.skip_resize {
        encode_image(&image, full_size_reencoded_path.clone()).with_context(|| {
            format!(
                "Failed to reencode image at full size: {}",
                input_file.to_string_lossy()
            )
        })?;
    }

    let mut generated_images: Vec<GeneratedImage> = Vec::with_capacity(resizes.len());

    let progress_bar = ProgressBar::new(resizes.len() as u64).with_message("Resizing Images");
    // TODO: Concurrency
    for resize in &resizes {
        let generated_image =
            scale_and_save(input_file, output_directory, &image, resize, ext, options)
                .with_context(|| {
                    format!(
                        "Failed to resize image to {:?} {}",
                        resize,
                        input_file.to_string_lossy()
                    )
                })?;
        generated_images.push(generated_image);
        progress_bar.inc(1);
    }
    progress_bar.finish_and_clear();
    m.count += 1;
    m.resized += resizes.len();

    Ok(ImageInfo::new(
        max,
        input_file.to_path_buf(),
        ext.to_owned(),
        resizes,
        generated_images,
        GeneratedImage::new(width, height, full_size_reencoded_path.clone()),
        OriginalImage::new(input_file.to_path_buf()),
    ))
}

fn encode_image(image: &Image, path: PathBuf) -> Result<()> {
    let mut file = File::create_new(&path)?;
    let encoder_options = JpegliOptions {
        quality: 90.0,
        ..JpegliOptions::default()
    };
    let mut encoder = JpegliEncoder::new_with_options(encoder_options);
    let image = encoder.encode(image).with_context(|| {
        format!(
            "Failed to write new encoded version of {}",
            path.to_string_lossy()
        )
    })?;
    file.write_all(&image)?;
    Ok(())
}

///  Resize the image provided by path and save the resulting new image onto output_directory
pub fn scale_and_save(
    path: &Path,
    output_directory: &Path,
    image: &Image,
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
        let mut image = image.clone();
        rimage::operations::resize::Resize::new(
            resize.width,
            resize.height,
            rimage::operations::resize::ResizeAlg::Convolution(
                rimage::operations::resize::FilterType::Lanczos3,
            ),
        )
        .execute(&mut image)?;

        encode_image(&image, image_path.clone())?;
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
        (output_directory
            .to_str()
            .expect("Unable to create destination path path")),
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
    pub width: usize,
    pub height: usize,
}

impl Resize {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height }
    }
}

/// Return an array of sizes of large and small images based on the provided max width
fn compute_resize_pairs(
    image_width: usize,
    image_height: usize,
    sizes: Vec<usize>,
) -> Option<Vec<Resize>> {
    // This function makes extensive use of casting.
    // The casting works here as we are always going from an "equivalent sized" unsigned integer to float. (usize -> f64)
    // When we "downcast" we always do that with floats which have sensible mechanisms for downcasting.
    // See: https://stackoverflow.com/questions/72247741/how-to-convert-a-f64-to-a-f32

    let aspect = image_width as f64 / image_height as f64;
    let sizes_smaller_than_current_image: Vec<Resize> = sizes
        .into_iter()
        .filter_map(|resize_width| {
            if image_width < resize_width {
                return None;
            }

            let resize_height = (resize_width as f64 / aspect) as usize;
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
