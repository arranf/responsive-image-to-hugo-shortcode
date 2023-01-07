use std::str::Utf8Error;

use custom_error::custom_error;

use s3::creds::error::CredentialsError;
use zip::result::ZipError;

custom_error! {pub AppError // Enum name
    // Specific types
    Io{source: std::io::Error}            = "Error performing IO",
    Unzip{source: ZipError} = "Error unzipping images",
    UnzipPath{} = "Error getting Zip path",
    Serde{source: serde_json::error::Error } = "Error saving or fetching data",
    SQIP{} = "Error obtaining SQIP placeholder",
    S3{source: s3::error::S3Error} = "Error uploading to S3",
    RegionFailure{source: Utf8Error} = "Failed to parse region",
    CredentialsError{source: CredentialsError} = "Failed to get AWS credentials"
}
