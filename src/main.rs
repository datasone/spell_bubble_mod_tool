mod external_map;
mod ffmpeg_helper;
mod interop;
mod map;

use std::{
    ffi::CString,
    fs, mem,
    path::{Path, PathBuf},
    process::exit,
};

use clap::{Parser, Subcommand};

use crate::interop::{
    initialize_assets, patch_music_and_character, patch_special_rules, ArrayWrapper,
};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
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

        /// Output path of generated content
        outdir: PathBuf,

        /// Exclude DLC IDs from being unlocked
        #[clap(short, long)]
        exclude: Vec<u16>,
    },
    /// Unlock special challenge rules for PvE games
    UnlockSpecialRule {
        /// The path to extracted share_data file
        share_data: PathBuf,

        /// Output path of generated content
        outdir: PathBuf,
    },
    /// Patch game files given map config toml
    PatchMap {
        /// The path to dumped game RomFS files
        romfs_root: PathBuf,

        /// Map config toml file
        maps: PathBuf,

        /// Output path of generated content
        outdir: PathBuf,
    },
    /// Convert map information (length, bpm, offset, scores) between toml and
    /// adofai maps Note that incomplete toml can be generated from adofai,
    /// but in inverted conversion, an existing and valid adofai map must be
    /// present and only scores conversion is supported.
    ConvertAdofai {
        /// The path to adofai map file
        adofai:     PathBuf,
        /// The path to map config toml file
        map:        PathBuf,
        /// Difficulty to choose inside map config
        difficulty: map::Difficulty,
        #[clap(long, short)]
        /// Update n-th element of the map config file, if not exists, add a new
        /// entry
        update:     Option<usize>,
    },
}

fn create_out_dir_structure(out_base: &Path) -> anyhow::Result<PathBuf> {
    let switch_path = "./contents/0100E9D00D6C2000/romfs/Data/StreamingAssets/Switch/";

    let mut assets_switch_out_path = out_base.to_owned();
    assets_switch_out_path.push(switch_path);
    fs::create_dir_all(&assets_switch_out_path)?;

    Ok(assets_switch_out_path)
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    initialize_assets();

    match &args.command {
        Commands::UnlockMusicAndCharacter {
            share_data,
            outdir,
            exclude: exclude_list,
        } => {
            if !share_data.is_file() {
                println!("share_data file does not exist!");
                exit(1)
            };

            let mut assets_switch_out_path = create_out_dir_structure(outdir)?;

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
        Commands::UnlockSpecialRule { share_data, outdir } => {
            if !share_data.is_file() {
                println!("share_data file does not exist!");
                exit(1)
            };

            let mut assets_switch_out_path = create_out_dir_structure(outdir)?;
            assets_switch_out_path.push("share_data");

            let share_data_path = CString::new(share_data.to_string_lossy().as_ref()).unwrap();
            let out_path = CString::new(assets_switch_out_path.to_string_lossy().as_ref()).unwrap();

            unsafe { patch_special_rules(share_data_path.as_ptr(), out_path.as_ptr()) }
        }
        Commands::PatchMap {
            romfs_root,
            maps,
            outdir,
        } => {
            let maps: map::MapsConfig = {
                let content = fs::read_to_string(maps)?;
                toml::from_str(&content)?
            };

            for map in maps.maps.iter() {
                map.validate()?
            }

            map::Map::patch_files(romfs_root, outdir, maps.maps)?;
        }
        Commands::ConvertAdofai {
            adofai,
            map,
            difficulty,
            update,
        } => {
            let mut adofai: external_map::ADoFaIMap = {
                let content = fs::read_to_string(adofai)?;
                serde_json::from_str(content.trim_start_matches('\u{feff}'))?
            };

            let mut maps_config = fs::read_to_string(map)
                .ok()
                .and_then(|s| toml::from_str(&s).ok())
                .unwrap_or(map::MapsConfig { maps: vec![] });

            let map_obj = match maps_config.maps.get_mut(update.unwrap_or(usize::MAX)) {
                Some(map_obj) => map_obj,
                None => {
                    maps_config.maps.push(map::Map::default());
                    maps_config.maps.last_mut().unwrap()
                }
            };

            map_obj.song_info.length = adofai.length() as u16;
            map_obj.song_info.bpm = adofai.bpm();
            map_obj.song_info.offset = adofai.offset();
            map_obj.map_scores.insert(
                *difficulty,
                map::MapScore {
                    stars:  1,
                    scores: map::ScoreData(adofai.scores()),
                },
            );

            let bpm_changes = adofai.bpm_changes();
            if !bpm_changes.is_empty() {
                map_obj.song_info.is_bpm_change = true;
                map_obj.song_info.bpm_changes = map::BpmChanges(bpm_changes).into();
            }

            if map_obj.song_info.info_text.is_empty() {
                map_obj
                    .song_info
                    .info_text
                    .insert(map::Lang::JA, map::SongInfoText::default());
            }

            fs::write(map, toml::to_string_pretty(&maps_config)?)?;
        }
    }

    Ok(())
}
