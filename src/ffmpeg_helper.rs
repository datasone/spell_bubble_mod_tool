use std::{path::Path, process::Command};

pub fn convert_file(file_path: &Path, dest_path: &Path) -> std::io::Result<()> {
    Command::new("ffmpeg")
        .arg("-i")
        .arg(file_path)
        .arg(dest_path)
        .output()?;

    Ok(())
}
