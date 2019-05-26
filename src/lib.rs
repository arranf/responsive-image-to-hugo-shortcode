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

use crate::command_line::Options;
use crate::constants::*;
use crate::data::Data;
use crate::error::AppError;
use crate::fallback_image::FallbackImage;
use crate::source::Source;

use chrono::prelude::*;
use regex::Regex;
use scraper::{Html, Selector};
use std::fs::{create_dir_all, read_dir, read_to_string, DirEntry, File};
use std::io::copy;
use std::path::PathBuf;
use zip::ZipArchive;

pub fn upload_images() -> Result<(), AppError> {
    // TODO
    Result::Ok(())
}

pub fn generate_data(options: &Options, image_directory: &PathBuf) -> Data {
    let html = get_html(&options.template).unwrap();
    let prefix = [
        BUCKET_NAME.to_owned(),
        get_prefix(&options.s3_directory, Local::now()),
    ]
    .join("");
    let fallback_image = get_fallback_image(&html, &prefix, image_directory);
    let sources = get_sources(&html, &prefix, image_directory);
    Data {
        name: options.name.clone(),
        fallback: fallback_image,
        sources: sources,
    }
}

pub fn write_data_to_hugo_data_template(
    data: Data,
    output_location: Option<PathBuf>,
) -> Result<(), AppError> {
    let output_location = output_location.unwrap_or_else(|| PathBuf::from("./data/images.json"));
    let mut existing_data: Vec<Data>;

    if output_location.exists() {
        existing_data = serde_json::from_str(&read_to_string(&output_location)?)?;

        // See if data already exists and should be updated
        match existing_data.iter().position(|a| &a.name == &data.name) {
            Some(index) => {
                existing_data.swap_remove(index);
                existing_data.push(data);
            }
            None => existing_data.push(data),
        }
    } else {
        existing_data = vec![data];
    }

    info!("Writing index to {}", &output_location.to_string_lossy());

    create_dir_all(&output_location.with_file_name(""))?;
    let serialized_data = serde_json::to_string(&existing_data)?;
    std::fs::write(&output_location, serialized_data)?;
    info!("Written index to {}", &output_location.to_string_lossy());
    Ok(())
}

pub fn unzip_images(zip_path: &PathBuf, temp_directory: &PathBuf) -> Result<PathBuf, AppError> {
    let file = File::open(&zip_path)?;
    let reader = std::io::BufReader::new(file);

    let mut zip = ZipArchive::new(reader)?;
    // TODO: Concurrency
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;

        let outpath = temp_directory.join(file.sanitized_name());

        if (&*file.name()).ends_with('/') {
            // Handle directories
            info!(
                "Directory {} extracted to \"{}\"",
                i,
                outpath.as_path().display()
            );
            create_dir_all(&outpath).unwrap();
        } else {
            // Handle files
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    create_dir_all(&p).unwrap();
                }
            }
            let mut outfile = File::create(&outpath)?;
            copy(&mut file, &mut outfile).unwrap();
            info!(
                "File {} extracted to \"{}\" ({} bytes)",
                i,
                outpath.as_path().display(),
                file.size()
            );
        }
    }

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

fn get_prefix(directory: &Option<String>, now: DateTime<Local>) -> String {
    let year = now.year();
    let month = MONTH_NAMES[now.month0() as usize];

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
    return srcset
        .split("w,")
        // Prefix
        .map(|img| [prefix, img.trim()].join(""))
        // Join again
        .fold("".to_owned(), |acc, value| match acc.as_ref() {
            "" => [acc, value].join(""),
            _ => [acc, "w,".to_owned(), value].join(""),
        });
}

