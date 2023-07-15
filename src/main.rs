mod ffmpeg_helper;
mod interop;
mod map;

use std::{ffi::CString, fs, mem, path::PathBuf, process::exit};

use clap::{Parser, Subcommand};

use crate::interop::{
    initialize_assets, patch_music_and_character, patch_special_rules, ArrayWrapper,
};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Output path of generated content
    outdir: PathBuf,

    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Unlocks DLC musics and characters (one DLC must be present, defaults to
    /// the first one)
    UnlockMusicAndCharacter {
        /// The path to extracted share_data file
        share_data: PathBuf,

        /// Exclude DLC IDs from being unlocked
        #[clap(short, long)]
        exclude: Vec<u16>,
    },
    /// Unlock special challenge rules for PvE games
    UnlockSpecialRule {
        /// The path to extracted share_data file
        share_data: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let switch_path = "./contents/0100E9D00D6C2000/romfs/Data/StreamingAssets/Switch/";

    let out_dir = args.outdir;
    let mut assets_switch_out_path = out_dir.clone();
    assets_switch_out_path.push(switch_path);
    fs::create_dir_all(&assets_switch_out_path)?;

    initialize_assets();

    match &args.command {
        Commands::UnlockMusicAndCharacter {
            share_data,
            exclude: exclude_list,
        } => {
            if !share_data.is_file() {
                println!("share_data file does not exist!");
                exit(1)
            };

            assets_switch_out_path.push("share_data");

            let share_data_path = CString::new(share_data.to_string_lossy().as_ref()).unwrap();
            let out_path = CString::new(assets_switch_out_path.to_string_lossy().as_ref()).unwrap();
            let left_music_id = CString::new("Lostword").unwrap();

            unsafe {
                let exclude_list_wrapper = ArrayWrapper {
                    size:  exclude_list.len() as u32,
                    array: mem::transmute(exclude_list.as_ptr()),
                };

                patch_music_and_character(
                    share_data_path.as_ptr(),
                    out_path.as_ptr(),
                    exclude_list_wrapper,
                    left_music_id.as_ptr(),
                    1,
                    1,
                );
            }
        }
        Commands::UnlockSpecialRule { share_data } => {
            if !share_data.is_file() {
                println!("share_data file does not exist!");
                exit(1)
            };

            assets_switch_out_path.push("share_data");

            let share_data_path = CString::new(share_data.to_string_lossy().as_ref()).unwrap();
            let out_path = CString::new(assets_switch_out_path.to_string_lossy().as_ref()).unwrap();

            unsafe { patch_special_rules(share_data_path.as_ptr(), out_path.as_ptr()) }
        }
    }

    Ok(())
}
