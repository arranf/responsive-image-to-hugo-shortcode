#![warn(clippy::all)]

#[macro_use]
extern crate log;

#[macro_use]
extern crate serde_derive;

pub mod command_line;
mod constants;
mod data;
pub mod error;
mod fallback_image;
mod source;
mod sqip;

use crate::command_line::Options;
use crate::data::Data;
use crate::error::AppError;
use crate::fallback_image::FallbackImage;
use crate::source::Source;
use crate::sqip::*;

use chrono::prelude::*;
use indicatif::ProgressBar;
use s3::creds::Credentials;
use s3::bucket::Bucket;
use scraper::{Html, Selector};
use std::fs::{create_dir_all, metadata, read_dir, read_to_string, DirEntry, File};
use std::io::copy;
use std::io::Read;
use std::path::{PathBuf, Path};
use zip::ZipArchive;

/// Upload images from a directory to S3
pub fn upload_images(
    image_directory: &PathBuf,
    s3_sub_directory: &Option<String>,
    now: DateTime<Local>,
) -> Result<(), AppError> {
    let files = read_dir(image_directory)?
        .filter_map(|result| result.ok())
        .map(|entry| entry.path())
        .filter(|path| !path.is_dir())
        .collect::<Vec<PathBuf>>();

    let total_size: u64 = files
        .iter()
        .filter_map(|path| metadata(path).ok())
        .map(|a| a.len())
        .sum();

    let prefix = get_prefix(s3_sub_directory, now);

    let region = constants::REGION.parse()?;
    // Loads from environment variables
    let credentials = Credentials::default()?;
    let bucket = Bucket::new(constants::BUCKET_NAME, region, credentials)?;

    //TODO: Concurrency
    let progress_bar = ProgressBar::new(total_size);
    for path in files {
        let file_name = path.file_name().unwrap().to_str().unwrap();
        let s3_path = [&prefix, file_name].join("");
        let mut file_contents = std::fs::File::open(&path)?;
        let metadata = file_contents.metadata()?;
        let size: usize = metadata.len() as usize;
        let mut bytes: Vec<u8> = Vec::with_capacity(size);
        file_contents.read_to_end(&mut bytes)?;

        let parts: Vec<&str> = file_name.split('.').collect();
        let mime_type = match parts.last() {
            Some(v) => match *v {
                "png" => "image/png",
                "jpg" => "image/jpeg",
                _ => "text/plain",
            },
            None => "text/plain",
        };
        bucket.put_object_with_content_type_blocking(&s3_path, &bytes, mime_type)?;
        progress_bar.inc(size as u64);
    }
    progress_bar.finish_and_clear();
    Result::Ok(())
}

/// Creates the data to be written to file
pub fn generate_data(options: &Options, image_directory: &Path, now: DateTime<Local>) -> Data {
    let html = get_html(&options.template).unwrap();
    let prefix = [
        constants::WEB_PREFIX,
        &get_prefix(&options.s3_directory, now),
    ]
    .join("");
    let fallback_image = get_fallback_image(&html, &prefix, image_directory);
    let sources = get_sources(&html, &prefix, image_directory);
    Data {
        name: options.name.clone(),
        fallback: fallback_image,
        sources,
    }
}

/// Checks if the name key is already used in the hugo data template

pub fn is_hugo_data_template_name_collision(name: &String, output_location: &Option<PathBuf>) -> Result<bool, AppError> {
    let name = name.to_owned();
    let output_location = output_location.to_owned().unwrap_or_else(|| PathBuf::from("./data/images.json"));
    if output_location.exists() {
        let existing_data: Vec<Data> = serde_json::from_str(&read_to_string(&output_location)?)?;
        // See if data already exists 
        Ok(existing_data.iter().any(|a| a.name == name))
    } else {
        Ok(false)
    }
}