///
fn get_sources(html: &Html, prefix: &str, image_directory: &PathBuf) -> Vec<Source> {
    let mut sources: Vec<Source> = Vec::new();

    let selector = Selector::parse("source").unwrap();
    let min_width_regex = Regex::new(r"max-width: (\d+)px").unwrap();
    for source in html.select(&selector) {
        let source = source.value();
        let media = source.attr("media").unwrap();

        let sizes = source.attr("sizes").unwrap();
        let min_width = min_width_regex.captures(media).unwrap()[1].to_owned();

        let srcset = prefix_source(source.attr("srcset").unwrap(), prefix);

        // Get filename of best quality file (last) and generate a SQIP
        let split: Vec<&str> = srcset.split("/").collect();
        let filename = split.last().unwrap().split(" ").last().unwrap();
        let path = image_directory.clone().push(filename);
        //TODO DO SQIP

        sources.push(Source::new(
            media.to_owned(),
            sizes.to_owned(),
            srcset,
            "".to_owned(),
        ));
    }
    sources
}

/// Create a [SQIP](https://www.afasterweb.com/2018/04/25/smooth-out-low-quality-placeholders-with-sqip/) to use as a placeholder for **only** the `img` tag
fn get_fallback_image(html: &Html, prefix: &str, image_directory: &PathBuf) -> FallbackImage {
    let img_selector = Selector::parse("img").unwrap();
    let img = html.select(&img_selector).nth(0).unwrap().value();
    let sizes = img.attr("sizes").unwrap();
    let srcset = prefix_source(img.attr("srcset").unwrap(), prefix);
    let src = prefix_source(img.attr("src").unwrap(), prefix);
    // TODO SQIP
    FallbackImage::new(src, sizes.to_owned(), srcset, "".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::read_dir;
    use std::path::PathBuf;
    use std::str::FromStr;
    use tempfile::tempdir;

    #[test]
    fn test_unzip_images_happy() {
        let dest_dir = tempdir().unwrap();

        // Nothing there to begin with
        let paths = read_dir(dest_dir.path()).unwrap();
        assert_eq!(0, paths.count());

        let directory = unzip_images(
            &PathBuf::from_str("./test/q8e2dqsin57gkjoe4msg.zip").unwrap(),
            &dest_dir.path().to_path_buf(),
        )
        .unwrap();
        // The top level folder should exist
        let paths = read_dir(dest_dir.path()).unwrap();
        assert_eq!(paths.count(), 1);

        // The inner files should have been unzipped
        let file_count = read_dir(directory).unwrap().count();
        assert_eq!(file_count, 18);

        dest_dir.close().unwrap();
    }

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
                "images/2019/Jun/test_prefix/robot_c_scale,w_480.png 480w,images/2019/Jun/test_prefix/robot_c_scale,w_589.png 589w,images/2019/Jun/test_prefix/robot_c_scale,w_700.png 700w,images/2019/Jun/test_prefix/robot_c_scale,w_823.png 823w,images/2019/Jun/test_prefix/robot_c_scale,w_868.png 868w,images/2019/Jun/test_prefix/robot_c_scale,w_962.png 962w,images/2019/Jun/test_prefix/robot_c_scale,w_1044.png 1044w,images/2019/Jun/test_prefix/robot_c_scale,w_1125.png 1125w,images/2019/Jun/test_prefix/robot_c_scale,w_1196.png 1196w,images/2019/Jun/test_prefix/robot_c_scale,w_1277.png 1277w,images/2019/Jun/test_prefix/robot_c_scale,w_1346.png 1346w,images/2019/Jun/test_prefix/robot_c_scale,w_1415.png 1415w,images/2019/Jun/test_prefix/robot_c_scale,w_1484.png 1484w,images/2019/Jun/test_prefix/robot_c_scale,w_1482.png 1482w,images/2019/Jun/test_prefix/robot_c_scale,w_1726.png 1726w,images/2019/Jun/test_prefix/robot_c_scale,w_1741.png 1741w,images/2019/Jun/test_prefix/robot_c_scale,w_1798.png 1798w,images/2019/Jun/test_prefix/robot_c_scale,w_1866.png 1866w,images/2019/Jun/test_prefix/robot_c_scale,w_2000.png 2000w".to_owned(),
                "".to_owned()
            )
        )
    }

}
