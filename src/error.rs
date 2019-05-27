use custom_error::custom_error;

use zip::result::ZipError;

custom_error! {pub AppError // Enum name
    // Specific types
    Io{source: std::io::Error}            = "Error performing IO",
    Unzip{source: ZipError} = "Error unzipping images",
    Serde{source: serde_json::error::Error } = "Error saving or fetching data",
    SQIP{} = "Error obtaining SQIP placeholder",
    S3{source: s3::error::S3Error} = "Error uploadging to S3"
}
