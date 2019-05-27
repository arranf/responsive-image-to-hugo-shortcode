use structopt::StructOpt;

use std::path::PathBuf;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "Responsive Image to Shortcode",
    about = "A tool to turn a responsive image .zip and HTML into a responsive image for a Hugo site"
)]
pub struct Options {
    /// The path to the .zip file containing images from https://responsivebreakpoints.com/
    #[structopt(parse(from_os_str))]
    pub images_zip: PathBuf,

    /// The name of the image. This uniquely identifies it in the Hugo data file
    #[structopt(short = "n", long = "name")]
    pub name: String,

    /// The path to the file containing the HTML template from https://responsivebreakpoints.com/
    #[structopt(parse(from_os_str))]
    pub template: PathBuf,

    /// The S3 sub-directory to add the files to
    #[structopt(short = "d", long = "directory")]
    pub s3_directory: Option<String>,

    /// The location of the Hugo the data file to modify
    #[structopt(short = "o", long = "output", parse(from_os_str))]
    pub output: Option<PathBuf>,

    /// Skip uploading images
    #[structopt(short = "s", long = "skip-upload")]
    pub skip_upload: bool,
}