/// Writes data to the specified location as JSON
pub fn write_data_to_hugo_data_template(
    data: Data,
    output_location: Option<PathBuf>,
    should_overwrite: bool
) -> Result<(), AppError> {
    let output_location = output_location.unwrap_or_else(|| PathBuf::from("./data/images.json"));
    let mut existing_data: Vec<Data>;

    if output_location.exists() {
        existing_data = serde_json::from_str(&read_to_string(&output_location)?)?;

        // See if data already exists and should be updated
        match existing_data.iter().position(|a| a.name == data.name) {
            Some(index) => {
                if should_overwrite {
                    existing_data.swap_remove(index);
                    existing_data.push(data);
                } else {
                    return Err(AppError::KeyAlreadyExists {  });
                }
            }
            None => existing_data.push(data),
        }
    } else {
        existing_data = vec![data];
    }

    debug!("Writing index to {}", &output_location.to_string_lossy());

    create_dir_all(output_location.with_file_name(""))?;
    let serialized_data = serde_json::to_string(&existing_data)?;
    std::fs::write(&output_location, serialized_data)?;
    info!("Index written to {}", &output_location.to_string_lossy());
    Ok(())
}

/// Unzip images to a temporary directory
pub fn unzip_images(zip_path: &PathBuf, temp_directory: &PathBuf) -> Result<PathBuf, AppError> {
    let file = File::open(zip_path)?;
    let reader = std::io::BufReader::new(file);

    let mut zip = ZipArchive::new(reader)?;
    let progress_bar = ProgressBar::new(zip.len() as u64);
    // TODO: Concurrency
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;

        let outpath = temp_directory.join(file.enclosed_name().ok_or(AppError::UnzipPath {})?);

        if (*file.name()).ends_with('/') {
            // Handle directories
            debug!(
                "Directory {} extracted to \"{}\"",
                i,
                outpath.as_path().display()
            );
            create_dir_all(&outpath).unwrap();
        } else {
            // Handle files
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    create_dir_all(p).unwrap();
                }
            }
            let mut outfile = File::create(&outpath)?;
            copy(&mut file, &mut outfile).unwrap();
            debug!(
                "File {} extracted to \"{}\" ({} bytes)",
                i,
                outpath.as_path().display(),
                file.size()
            );
        }
        progress_bar.inc(1);
    }
    progress_bar.finish_and_clear();

    let paths = read_dir(temp_directory)?;
    let directories: Vec<DirEntry> = paths
        .filter_map(|d| d.ok())
        .filter(|d| d.file_type().is_ok() && d.file_type().unwrap().is_dir())
        .collect();
    let zip_inner_path = directories[0].path();
    let temp_directory = temp_directory.join(zip_inner_path);
    Ok(temp_directory)
}

fn get_html(src_path: &PathBuf) -> Result<Html, AppError> {
    let contents = read_to_string(src_path)?;
    Ok(Html::parse_fragment(&contents))
}

/// The prefix that should be appended to all filenames to match the AWS path
/// Example: images/12/25/christmas/
fn get_prefix(directory: &Option<String>, now: DateTime<Local>) -> String {
    let year = now.year();
    let month = constants::MONTH_NAMES[now.month0() as usize];

    match directory {
        Some(directory) => format!(
            "images/{0}/{1}/{2}/",
            year,
            month,
            directory.trim_end_matches('/').trim_start_matches('/')
        ),
        None => format!("images/{0}/{1}/", year, month),
    }
}

/// Ensures the srcset points to the correct CDN
fn prefix_source(srcset: &str, prefix: &str) -> String {
    srcset
        .split("w,")
        // Prefix
        .map(|img| [prefix, img.trim()].join(""))
        // Join again
        .fold("".to_owned(), |acc, value| match acc.as_ref() {
            "" => [acc, value].join(""),
            _ => [acc, "w,".to_owned(), value].join(""),
        })
}

