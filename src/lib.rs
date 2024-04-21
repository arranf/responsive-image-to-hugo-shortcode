#![warn(clippy::all)]

pub mod constants;
pub mod error;
mod fallback_image;
mod hugo;
pub mod image;
pub mod metrics;
pub mod options;
mod source;
mod sqip;
pub mod structs;

use crate::error::AppError;
use crate::fallback_image::FallbackImage;
use crate::hugo::HugoData;
use crate::image::*;
use crate::metrics::Metrics;
use crate::options::Options;
use crate::sqip::*;
use itertools::Itertools;

use anyhow::{Context, Result};
use chrono::prelude::*;
use indicatif::ProgressBar;
use log::{debug, info, warn};
use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::Region;
use structs::{GeneratedImage, ImageInfo, Uploadable};

use std::fs::{create_dir_all, metadata, read_to_string};
use std::io::Read;
use std::iter::once;
use std::path::{Path, PathBuf};

/// Upload images from a directory to S3
pub fn upload_images(
    image: ImageInfo,
    s3_sub_directory: &Option<String>,
    now: DateTime<Local>,
) -> Result<ImageInfo> {
    // TODO: Do this in a way that handles errors and is not potentially misleading
    let total_size: u64 = once(&image.full_size_reencoded_image)
        .chain(&image.generated_images)
        .filter_map(|image| metadata(image.path()).ok())
        .map(|a| a.len())
        .sum::<u64>()
        // Figure out how to make this less ugly just because you can't chain it
        + metadata(image.original_image.path()).ok().unwrap().len();

    let prefix = get_uploaded_prefix(s3_sub_directory, now);

    let region = constants::REGION
        .parse::<Region>()
        .map_err(AppError::RegionParse)?;

    // Loads from environment variables
    let credentials = Credentials::default()?;
    let bucket = Bucket::new(constants::BUCKET_NAME, region, credentials)?.with_path_style();

    // TODO: Concurrency
    let progress_bar = ProgressBar::new(total_size);
    let mut s3_images: Vec<GeneratedImage> = Vec::with_capacity(image.generated_images.len() + 1);
    let full_size_reencoded_image = upload_image(
        &image.full_size_reencoded_image,
        &prefix,
        &bucket,
        &progress_bar,
        None,
    )?;
    let original_image = upload_image(
        &image.original_image,
        &prefix,
        &bucket,
        &progress_bar,
        Some("copy-of-original".to_owned()),
    )?;
    for image in &image.generated_images {
        let s3_image = upload_image(image, &prefix, &bucket, &progress_bar, None)?;
        s3_images.push(s3_image)
    }
    progress_bar.finish_and_clear();
    Ok(image
        .with_full_size_reencoded_image(full_size_reencoded_image)
        .with_original_image(original_image)
        .with_generated_images(s3_images))
}

fn upload_image<T: Uploadable>(
    image: &T,
    prefix: &String,
    bucket: &Bucket,
    progress_bar: &ProgressBar,
    suffix: Option<String>,
) -> Result<T> {
    let s3_path = get_file_s3_bucket_path(&image.path(), prefix, suffix);
    let mut file_contents = std::fs::File::open(image.path())?;
    let size = file_contents.metadata()?.len();
    let mut bytes: Vec<u8> = Vec::with_capacity(size.try_into().with_context(|| {
        format!(
            "Error creating a buffer to read {} into",
            &image.path().to_string_lossy()
        )
    })?);
    file_contents.read_to_end(&mut bytes)?;
    let extension = image
        .path()
        .extension()
        .with_context(|| {
            format!(
                "Error getting extension for {}",
                &image.path().to_string_lossy()
            )
        })?
        .to_str()
        .with_context(|| {
            format!(
                "Error getting extension for {}",
                &image.path().to_string_lossy()
            )
        })?
        .to_lowercase();
    let mime_type = &MIME_TABLE.get(&extension as &str).with_context(|| {
        format!(
            "Failed to get a matching mimetype when processing {}. {} does not map to a mime-type.",
            &image.path().to_string_lossy(),
            extension
        )
    })?;
    bucket.put_object_with_content_type_blocking(&s3_path, &bytes, mime_type)?;
    progress_bar.inc(size);
    Ok(image.with_s3_path(Some([constants::WEB_PREFIX, &s3_path].join(""))))
}

