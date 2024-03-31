#![warn(clippy::all, clippy::pedantic)]

use anyhow::Result;
use log::{debug, error, info};
use responsive_image_for_hugo::constants;
use std::time::Duration;

use env_logger::Env;

use chrono::prelude::*;
use responsive_image_for_hugo::error::AppError;
use responsive_image_for_hugo::image::{GeneratedImage, ImageInfo};
use responsive_image_for_hugo::options::Options;
use structopt::StructOpt;
use tempfile::Builder;

use indicatif::ProgressBar;

/// Used to produce responsive images for use with [Hugo](https://gohugo.io/templates/data-templates).
///
/// This program:
/// 1. Takes an image (or directory of images) as input
/// 2. Converts each input image to `.webp` (whilst preserving its orientation).
/// 3. Creates resized versions of each input image suitable for different screen sizes.
/// 4. Uploads all image versions to S3.
/// 5. Generates a [srcset](https://css-tricks.com/a-guide-to-the-responsive-images-syntax-in-html/#using-srcset) and [sizes](https://css-tricks.com/a-guide-to-the-responsive-images-syntax-in-html/#aa-using-srcset-w-sizes) attribute for each input image
/// 5. Creates a [Hugo data file](https://gohugo.io/templates/data-templates/) with JSON formatted data for each image.
/// 6. Outputs either a prefilled [shortcode](https://gohugo.io/content-management/shortcodes/) to copy and paste or a YAML formatted list of the data keys.
/// I go into detail on the reasons behind this program [in a blog post](https://blog.arranfrance.com/post/responsive-blog-images/)
fn main() -> Result<(), AppError> {
    // TODO: Do better logging
    env_logger::Builder::from_env(Env::new().filter_or("responsive_image_for_hugo_LOG", "info"))
        .init();

    // Generate a single timestamp to use for the whole program
    let now = Local::now();

    let options = Options::from_args();
    let temp_dir = Builder::new().prefix("rith").tempdir()?;
    let temp_dir_path = &temp_dir.path().to_path_buf();
    debug!("Temp directory path: {}", &temp_dir_path.to_string_lossy());

    let does_data_already_exist = responsive_image_for_hugo::is_hugo_data_template_name_collision(
        &options.name,
        &options.output,
    )?;

    if does_data_already_exist && !options.force_overwrite {
        error!("Key {0} already exists in data template and the --force flag is not set. Will not overwrite", &options.name);
        return Err(AppError::KeyAlreadyExists {});
    }

    debug!("Temp directory: {:?}", temp_dir_path);
    info!("Generating images at sizes {:?}", &options.sizes);
    let images = responsive_image_for_hugo::generate_images(
        &options.image_location,
        temp_dir_path,
        &options,
    )?;

    // TODO: Optimize images for web further using pio or pio-like techniques (see: https://github.com/siiptuo/pio)

    let mut images_with_s3_paths: Vec<ImageInfo> =
        Vec::with_capacity(images.iter().map(|i| i.generated_images.len() + 1).sum());

    if options.skip_upload {
        for image in &images {
            let (original_image, generated_images) =
                fake_upload_images(image, &options.s3_directory, now);
            images_with_s3_paths.push(
                image
                    .with_full_size_image(original_image)
                    .with_generated_images(generated_images),
            );
        }
    } else {
        info!("Uploading images");
        for image in &images {
            let (original_image, generated_images) = responsive_image_for_hugo::upload_images(
                image.clone(),
                &options.s3_directory,
                now,
            )?;
            images_with_s3_paths.push(
                image
                    .with_full_size_image(original_image)
                    .with_generated_images(generated_images),
            );
        }
    }

    let spinner = ProgressBar::new_spinner();
    spinner.enable_steady_tick(Duration::from_millis(100));
    info!("Generating data file");
    let data = responsive_image_for_hugo::generate_data(images_with_s3_paths, &options);
    debug!("Writing data");
    responsive_image_for_hugo::write_data_to_hugo_data_template(
        data,
        options.output.clone(),
        options.force_overwrite,
    )?;
    spinner.finish();

    // Print template
    if images.len() > 1 {
        for image in images {
            let name = image.get_hugo_data_key(&options);
            println!("-  {name}");
        }
    } else {
        println!(
            "Shortcode: \n \n{{{{< picture name=\"{0}\" caption=\"\" >}}}}\n",
            images.first().unwrap().get_hugo_data_key(&options)
        );
    }

    temp_dir.close()?;
    Ok(())
}

/// Calculates the paths the images would be uploaded to in S3 and returns the modified
fn fake_upload_images(
    image: &ImageInfo,
    s3_sub_directory: &Option<String>,
    now: DateTime<Local>,
) -> (GeneratedImage, Vec<GeneratedImage>) {
    let prefix = responsive_image_for_hugo::get_uploaded_prefix(s3_sub_directory, now);
    let s3_images = image
        .generated_images
        .iter()
        .map(|image| {
            let s3_path = responsive_image_for_hugo::get_file_s3_bucket_path(&image.path, &prefix);
            image.with_s3_path(Some([constants::WEB_PREFIX, &s3_path].join("")))
        })
        .collect::<Vec<GeneratedImage>>();

    let full_size_image = image.full_size_image.with_s3_path(Some(
        [
            constants::WEB_PREFIX,
            &responsive_image_for_hugo::get_file_s3_bucket_path(
                &image.full_size_image.path,
                &prefix,
            ),
        ]
        .join(""),
    ));

    (full_size_image, s3_images)
}