///
fn get_sources(html: &Html, prefix: &str, image_directory: &Path) -> Vec<Source> {
    let mut sources: Vec<Source> = Vec::new();

    let selector = Selector::parse("source").unwrap();
    for source in html.select(&selector) {
        let source = source.value();
        let media = source.attr("media").unwrap();

        let sizes = source.attr("sizes").unwrap();
        let srcset = prefix_source(source.attr("srcset").unwrap(), prefix);

        // Get filename of best quality file (last) and generate a SQIP
        // Split robot_ar_1_1,c_fill,g_auto__c_scale,w_200.png 200w, robot_ar_1_1,c_fill,g_auto__c_scale,w_335.png 335w,
        let split: Vec<&str> = srcset.split('/').collect();
        // Split robot_ar_1_1,c_fill,g_auto__c_scale,w_335.png 335w,
        let filename = split.last().unwrap().split(' ').collect::<Vec<&str>>();
        // Take robot_ar_1_1,c_fill,g_auto__c_scale,w_335.png
        let filename = filename.first().unwrap();
        let mut path = image_directory.to_path_buf();
        path.push(filename);

        let svg_placeholder = 
        if cfg!(test) {
            String::new()
        } else {
            debug!("Making SVG placeholder");
            make_sqip(&path.to_string_lossy()).expect("Error getting SQIP")
        };

        sources.push(Source::new(
            media.to_owned(),
            sizes.to_owned(),
            srcset,
            svg_placeholder,
        ));
    }
    sources
}

