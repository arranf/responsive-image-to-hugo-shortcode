use peck_exif::exif::Exif as PeckExif;
use serde::{Deserialize, Serialize};
use std::convert::From;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Exif {
    pub shutter_speed: Option<String>,
    pub aperture: Option<String>,
    pub camera_model: Option<String>,
    pub focal_length: Option<String>,
    pub lens: Option<String>,
    pub megapixels: Option<String>,
    pub iso: Option<String>,
    pub exposure_compensation: Option<String>,
}

impl From<PeckExif> for Exif {
    fn from(item: PeckExif) -> Self {
        Exif {
            shutter_speed: item.attributes.get("ShutterSpeed").cloned(),
            aperture: item.attributes.get("Aperture").cloned(),
            camera_model: item.attributes.get("CameraModelName").cloned(),
            focal_length: item
                .attributes
                .get("FocalLength")
                .cloned()
                .map(|x| x.replace(')', "")),
            lens: item.attributes.get("LensType").cloned(),
            megapixels: item.attributes.get("Megapixels").cloned(),
            iso: item.attributes.get("ISO").cloned(),
            exposure_compensation: item.attributes.get("ExposureCompensation").cloned(),
        }
    }
}