// This is only public so main can use it. See: See: https://users.rust-lang.org/t/lib-rs-declare-module-publicly-visible-only-to-main-rs/97368
#[doc(hidden)]
/// Gets the path from a bucket's root to a file
pub fn get_file_s3_bucket_path(path: &Path, prefix: &String, suffix: Option<String>) -> String {
    let file_name = path
        .file_name()
        .with_context(|| format!("Failed to get file name for {}", &path.to_string_lossy()))
        .unwrap()
        .to_str()
        .unwrap()
        .replace(' ', "-");

    [
        prefix.to_owned(),
        file_name,
        suffix.unwrap_or_else(|| String::from("")),
    ]
    .join("")
}

/// Creates the data to be written to file
pub fn generate_data(s3_images: Vec<ImageInfo>, options: &Options) -> Vec<HugoData> {
    let mut data: Vec<HugoData> = Vec::with_capacity(s3_images.len());
    for image in s3_images {
        let image = image.clone();
        let srcset = image
            .clone()
            .generated_images
            .iter()
            .map(|i| format!("{} {}w", i.s3_path.as_ref().unwrap(), i.width))
            .intersperse(",".to_owned())
            .collect();

        let src_image = &image
            .generated_images
            .iter()
            .max_by_key(|x| x.width)
            .unwrap();

        let sizes = format!("(max-width: {0}px) 100vw, {0}px", image.max_width);

        let fallback = get_fallback_image(
            src_image.s3_path.as_ref().unwrap().clone(),
            srcset,
            sizes,
            &image.input_path,
        );

        data.push(HugoData {
            name: image.get_hugo_data_key(options),
            fallback,
            sources: vec![],
            hqimage: Some(
                image
                    .full_size_reencoded_image
                    .s3_path
                    .expect("High quality image missing S3 bucket path when generating data."),
            ),
            original_image: Some(
                image
                    .original_image
                    .s3_path
                    .expect("High quality image missing S3 bucket path when generating data."),
            ),
        });
    }
    data
}

/// Checks if the name key is already used in the hugo data template
pub fn is_hugo_data_template_name_collision(
    name: &String,
    output_location: &Option<PathBuf>,
) -> Result<bool, AppError> {
    let name = name.to_owned();
    let output_location = output_location
        .to_owned()
        .unwrap_or_else(|| PathBuf::from("./data/images.json"));
    if output_location.exists() {
        let existing_data: Vec<HugoData> =
            serde_json::from_str(&read_to_string(&output_location)?)?;
        // See if data already exists
        Ok(existing_data.iter().any(|a| a.name == name))
    } else {
        Ok(false)
    }
}

/// Writes data to the specified location as JSON
pub fn write_data_to_hugo_data_template(
    data: Vec<HugoData>,
    output_location: Option<PathBuf>,
    should_overwrite: bool,
) -> Result<(), AppError> {
    let mut existing_data: Vec<HugoData>;
    let output_location = output_location.unwrap_or_else(|| PathBuf::from("./data/images.json"));

    if output_location.exists() {
        existing_data = serde_json::from_str(&read_to_string(&output_location)?)?;

        for image in data {
            match existing_data.iter().position(|a| a.name == image.name) {
                Some(index) => {
                    if should_overwrite {
                        existing_data.swap_remove(index);
                        existing_data.push(image);
                    } else {
                        return Err(AppError::KeyAlreadyExists {});
                    }
                }
                None => existing_data.push(image),
            }
        }
        // See if data already exists and should be updated
    } else {
        existing_data = data;
    }

    debug!("Writing index to {}", &output_location.to_string_lossy());

    create_dir_all(output_location.with_file_name(""))?;
    let serialized_data = serde_json::to_string(&existing_data)?;
    std::fs::write(&output_location, serialized_data)?;
    info!("Index written to {}", &output_location.to_string_lossy());
    Ok(())
}

