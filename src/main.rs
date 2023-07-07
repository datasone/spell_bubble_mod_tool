mod ffmpeg_helper;
mod interop;
mod map;

use crate::interop::{
    initialize_assets, patch_music_and_character, patch_special_rules, ArrayWrapper,
};
use clap::{Parser, Subcommand};
use map::Map;
use std::ffi::CString;
use std::fs;
use std::mem;
use std::path::{PathBuf, MAIN_SEPARATOR};
use std::process::exit;
use yaml_rust::YamlLoader;

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
    /// Unlocks DLC musics and characters (one DLC must be present, defaults to the first one)
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
    /// Convert external map to mod files (deprecated)
    ConvertExtMap {
        config_file: PathBuf,
    },
}

fn main() {
    let args = Args::parse();

    let switch_path = format!(
        ".{}contents{}0100E9D00D6C2000{}romfs{}Data{}StreamingAssets{}Switch{}",
        MAIN_SEPARATOR,
        MAIN_SEPARATOR,
        MAIN_SEPARATOR,
        MAIN_SEPARATOR,
        MAIN_SEPARATOR,
        MAIN_SEPARATOR,
        MAIN_SEPARATOR
    );

    let out_dir = args.outdir;
    let mut out_path = out_dir.clone();
    out_path.push(switch_path);
    fs::create_dir_all(&out_path).unwrap();

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

            out_path.push(format!(".{}share_data", MAIN_SEPARATOR));

            let share_data_path = CString::new(share_data.to_string_lossy().as_ref()).unwrap();
            let out_path = CString::new(out_path.to_string_lossy().as_ref()).unwrap();
            let left_music_id = CString::new("Lostword").unwrap();

            unsafe {
                let exclude_list_wrapper = ArrayWrapper {
                    size: exclude_list.len() as u32,
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

            out_path.push(format!(".{}share_data", MAIN_SEPARATOR));

            let share_data_path = CString::new(share_data.to_string_lossy().as_ref()).unwrap();
            let out_path = CString::new(out_path.to_string_lossy().as_ref()).unwrap();

            unsafe { patch_special_rules(share_data_path.as_ptr(), out_path.as_ptr()) }
        }
        Commands::ConvertExtMap { config_file } => {
            if !config_file.is_file() {
                println!("Config file does not exist!");
                exit(1)
            }

            let file_content = fs::read_to_string(config_file).unwrap();
            if let Ok(config) = YamlLoader::load_from_str(&file_content) {
                let game_files_dir = config[0]["game_files_dir"].as_str().unwrap();
                let maps = Map::new_from_yaml(&config[0]);
                Map::patch_files(game_files_dir, &out_dir.to_string_lossy(), maps);
            } else {
                println!("Config file parsing error");
                exit(2)
            }
        }
    }
}
