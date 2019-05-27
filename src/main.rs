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

fn main() -> Result<(), AppError> {
    env_logger::Builder::from_env(
        Env::new().filter_or("RESPOSIVE_IMAGE_TO_HUGO_TEMPLATE_LOG", "info"),
    )
    .init();

    let options = Options::from_args();
    let temp_dir = Builder::new().prefix("rith").tempdir()?;;

    info!("Unzipping images");
    let image_directory = responsive_image_to_hugo_template::unzip_images(
        &options.images_zip,
        &temp_dir.path().to_path_buf(),
    )?;

    let now = Local::now();

    if !&options.skip_upload {
        info!("Uploading images");
        responsive_image_to_hugo_template::upload_images(
            &image_directory,
            &options.s3_directory,
            now,
        )?;
    }

    debug!("Generating data file");
    let data = responsive_image_to_hugo_template::generate_data(&options, &image_directory, now);
    debug!("Writing data");
    responsive_image_to_hugo_template::write_data_to_hugo_data_template(data, options.output)?;

    temp_dir.close()?;
    Ok(())
}