/// Given the path to a directory of images, or a single image, generate resized images which
pub fn generate_images(
    image_path: &PathBuf,
    output_directory: &Path,
    options: &Options,
) -> Result<Vec<ImageInfo>> {
    let mut m = Metrics::default();
    if !image_path.is_dir() {
        debug!("Processing {}", image_path.to_string_lossy());
        let image_info = vec![image::process_image(
            image_path,
            output_directory,
            options,
            &mut m,
        )?];
        debug!("Metrics {:?}", m);
        return Ok(image_info);
    }

    // An error here (permission denied) will bail the walk. Dont bail the walk. Instead continue back to the parent
    let directory = std::fs::read_dir(image_path)?;
    let directory = directory.collect::<Vec<_>>();
    let item_count = directory.len();

    let progress_bar = ProgressBar::new(item_count as u64);
    let mut image_infos = Vec::with_capacity(item_count);
    for entry in directory {
        m.traversed += 1;

        // TODO: Special case this for lib vs command line
        // We ignore errors accessing files or performing other IO as one file we can't access isn't an indication the input parameters are incorrect necessarily.
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                warn!("WARNING: Processing error {:?}", e);
                continue;
            }
        };

        let path = entry.path();

        // We don't recurse
        if path.is_dir() {
            continue;
        } else {
            match digest_path(&path, output_directory, options, &mut m)? {
                Some(image_info) => image_infos.push(image_info),
                None => m.skipped += 1,
            }
        }
        progress_bar.inc(1);
    }
    progress_bar.finish_and_clear();

    debug!("Metrics {:?}", m);
    Ok(image_infos)
}

// TODO: Make this _way_ less opinionated
// This is only public so main can use it. See: https://users.rust-lang.org/t/lib-rs-declare-module-publicly-visible-only-to-main-rs/97368
/// The prefix that should be prepended to all filenames to match the AWS path
/// Example: images/12/25/christmas/
#[doc(hidden)]
pub fn get_uploaded_prefix(s3_path_suffix: &Option<String>, now: DateTime<Local>) -> String {
    let year = now.year();
    let month = constants::MONTH_NAMES[now.month0() as usize];

    match s3_path_suffix {
        Some(directory) => format!(
            "images/{0}/{1}/{2}/",
            year,
            month,
            directory.trim_end_matches('/').trim_start_matches('/')
        ),
        None => format!("images/{0}/{1}/", year, month),
    }
}

