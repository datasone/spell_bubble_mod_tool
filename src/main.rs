mod external_map;
mod ffmpeg_helper;
mod interop;
mod map;

use std::{
    ffi::{c_char, c_int, CStr, CString},
    fs,
    fs::File,
    io::{BufWriter, Write},
    mem,
    path::{Path, PathBuf},
    process::exit,
};

use clap::{Parser, Subcommand};
use interop::{initialize_assets, ArrayWrapper, StringWrapper};
use itertools::Itertools;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    class_package_path: PathBuf,

    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Unlocks some hidden or DLC-related game features
    UnlockFeatures {
        /// The path to extracted share_data file
        share_data: PathBuf,

        /// Output path of generated content
        outdir: PathBuf,

        /// Unlock special challenge rules for PvE games
        #[clap(short, long)]
        special_rules: bool,

        /// Unlock all musics (including DLC musics and musics in shop, one
        /// music: "Lostword" is kept unlocked to keep the shop functioning
        /// normally)
        #[clap(short, long)]
        musics: bool,

        /// Unlock DLC characters (one DLC must be present, the program sets it
        /// to the first one)
        #[clap(short, long)]
        characters: bool,

        /// Exclude DLC IDs from being unlocked
        #[clap(short, long)]
        exclude: Vec<u16>,
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
    /// Convert map information (length, bpm, offset, scores) from adofai to
    /// toml files
    ConvertAdofai {
        /// The path to adofai map file
        #[clap(required_unless_present("list"))]
        adofai:     Option<PathBuf>,
        /// The path to map config toml file
        map:        PathBuf,
        /// Difficulty to choose inside map config
        #[clap(required_unless_present("list"))]
        difficulty: Option<map::Difficulty>,
        /// Update n-th element of the map config file, if not exists, add a new
        /// entry
        #[clap(long, short)]
        update:     Option<usize>,
        /// List current maps in the config file
        #[clap(long, short)]
        list:       bool,
    },
    /// Extract song information
    ExtractSongInfo {
        /// The path to dumped game RomFS files
        romfs_root: PathBuf,
        /// Output csv file
        out_csv:    PathBuf,
    },
}

fn create_out_dir_structure(out_base: &Path) -> anyhow::Result<PathBuf> {
    let switch_path = "./contents/0100E9D00D6C2000/romfs/Data/StreamingAssets/Switch/";

    let mut assets_switch_out_path = out_base.to_owned();
    assets_switch_out_path.push(switch_path);
    fs::create_dir_all(&assets_switch_out_path)?;

    Ok(assets_switch_out_path)
}

extern "C" {
    pub fn patch_features(
        share_data_path: *const c_char,
        out_path: *const c_char,
        patch_music: c_int, // C style bool, 0 for false, others for true
        excluded_dlcs: ArrayWrapper,
        left_music_id: *const c_char, // Unused for now
        patch_characters: c_int,      // C style bool, 0 for false, others for true
        character_target_dlc: c_int,  // Unused for now
        patch_special_rules: c_int,   // C style bool, 0 for false, others for true
    );

    pub fn get_dlc_list(share_data_path: *const c_char) -> ArrayWrapper;
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    initialize_assets(args.class_package_path);

    match &args.command {
        Commands::UnlockFeatures {
            share_data,
            outdir,
            special_rules,
            musics,
            characters,
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
                    managed: 0,
                    size:    exclude_list.len() as u32,
                    array:   mem::transmute(exclude_list.as_ptr()),
                };

                patch_features(
                    share_data_path.as_ptr(),
                    out_path.as_ptr(),
                    if *musics { 1 } else { 0 },
                    exclude_list_wrapper,
                    left_music_id.as_ptr(),
                    if *characters { 1 } else { 0 },
                    1,
                    if *special_rules { 1 } else { 0 },
                );
            }
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
            list,
        } => {
            let mut maps_config = fs::read_to_string(map)
                .ok()
                .and_then(|s| toml::from_str(&s).ok())
                .unwrap_or(map::MapsConfig { maps: vec![] });

            if *list {
                let output = maps_config
                    .maps
                    .iter()
                    .enumerate()
                    .map(|(i, m)| {
                        let title = m
                            .song_info
                            .info_text
                            .iter()
                            .next()
                            .map(|(_, it)| it.title())
                            .unwrap_or_default();

                        let effective_bpm = m.effective_bpm();
                        let replace = &m.song_info.id;

                        let (level_e, level_n, level_h) = m.levels();

                        format!(
                            "Map {i}: {title}, effective BPM: {effective_bpm}, levels (E/N/H): \
                             {level_e}/{level_n}/{level_h}, id: {replace}"
                        )
                    })
                    .join("\n");

                println!("{output}");
                return Ok(());
            }

            let mut adofai: external_map::ADoFaIMap = {
                let content = fs::read_to_string(adofai.as_ref().unwrap())?;
                serde_json::from_str(content.trim_start_matches('\u{feff}'))?
            };

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
                difficulty.unwrap(),
                map::MapScore {
                    scores: map::ScoreData(adofai.scores()),
                },
            );

            let bpm_changes = adofai.bpm_changes();
            if !bpm_changes.is_empty() {
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
        Commands::ExtractSongInfo {
            romfs_root,
            out_csv,
        } => {
            let mut share_data = romfs_root.clone();
            share_data.push("StreamingAssets/Switch/share_data");
            let share_data_path = CString::new(share_data.to_string_lossy().as_ref()).unwrap();

            let dlcs = unsafe {
                let arr = get_dlc_list(share_data_path.as_ptr());
                let arr = std::slice::from_raw_parts(
                    arr.array as *const *const c_char,
                    arr.size as usize,
                );
                arr.iter().map(|&p| StringWrapper(p)).collect::<Vec<_>>()
            };

            let dlcs = unsafe {
                dlcs.iter()
                    .map(|sw| CStr::from_ptr(sw.0).to_str().unwrap())
                    .collect::<Vec<_>>()
            };

            let maps = map::get_song_info(romfs_root);

            let mut writer = BufWriter::new(File::create(out_csv).unwrap());
            // Write BOM for Windows programs to recognize encoding
            writer.write_all(&[0xEF, 0xBB, 0xBF]).unwrap();
            let mut writer = csv::Writer::from_writer(writer);

            writer
                .write_record([
                    "ID",
                    "Title",
                    "Artist",
                    "Original",
                    "Effective BPM",
                    "Has Tempo Changes",
                    "Levels - Easy",
                    "Levels - Normal",
                    "Levels - Hard",
                    "Length",
                    "Area",
                    "DLC",
                ])
                .unwrap();

            maps.iter()
                .map(|(m, e, n, h)| {
                    let song_info = &m.song_info;
                    let info_text = song_info.info_text.get(&map::Lang::JA).unwrap();
                    writer.write_record(&[
                        song_info.id.to_string(),
                        info_text.title(),
                        info_text.artist(),
                        info_text.original(),
                        m.effective_bpm().to_string(),
                        song_info.is_bpm_change().to_string(),
                        m.level(map::Difficulty::Easy, Some(e)).to_string(),
                        m.level(map::Difficulty::Normal, Some(n)).to_string(),
                        m.level(map::Difficulty::Hard, Some(h)).to_string(),
                        song_info.length.to_string(),
                        song_info.area.to_string(),
                        if song_info.dlc_index == 0 {
                            "本体"
                        } else {
                            dlcs[song_info.dlc_index as usize - 1]
                        }
                        .to_string(),
                    ])
                })
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
        }
    }

    Ok(())
}
