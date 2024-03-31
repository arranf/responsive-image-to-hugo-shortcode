use thiserror::Error;

use std::{io, str::Utf8Error};

use s3::creds::error::CredentialsError;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Error reading from or writing to filesystem")]
    Io(#[from] io::Error),
    #[error("Error reading from or writing to Hugo's data file")]
    Serde(#[from] serde_json::error::Error),
    #[error("Error obtaining SQIP placeholder")]
    SQIP(),
    #[error("Error uploading to S3")]
    S3(#[from] s3::error::S3Error),
    #[error("Failed to parse region")]
    RegionParse(#[from] Utf8Error),
    #[error("Failed to get AWS credentials")]
    Credentials(#[from] CredentialsError),
    #[error("Key already exists in data template")]
    KeyAlreadyExists,
    #[error("Image is too small")]
    ImageTooSmall,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