/// Create a [SQIP](https://www.afasterweb.com/2018/04/25/smooth-out-low-quality-placeholders-with-sqip/) to use as a placeholder for **only** the `img` tag
fn get_fallback_image(
    src: String,
    srcset: String,
    sizes: String,
    image_path: &Path,
) -> FallbackImage {
    let svg_placeholder = if cfg!(test) {
        String::new()
    } else {
        debug!("Making SVG placeholder");
        make_sqip(&image_path.to_string_lossy()).expect("Error getting SQIP")
    };

    FallbackImage::new(src, sizes.to_owned(), srcset, svg_placeholder)
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use std::fs::read_dir;
//     use std::path::PathBuf;
//     use std::str::FromStr;
//     use tempfile::tempdir;

//     const ZIP_FILE: &str = "./test/example_zip.zip";

//     fn init() {
//         let _ = env_logger::builder().is_test(true).try_init();
//     }

//     // TODO: make integration test
//     #[test]
//     fn test_unzip_images_happy() {
//         let dest_dir = tempdir().unwrap();

//         // Nothing there to begin with
//         let paths = read_dir(dest_dir.path()).unwrap();
//         assert_eq!(0, paths.count());

//         let directory = unzip_images(
//             &PathBuf::from_str(ZIP_FILE).unwrap(),
//             &dest_dir.path().to_path_buf(),
//         )
//         .unwrap();
//         // The top level folder should exist
//         let paths = read_dir(dest_dir.path()).unwrap();
//         assert_eq!(paths.count(), 1);

//         // The inner files should have been unzipped
//         let file_count = read_dir(directory).unwrap().count();
//         assert_eq!(file_count, 52);

//         dest_dir.close().unwrap();
//     }

//     // TODO: make integration test
//     #[test]
//     fn test_unzip_images_error() {
//         let dest_dir = tempdir().unwrap();
//         let zip_dir = PathBuf::from_str("/tmp/bad/path").unwrap();
//         let directory = unzip_images(&zip_dir, &dest_dir.path().to_path_buf());
//         assert!(directory.is_err());
//         dest_dir.close().unwrap();
//     }

//     #[test]
//     fn test_get_html_error() {
//         let src_path = PathBuf::from_str("/tmp/bad/path_again").unwrap();
//         let result = get_html(&src_path);
//         assert!(result.is_err());
//     }

//     // TODO: remove external dependency
//     #[test]
//     fn test_get_html_happy_path() {
//         let src_path = PathBuf::from_str("./test/example_input.txt").unwrap();
//         let result = get_html(&src_path);
//         assert!(result.is_ok());
//         let result = result.unwrap();
//         assert!(result.errors.is_empty());
//     }

//     #[test]
//     fn test_get_prefix_with_directory() {
//         // This directory path should have a / appended to it and a / stripped from the front
//         let directory = "/tmp/something/arran".to_owned();
//         let now: DateTime<Local> = Local.with_ymd_and_hms(2012, 4, 6, 12, 12, 12).unwrap();
//         assert_eq!(
//             "images/2012/Apr/tmp/something/arran/",
//             get_prefix(&Some(directory), now)
//         );
//     }

//     #[test]
//     fn test_get_prefix_without_directory() {
//         let now: DateTime<Local> = Local.with_ymd_and_hms(2012, 4, 6, 12, 12, 12).unwrap();
//         assert_eq!("images/2005/May/", get_prefix(&None, now));
//     }

//     #[test]
//     fn test_prefix_source() {
//         let expected = "images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_200.png 200w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_335.png 335w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_448.png 448w";
//         let srcset = "robot_ar_1_1,c_fill,g_auto__c_scale,w_200.png 200w,robot_ar_1_1,c_fill,g_auto__c_scale,w_335.png 335w,robot_ar_1_1,c_fill,g_auto__c_scale,w_448.png 448w";
//         let result = prefix_source(srcset, "images/2019/Jun/test_prefix/");
//         assert_eq!(expected, result);
//     }

//     #[test]
//     fn test_get_sources() {
//         init();
//         let src_path = PathBuf::from_str("./test/example_input.txt").unwrap();
//         let html = get_html(&src_path).unwrap();
//         let result = get_sources(
//             &html,
//             "images/2019/Jun/test_prefix/",
//             &PathBuf::from("/tmp/directory/"),
//         );
//         assert_eq!(result.len(), 3);
//         assert_eq!(
//             result[0],
//             Source::new(
//                 "(max-width: 767px)".to_owned(),
//                 "(max-width: 1534px) 100vw, 1534px".to_owned(),
//                 r#"images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_200.png 200w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_335.png 335w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_448.png 448w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_551.png 551w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_641.png 641w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_725.png 725w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_811.png 811w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_886.png 886w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_961.png 961w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_1034.png 1034w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_1092.png 1092w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_1169.png 1169w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_1235.png 1235w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_1293.png 1293w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_1356.png 1356w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_1419.png 1419w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_1534.png 1534w"#
//                 .to_owned(),
//                 "".to_owned()
//             )
//         );
//     }

//     #[test]
//     fn test_get_fallback_image() {
//         init();
//         let src_path = PathBuf::from_str("./test/example_input.txt").unwrap();
//         let html = get_html(&src_path).unwrap();
//         let result = get_fallback_image(
//             &html,
//             "images/2019/Jun/test_prefix/",
//             &PathBuf::from("/tmp/directory/"),
//         );
//         assert_eq!(
//             result,
//             FallbackImage::new(
//                 "images/2019/Jun/test_prefix/robot_c_scale,w_2000.png".to_owned(),
//                 "(max-width: 5000px) 40vw, 2000px".to_owned(),
//                 "images/2019/Jun/test_prefix/robot_c_scale,w_480.png 480w,images/2019/Jun/test_prefix/robot_c_scale,w_587.png 587w,images/2019/Jun/test_prefix/robot_c_scale,w_695.png 695w,images/2019/Jun/test_prefix/robot_c_scale,w_789.png 789w,images/2019/Jun/test_prefix/robot_c_scale,w_883.png 883w,images/2019/Jun/test_prefix/robot_c_scale,w_962.png 962w,images/2019/Jun/test_prefix/robot_c_scale,w_1044.png 1044w,images/2019/Jun/test_prefix/robot_c_scale,w_1125.png 1125w,images/2019/Jun/test_prefix/robot_c_scale,w_1196.png 1196w,images/2019/Jun/test_prefix/robot_c_scale,w_1277.png 1277w,images/2019/Jun/test_prefix/robot_c_scale,w_1346.png 1346w,images/2019/Jun/test_prefix/robot_c_scale,w_1415.png 1415w,images/2019/Jun/test_prefix/robot_c_scale,w_1484.png 1484w,images/2019/Jun/test_prefix/robot_c_scale,w_1482.png 1482w,images/2019/Jun/test_prefix/robot_c_scale,w_1726.png 1726w,images/2019/Jun/test_prefix/robot_c_scale,w_1741.png 1741w,images/2019/Jun/test_prefix/robot_c_scale,w_1798.png 1798w,images/2019/Jun/test_prefix/robot_c_scale,w_1866.png 1866w,images/2019/Jun/test_prefix/robot_c_scale,w_2000.png 2000w".to_owned(),
//                 "".to_owned()
//             )
//         )
//     }
// }
