mod error;

use crate::error::AppError;

use std::fs::{create_dir_all, File};
use std::io::copy;
use std::path::PathBuf;
use zip::ZipArchive;

fn unzip_images(zip_path: &PathBuf, temp_directory: &PathBuf) -> Result<(), AppError> {
    let file = File::open(&zip_path)?;
    let reader = std::io::BufReader::new(file);

    let mut zip = ZipArchive::new(reader)?;
    // TODO: Concurrency
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;

        let outpath = temp_directory.join(file.sanitized_name());
        if (&*file.name()).ends_with('/') {
            println!(
                "File {} extracted to \"{}\"",
                i,
                outpath.as_path().display()
            );
            create_dir_all(&outpath).unwrap();
        } else {
            println!(
                "File {} extracted to \"{}\" ({} bytes)",
                i,
                outpath.as_path().display(),
                file.size()
            );
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    create_dir_all(&p).unwrap();
                }
            }
            println!("{:?}", &outpath);
            let mut outfile = File::create(&outpath).unwrap();
            copy(&mut file, &mut outfile).unwrap();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::read_dir;
    use std::path::PathBuf;
    use std::str::FromStr;
    use tempfile::tempdir;

    #[test]
    fn test_basic() {
        let dir = tempdir().unwrap();
        println!("{}", dir.path().to_string_lossy());
        unzip_images(
            &std::path::PathBuf::from_str("/home/afrance/Downloads/q8e2dqsin57gkjoe4msg.zip")
                .unwrap(),
            &dir.path().to_path_buf(),
        )
        .unwrap();
        let paths = read_dir(dir.path()).unwrap();
        println!("{}", paths.count())
    }
}
