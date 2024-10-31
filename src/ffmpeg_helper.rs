use std::{path::Path, process::Command};

pub fn convert_file(file_path: &Path, dest_path: &Path) -> std::io::Result<()> {
    let mut cmd = Command::new("ffmpeg");

    setup_cmd(&mut cmd);

    cmd.arg("-i").arg(file_path).arg(dest_path).output()?;

    Ok(())
}

#[cfg(windows)]
fn setup_cmd(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn setup_cmd(_cmd: &mut Command) {}
