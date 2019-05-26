#![warn(clippy::all, clippy::pedantic)]
#![macro_use]
extern crate env_logger;
#[macro_use]
extern crate log;

use env_logger::Env;
use responsive_image_to_hugo_template::command_line::Options;
use responsive_image_to_hugo_template::error::AppError;
use structopt::StructOpt;
use tempfile::tempdir;

fn main() -> Result<(), AppError> {
    env_logger::Builder::from_env(
        Env::new().filter_or("RESPOSIVE_IMAGE_TO_HUGO_TEMPLATE_LOG", "info"),
    )
    .init();

    let options = Options::from_args();
    let temp_dir = tempdir()?;

    info!("Unzipping images");
    let image_directory = responsive_image_to_hugo_template::unzip_images(
        &options.images_zip,
        &temp_dir.path().to_path_buf(),
    )?;

    // TODO: Upload Images

    info!("Generating data file");
    let data = responsive_image_to_hugo_template::generate_data(&options, &image_directory);
    info!("Writing data");
    responsive_image_to_hugo_template::write_data_to_hugo_data_template(data, options.output)?;
    Ok(())
}
