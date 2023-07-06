use std::error::Error;
use std::process::Command;

pub fn get_duration(file_path: &str) -> Result<f32, Box<dyn Error>> {
    let output = Command::new("ffprobe")
        .arg("-i")
        .arg(file_path)
        .arg("-show_entries")
        .arg("format=duration")
        .arg("-v")
        .arg("quiet")
        .arg("-of")
        .arg("csv=p=0")
        .output()?;

    let duration = std::str::from_utf8(&output.stdout)?;
    let duration = duration.trim_end().parse::<f32>()?;

    Ok(duration)
}

pub fn convert_file(file_path: &str, dest_path: &str) -> Result<(), Box<dyn Error>> {
    Command::new("ffmpeg")
        .arg("-i")
        .arg(file_path)
        .arg(dest_path)
        .output()?;

    Ok(())
}