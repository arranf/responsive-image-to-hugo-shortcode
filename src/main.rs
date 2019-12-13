#![warn(clippy::all, clippy::pedantic)]
#![macro_use]
extern crate env_logger;
#[macro_use]
extern crate log;

use env_logger::Env;

use chrono::prelude::*;
use responsive_image_to_hugo_template::command_line::Options;
use responsive_image_to_hugo_template::error::AppError;
use structopt::StructOpt;
use tempfile::Builder;

use indicatif::ProgressBar;

///
/// This program is used to produce a Hugo shortcode from the input of a .zip file and HTML from [responsivebreakpints.com](https://responsivebreakpoints.com)
/// It does this in three steps:
/// 1. Writing to the [Hugo images data template](https://gohugo.io/templates/data-templates/)
/// 2. Providing a shortcode that can be copy-pasted with values autofilled
/// 3. Uploading images in a .zip file to S3
///
/// I go into detail on the reasons behind this program [in a blog post](https://blog.arranfrance.com/post/responsive-blog-images/)
fn main() -> Result<(), AppError> {
    env_logger::Builder::from_env(
        Env::new().filter_or("RESPOSIVE_IMAGE_TO_HUGO_TEMPLATE_LOG", "info"),
    )
    .init();

    let options = Options::from_args();
    let temp_dir = Builder::new().prefix("rith").tempdir()?;
    let temp_dir_path = &temp_dir.path().to_path_buf();

    info!("Unzipping images");
    let image_directory =
        responsive_image_to_hugo_template::unzip_images(&options.images_zip, &temp_dir_path)?;

    // Generate a single timestamp to use for the whole program
    let now = Local::now();

    if !&options.skip_upload {
        info!("Uploading images");
        responsive_image_to_hugo_template::upload_images(
            &image_directory,
            &options.s3_directory,
            now,
        )?;
    }

    let spinner = ProgressBar::new_spinner();
    spinner.enable_steady_tick(100);
    info!("Generating data file");
    let data = responsive_image_to_hugo_template::generate_data(&options, &image_directory, now);
    debug!("Writing data");
    responsive_image_to_hugo_template::write_data_to_hugo_data_template(data, options.output)?;
    spinner.finish();

    // Print template
    println!(
        "Shortcode: \n \n{{{{< picture name=\"{0}\" caption=\"\" >}}}}\n",
        &options.name
    );
    temp_dir.close()?;
    Ok(())
}
