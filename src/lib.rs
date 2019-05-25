#[macro_use]
extern crate log;

mod constants;
mod error;

use crate::error::AppError;

use std::fs::{create_dir_all, read_dir, DirEntry, File};
use std::io::copy;
use std::path::PathBuf;
use zip::ZipArchive;

fn unzip_images(zip_path: &PathBuf, temp_directory: &PathBuf) -> Result<PathBuf, AppError> {
    let file = File::open(&zip_path)?;
    let reader = std::io::BufReader::new(file);

    let mut zip = ZipArchive::new(reader)?;
    // TODO: Concurrency
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;

        let outpath = temp_directory.join(file.sanitized_name());

        if (&*file.name()).ends_with('/') {
            // Handle directories
            info!(
                "Directory {} extracted to \"{}\"",
                i,
                outpath.as_path().display()
            );
            create_dir_all(&outpath).unwrap();
        } else {
            // Handle files
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    create_dir_all(&p).unwrap();
                }
            }
            let mut outfile = File::create(&outpath)?;
            copy(&mut file, &mut outfile).unwrap();
            info!(
                "File {} extracted to \"{}\" ({} bytes)",
                i,
                outpath.as_path().display(),
                file.size()
            );
        }
    }

    let paths = read_dir(temp_directory)?;
    let directories: Vec<DirEntry> = paths
        .filter_map(|d| d.ok())
        .filter(|d| d.file_type().is_ok() && d.file_type().unwrap().is_dir())
        .collect();
    let zip_inner_path = directories[0].path();
    let temp_directory = temp_directory.join(zip_inner_path);
    Ok(temp_directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::read_dir;
    use std::path::PathBuf;
    use std::str::FromStr;
    use tempfile::tempdir;

    #[test]
    fn test_unzip_images_happy() {
        let dest_dir = tempdir().unwrap();

        // Nothing there to begin with
        let paths = read_dir(dest_dir.path()).unwrap();
        assert_eq!(0, paths.count());

        let directory = unzip_images(
            &PathBuf::from_str("./test/q8e2dqsin57gkjoe4msg.zip").unwrap(),
            &dest_dir.path().to_path_buf(),
        )
        .unwrap();
        // The top level folder should exist
        let paths = read_dir(dest_dir.path()).unwrap();
        assert_eq!(paths.count(), 1);

        // The inner files should have been unzipped
        let file_count = read_dir(directory).unwrap().count();
        assert_eq!(file_count, 18);

        dest_dir.close().unwrap();
    }

    #[test]
    fn test_unzip_images_error() {
        let dest_dir = tempdir().unwrap();
        let zip_dir = PathBuf::from_str("/tmp/bad/path").unwrap();
        let directory = unzip_images(&zip_dir, &dest_dir.path().to_path_buf());
        assert!(directory.is_err());
        dest_dir.close().unwrap();
    }
}
