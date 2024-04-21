use structopt::StructOpt;

use std::path::PathBuf;

#[derive(Debug, PartialEq, Clone)]
pub struct Sizes(pub Vec<usize>);

impl std::str::FromStr for Sizes {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut sizes: Vec<usize> = Vec::new();
        for s in s.split(',') {
            sizes.push(s.parse::<usize>()?)
        }
        Ok(Self(sizes))
    }
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "Responsive Image to Shortcode",
    about = "A tool to generate responsive images for a Hugo site"
)]
pub struct Options {
    /// The path to a folder of images or a single image
    #[structopt(parse(from_os_str))]
    pub image_location: PathBuf,

    /// The name of the set of images. This is used in conjunction with the filename to uniquely identify each image.
    #[structopt(short = "n", long = "name")]
    pub name: String,

    /// The S3 sub-directory to add the files to
    #[structopt(short = "d", long = "directory")]
    pub s3_directory: Option<String>,

    /// The location of the Hugo the data file to modify
    #[structopt(short = "o", long = "output", parse(from_os_str))]
    pub output: Option<PathBuf>,

    /// Skip uploading images
    #[structopt(long = "skip-upload")]
    pub skip_upload: bool,

    /// Skip image resizing
    #[structopt(long = "skip-resize")]
    pub skip_resize: bool,

    /// Force overwrite of existing data
    #[structopt(short = "f", long = "force", alias = "clobber")]
    pub force_overwrite: bool,

    #[structopt(
        long,
        default_value = "320,480,640,768,960,1024,1366,1600,1920,1440,1600,1800"
    )]
    pub sizes: Sizes,
}
