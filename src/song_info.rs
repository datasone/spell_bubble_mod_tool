use std::{
    ffi::{c_char, CStr, CString},
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

use crate::{
    interop::{ArrayWrapper, StringWrapper},
    map::{Difficulty::*, Lang::JA},
};

extern "C" {
    pub fn get_dlc_list(share_data_path: *const c_char) -> ArrayWrapper;
}

pub struct MapInfo {
    pub map:     crate::map::Map,
    pub score_e: String,
    pub score_n: String,
    pub score_h: String,
}

pub struct SongInfos {
    pub maps: Vec<MapInfo>,
    pub dlcs: Vec<String>,
}

pub fn get_song_info(romfs_root: &Path) -> SongInfos {
    let mut share_data = romfs_root.to_owned();
    share_data.push("StreamingAssets/Switch/share_data");
    let share_data_path = CString::new(share_data.to_string_lossy().as_ref()).unwrap();

    let dlcs = unsafe {
        let arr = get_dlc_list(share_data_path.as_ptr());
        let arr = std::slice::from_raw_parts(arr.array as *const *const c_char, arr.size as usize);
        arr.iter().map(|&p| StringWrapper(p)).collect::<Vec<_>>()
    };

    let dlcs = unsafe {
        dlcs.iter()
            .map(|sw| CStr::from_ptr(sw.0).to_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };

    let maps = crate::map::get_song_info(romfs_root);

    let maps = maps
        .into_iter()
        .map(|(map, level_e, level_n, level_h)| MapInfo {
            map,
            score_e: level_e,
            score_n: level_n,
            score_h: level_h,
        })
        .collect();

    SongInfos { maps, dlcs }
}

pub fn write_song_info_csv(infos: SongInfos, out_path: &Path) {
    let mut writer = BufWriter::new(File::create(out_path).unwrap());
    if cfg!(windows) {
        // Write BOM for Windows programs to recognize encoding
        writer.write_all(&[0xEF, 0xBB, 0xBF]).unwrap();
    }
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

    infos
        .maps
        .iter()
        .map(|map_info| {
            let song_info = &map_info.map.song_info;
            let info_text = song_info.info_text.get(&JA).unwrap();
            writer.write_record(&[
                song_info.id.to_string(),
                info_text.title(),
                info_text.artist(),
                info_text.original(),
                map_info.map.effective_bpm().to_string(),
                song_info.is_bpm_change().to_string(),
                map_info
                    .map
                    .level(Easy, Some(&map_info.score_e))
                    .to_string(),
                map_info
                    .map
                    .level(Normal, Some(&map_info.score_n))
                    .to_string(),
                map_info
                    .map
                    .level(Hard, Some(&map_info.score_h))
                    .to_string(),
                song_info.length.to_string(),
                song_info.area.to_string(),
                if song_info.dlc_index == 0 {
                    "本体"
                } else {
                    &infos.dlcs[song_info.dlc_index as usize - 1]
                }
                .to_string(),
            ])
        })
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
}
