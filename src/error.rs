use custom_error::custom_error;
use zip::result::ZipError;

custom_error! {pub AppError // Enum name
    // Specific types
    // InvalidDeckEncoding{encoding_type: String} = "Invalid deck encoding: {encoding_type}.",
    Io{source: std::io::Error}            = "Error performing IO",
    Unzip{source: ZipError} = "Error unzipping images"
}
