use std::{path::Path, process::Command};

pub fn convert_file(file_path: &Path, dest_path: &Path) -> std::io::Result<()> {
    let mut cmd = Command::new("ffmpeg");

    if cfg!(windows) {
        use std::os::windows::process::CommandExt;

        cmd = cmd.creation_flags(CREATE_NO_WINDOW);
    }

    cmd
        .arg("-i")
        .arg(file_path)
        .arg(dest_path)
        .output()?;

    Ok(())
}