/// Create a [SQIP](https://www.afasterweb.com/2018/04/25/smooth-out-low-quality-placeholders-with-sqip/) to use as a placeholder for **only** the `img` tag
fn get_fallback_image(html: &Html, prefix: &str, image_directory: &Path) -> FallbackImage {
    let img_selector = Selector::parse("img").unwrap();
    let img = html.select(&img_selector).next().unwrap().value();
    let sizes = img.attr("sizes").unwrap();
    let srcset = prefix_source(img.attr("srcset").unwrap(), prefix);
    let src = prefix_source(img.attr("src").unwrap(), prefix);

    // Generate SVG
    let filename = img.attr("src").unwrap();
    let mut path = image_directory.to_path_buf();
    path.push(filename);

    let svg_placeholder =
    if cfg!(test) {
        String::new()
    } else {
        debug!("Making SVG placeholder");
        make_sqip(&path.to_string_lossy()).expect("Error getting SQIP")
    };

    FallbackImage::new(src, sizes.to_owned(), srcset, svg_placeholder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::read_dir;
    use std::path::PathBuf;
    use std::str::FromStr;
    use tempfile::tempdir;

    const ZIP_FILE: &str = "./test/example_zip.zip";

    fn init() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    // TODO: make integration test
    #[test]
    fn test_unzip_images_happy() {
        let dest_dir = tempdir().unwrap();

        // Nothing there to begin with
        let paths = read_dir(dest_dir.path()).unwrap();
        assert_eq!(0, paths.count());

        let directory = unzip_images(
            &PathBuf::from_str(ZIP_FILE).unwrap(),
            &dest_dir.path().to_path_buf(),
        )
        .unwrap();
        // The top level folder should exist
        let paths = read_dir(dest_dir.path()).unwrap();
        assert_eq!(paths.count(), 1);

        // The inner files should have been unzipped
        let file_count = read_dir(directory).unwrap().count();
        assert_eq!(file_count, 52);

        dest_dir.close().unwrap();
    }

    // TODO: make integration test
    #[test]
    fn test_unzip_images_error() {
        let dest_dir = tempdir().unwrap();
        let zip_dir = PathBuf::from_str("/tmp/bad/path").unwrap();
        let directory = unzip_images(&zip_dir, &dest_dir.path().to_path_buf());
        assert!(directory.is_err());
        dest_dir.close().unwrap();
    }

    #[test]
    fn test_get_html_error() {
        let src_path = PathBuf::from_str("/tmp/bad/path_again").unwrap();
        let result = get_html(&src_path);
        assert!(result.is_err());
    }

    // TODO: remove external dependency
    #[test]
    fn test_get_html_happy_path() {
        let src_path = PathBuf::from_str("./test/example_input.txt").unwrap();
        let result = get_html(&src_path);
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_get_prefix_with_directory() {
        // This directory path should have a / appended to it and a / stripped from the front
        let directory = "/tmp/something/arran".to_owned();
        let now: DateTime<Local> = Local
            .ymd(2012, 4, 6)
            .and_time(NaiveTime::from_hms(12, 12, 12))
            .unwrap();
        assert_eq!(
            "images/2012/Apr/tmp/something/arran/",
            get_prefix(&Some(directory), now)
        );
    }

    #[test]
    fn test_get_prefix_without_directory() {
        let now: DateTime<Local> = Local
            .ymd(2005, 5, 19)
            .and_time(NaiveTime::from_hms(12, 12, 12))
            .unwrap();
        assert_eq!("images/2005/May/", get_prefix(&None, now));
    }

    #[test]
    fn test_prefix_source() {
        let expected = "images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_200.png 200w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_335.png 335w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_448.png 448w";
        let srcset = "robot_ar_1_1,c_fill,g_auto__c_scale,w_200.png 200w,robot_ar_1_1,c_fill,g_auto__c_scale,w_335.png 335w,robot_ar_1_1,c_fill,g_auto__c_scale,w_448.png 448w";
        let result = prefix_source(srcset, "images/2019/Jun/test_prefix/");
        assert_eq!(expected, result);
    }

    #[test]
    fn test_get_sources() {
        init();
        let src_path = PathBuf::from_str("./test/example_input.txt").unwrap();
        let html = get_html(&src_path).unwrap();
        let result = get_sources(
            &html,
            "images/2019/Jun/test_prefix/",
            &PathBuf::from("/tmp/directory/"),
        );
        assert_eq!(result.len(), 3);
        assert_eq!(
            result[0],
            Source::new(
                "(max-width: 767px)".to_owned(),
                "(max-width: 1534px) 100vw, 1534px".to_owned(),
                r#"images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_200.png 200w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_335.png 335w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_448.png 448w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_551.png 551w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_641.png 641w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_725.png 725w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_811.png 811w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_886.png 886w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_961.png 961w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_1034.png 1034w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_1092.png 1092w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_1169.png 1169w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_1235.png 1235w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_1293.png 1293w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_1356.png 1356w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_1419.png 1419w,images/2019/Jun/test_prefix/robot_ar_1_1,c_fill,g_auto__c_scale,w_1534.png 1534w"#
                .to_owned(),
                "".to_owned()
            )
        );
    }

    #[test]
    fn test_get_fallback_image() {
        init();
        let src_path = PathBuf::from_str("./test/example_input.txt").unwrap();
        let html = get_html(&src_path).unwrap();
        let result = get_fallback_image(
            &html,
            "images/2019/Jun/test_prefix/",
            &PathBuf::from("/tmp/directory/"),
        );
        assert_eq!(
            result,
            FallbackImage::new(
                "images/2019/Jun/test_prefix/robot_c_scale,w_2000.png".to_owned(),
                "(max-width: 5000px) 40vw, 2000px".to_owned(),
                "images/2019/Jun/test_prefix/robot_c_scale,w_480.png 480w,images/2019/Jun/test_prefix/robot_c_scale,w_587.png 587w,images/2019/Jun/test_prefix/robot_c_scale,w_695.png 695w,images/2019/Jun/test_prefix/robot_c_scale,w_789.png 789w,images/2019/Jun/test_prefix/robot_c_scale,w_883.png 883w,images/2019/Jun/test_prefix/robot_c_scale,w_962.png 962w,images/2019/Jun/test_prefix/robot_c_scale,w_1044.png 1044w,images/2019/Jun/test_prefix/robot_c_scale,w_1125.png 1125w,images/2019/Jun/test_prefix/robot_c_scale,w_1196.png 1196w,images/2019/Jun/test_prefix/robot_c_scale,w_1277.png 1277w,images/2019/Jun/test_prefix/robot_c_scale,w_1346.png 1346w,images/2019/Jun/test_prefix/robot_c_scale,w_1415.png 1415w,images/2019/Jun/test_prefix/robot_c_scale,w_1484.png 1484w,images/2019/Jun/test_prefix/robot_c_scale,w_1482.png 1482w,images/2019/Jun/test_prefix/robot_c_scale,w_1726.png 1726w,images/2019/Jun/test_prefix/robot_c_scale,w_1741.png 1741w,images/2019/Jun/test_prefix/robot_c_scale,w_1798.png 1798w,images/2019/Jun/test_prefix/robot_c_scale,w_1866.png 1866w,images/2019/Jun/test_prefix/robot_c_scale,w_2000.png 2000w".to_owned(),
                "".to_owned()
            )
        )
    }
}
