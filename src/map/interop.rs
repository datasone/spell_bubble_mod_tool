use std::{
    collections::{HashMap, HashSet},
    env::temp_dir,
    ffi::{c_void, CStr, CString},
    mem,
    os::raw::c_char,
    path::{Path, PathBuf},
    str::FromStr,
};

use itertools::izip;
use maplit::hashset;
use memmem::{Searcher, TwoWaySearcher};

use crate::{
    ffmpeg_helper::convert_file,
    interop::{free_dotnet, ArrayWrapper, DualArrayWrapper, StringWrapper},
    map::{
        enums::{Area, Music},
        BeatsLayout, BpmChanges, Difficulty, Lang, Map, MapScore, SongInfo, SongInfoText,
    },
};

#[repr(C)]
struct SongEntry {
    /// 0 for structs from Rust, 1 for C#
    managed:      u32,
    id:           *const c_char,
    music_entry:  MusicEntry,
    word_entries: ArrayWrapper,
}

impl Drop for SongEntry {
    fn drop(&mut self) {
        if self.managed == 1 {
            unsafe { free_dotnet(self.id as *mut c_void) }
        }
    }
}

#[repr(C)]
struct MusicEntry {
    /// 0 for structs from Rust, 1 for C#
    managed: u32,
    area:    *const c_char,
    bpm:     f32,
    length:  u16,
    /// Ignored when patching files
    dlc_idx: u16,
    offset:  f32,
}

impl Drop for MusicEntry {
    fn drop(&mut self) {
        if self.managed == 1 {
            unsafe { free_dotnet(self.area as *mut c_void) }
        }
    }
}

#[repr(C)]
struct WordEntry {
    /// 0 for structs from Rust, 1 for C#
    managed:     u32,
    lang:        *const c_char,
    title:       *const c_char,
    sub_title:   *const c_char,
    title_kana:  *const c_char,
    artist:      *const c_char,
    artist2:     *const c_char,
    artist_kana: *const c_char,
    original:    *const c_char,
}

impl Drop for WordEntry {
    fn drop(&mut self) {
        if self.managed == 1 {
            unsafe {
                [
                    self.lang,
                    self.title,
                    self.sub_title,
                    self.title_kana,
                    self.artist,
                    self.artist2,
                    self.artist_kana,
                    self.original,
                ]
                .into_iter()
                .for_each(|p| free_dotnet(p as *mut c_void))
            }
        }
    }
}

extern "C" {
    fn patch_acb(
        wav_path: *const c_char,
        acb_path: *const c_char,
        out_acb_path: *const c_char,
        out_awb_path: *const c_char,
    );
    fn patch_score(
        score_path: *const c_char,
        out_path: *const c_char,
        song_id: *const c_char,
        params: ArrayWrapper,
    );
    fn patch_share_data_music_data(
        share_data_path: *const c_char,
        out_file: *const c_char,
        params: ArrayWrapper,
    );
    fn get_music_info(romfs_path: *const c_char) -> DualArrayWrapper;
}

pub(super) fn patch_acb_file(
    music_file: &str,
    acb_path: &Path,
    out_acb_path: &Path,
    out_awb_path: &Path,
    prev_start_ms: u32,
) -> std::io::Result<()> {
    let mut wav_path = temp_dir();
    wav_path.push("hca_convert_tmp.wav");

    let mut i = 0;
    while Path::new(&wav_path).is_file() {
        wav_path.pop();
        wav_path.push(format!("hca_convert_tmp{i}.wav"));
        i += 1;
    }

    let music_file = PathBuf::from(music_file);
    let wav_path = if let Some("wav") = music_file.extension().and_then(|e| e.to_str()) {
        music_file
    } else {
        convert_file(&music_file, &wav_path)?;
        wav_path
    };

    let wav_path_c = CString::new(wav_path.to_string_lossy().to_string()).unwrap();
    let acb_path_c = CString::new(acb_path.to_string_lossy().to_string()).unwrap();
    let out_acb_path_c = CString::new(out_acb_path.to_string_lossy().to_string()).unwrap();
    let out_awb_path_c = CString::new(out_awb_path.to_string_lossy().to_string()).unwrap();

    unsafe {
        patch_acb(
            wav_path_c.as_ptr(),
            acb_path_c.as_ptr(),
            out_acb_path_c.as_ptr(),
            out_awb_path_c.as_ptr(),
        );
    }

    patch_acb_preview(out_acb_path, prev_start_ms)?;

    std::fs::remove_file(&wav_path)?;

    Ok(())
}

/// Patch preview starting point in acb file
/// The preview is controlled by the TrackEvent table in acb file
/// We find "TrackEvent" in the binary, and the offset to the 'T' character is
/// determined by other bytes. The offset is 0x21 when that byte is 0x11, and
/// 0x17 when that byte is 0x0A. The value is stored as milliseconds of the
/// starting point, within big endian.
fn patch_acb_preview(out_acb_path: &Path, prev_start_ms: u32) -> std::io::Result<()> {
    let mut acb_content = std::fs::read(out_acb_path)?;
    let searcher = TwoWaySearcher::new("TrackEvent\x00".as_bytes());

    if let Some(idx) = searcher.search_in(&acb_content) {
        let offset = match acb_content[idx - 1] {
            0x11 => 0x21,
            0x0A => 0x17,
            _ => 0x21, /* We defaults to 0x21 here, if there is more pattern, at least it won't
                        * damage much things */
        };

        let prev_start_ms: [u8; 4] = prev_start_ms.to_be_bytes();
        for i in 1..4 {
            acb_content[idx + offset + (i - 1)] = prev_start_ms[i];
        }
    }

    std::fs::write(out_acb_path, acb_content)?;

    Ok(())
}

pub(super) fn patch_score_file(
    score_file: &Path,
    out_path: &Path,
    song_id: &str,
    scores: &HashMap<Difficulty, MapScore>,
    bpm_changes: &Option<BpmChanges>,
) {
    let len = scores.iter().next().unwrap().1.scores.0.len();
    let mut scores = scores.to_owned();
    let required_keys = hashset![Difficulty::Easy, Difficulty::Normal, Difficulty::Hard];
    let provided_keys = scores.keys().cloned().collect::<HashSet<_>>();
    for difficulty in required_keys.difference(&provided_keys) {
        scores.insert(*difficulty, MapScore::default_with_len(len));
    }

    let mut params: Vec<CString> = vec![];

    let beat_script = bpm_changes
        .as_ref()
        .map(|b| b.to_script())
        .unwrap_or("".to_owned());
    params.push(CString::new(beat_script).unwrap());

    let beats_layout = bpm_changes
        .as_ref()
        .map(|b| b.beats_layout())
        .unwrap_or_default();

    for (difficulty, item) in scores.iter() {
        let difficulty = match difficulty {
            Difficulty::Easy => "Easy",
            Difficulty::Normal => "Normal",
            Difficulty::Hard => "Hard",
        };
        let difficulty = CString::new(difficulty).unwrap();

        let score = CString::new(item.to_script(&beats_layout)).unwrap();
        params.push(difficulty);
        params.push(score);
    }

    let param_ptrs: Vec<*const c_char> = params.iter().map(|s| s.as_ptr()).collect();

    let score_file_c = CString::new(score_file.to_string_lossy().to_string()).unwrap();
    let out_path_c = CString::new(out_path.to_string_lossy().to_string()).unwrap();
    let song_id_c = CString::new(song_id).unwrap();

    unsafe {
        let param = ArrayWrapper {
            managed: 0,
            size:    param_ptrs.len() as u32,
            array:   mem::transmute(param_ptrs.as_ptr()),
        };
        patch_score(
            score_file_c.as_ptr(),
            out_path_c.as_ptr(),
            song_id_c.as_ptr(),
            param,
        );
    }
}

pub(super) fn patch_share_data(share_data_file: &Path, out_path: &Path, maps: &[Map]) {
    let share_data_c = CString::new(share_data_file.to_string_lossy().to_string()).unwrap();
    let out_path_c = CString::new(out_path.to_string_lossy().to_string()).unwrap();

    let mut plus_1s_cstring: Vec<CString> = vec![]; // +1s for objects created in loop
    let mut plus_1s_vec: Vec<Vec<WordEntry>> = vec![]; // +1s for objects created in loop

    let mut song_entries: Vec<SongEntry> = vec![];

    for map in maps {
        let area_c = if map.song_info.area == Area::NotDefined {
            CString::new("").unwrap()
        } else {
            CString::new(map.song_info.area.to_string()).unwrap()
        };
        let area_idx = vec_push_idx(&mut plus_1s_cstring, area_c);

        let music_entry = MusicEntry {
            managed: 0,
            area:    plus_1s_cstring[area_idx].as_ptr(),
            bpm:     map.song_info.bpm,
            length:  map.song_info.length,
            dlc_idx: 0,
            offset:  map.song_info.offset,
        };

        let mut word_entries: Vec<WordEntry> = vec![];

        for (lang, text) in map.song_info.info_text.iter() {
            let lang_c = CString::new(lang.to_string().to_lowercase()).unwrap();
            let title_c = CString::new(text.title.clone()).unwrap();
            let sub_title_c = CString::new(text.sub_title.clone()).unwrap();
            let title_kana_c = CString::new(text.title_kana.clone()).unwrap();
            let artist_c = CString::new(text.artist.clone()).unwrap();
            let artist2_c = CString::new(text.artist2.clone()).unwrap();
            let artist_kana_c = CString::new(text.artist_kana.clone()).unwrap();
            let original_c = CString::new(text.original.clone()).unwrap();

            let lang_idx = vec_push_idx(&mut plus_1s_cstring, lang_c);
            let title_idx = vec_push_idx(&mut plus_1s_cstring, title_c);
            let sub_title_idx = vec_push_idx(&mut plus_1s_cstring, sub_title_c);
            let title_kana_idx = vec_push_idx(&mut plus_1s_cstring, title_kana_c);
            let artist_idx = vec_push_idx(&mut plus_1s_cstring, artist_c);
            let artist2_idx = vec_push_idx(&mut plus_1s_cstring, artist2_c);
            let artist_kana_idx = vec_push_idx(&mut plus_1s_cstring, artist_kana_c);
            let original_idx = vec_push_idx(&mut plus_1s_cstring, original_c);

            let word_entry = WordEntry {
                managed:     0,
                lang:        plus_1s_cstring[lang_idx].as_ptr(),
                title:       plus_1s_cstring[title_idx].as_ptr(),
                sub_title:   plus_1s_cstring[sub_title_idx].as_ptr(),
                title_kana:  plus_1s_cstring[title_kana_idx].as_ptr(),
                artist:      plus_1s_cstring[artist_idx].as_ptr(),
                artist2:     plus_1s_cstring[artist2_idx].as_ptr(),
                artist_kana: plus_1s_cstring[artist_kana_idx].as_ptr(),
                original:    plus_1s_cstring[original_idx].as_ptr(),
            };

            word_entries.push(word_entry);
        }

        let word_entries_idx = vec_push_idx(&mut plus_1s_vec, word_entries);

        let wrapper = unsafe {
            ArrayWrapper {
                managed: 0,
                size:    plus_1s_vec[word_entries_idx].len() as u32,
                array:   mem::transmute(plus_1s_vec[word_entries_idx].as_ptr()),
            }
        };

        let song_id_c = CString::new(map.song_info.id.to_string()).unwrap();
        let song_id_idx = vec_push_idx(&mut plus_1s_cstring, song_id_c);

        let song_entry = SongEntry {
            managed: 0,
            id: plus_1s_cstring[song_id_idx].as_ptr(),
            music_entry,
            word_entries: wrapper,
        };

        song_entries.push(song_entry);
    }

    unsafe {
        let wrapper = ArrayWrapper {
            managed: 0,
            size:    song_entries.len() as u32,
            array:   mem::transmute(song_entries.as_ptr()),
        };
        patch_share_data_music_data(share_data_c.as_ptr(), out_path_c.as_ptr(), wrapper);
    }
}

fn vec_push_idx<T>(vec: &mut Vec<T>, element: T) -> usize {
    vec.push(element);
    vec.len() - 1
}

pub fn get_song_info(romfs_path: &Path) -> Vec<(Map, String, String, String)> {
    let romfs_path_c = CString::new(romfs_path.to_string_lossy().to_string()).unwrap();

    let result = unsafe { get_music_info(romfs_path_c.as_ptr()) };

    let (song_entries, _score_data_outer, score_data) = unsafe {
        let song_entries =
            std::slice::from_raw_parts(result.array as *const SongEntry, result.size as usize);
        let score_data_outer =
            std::slice::from_raw_parts(result.array2 as *const ArrayWrapper, result.size2 as usize);
        let score_data = score_data_outer
            .iter()
            .map(|a| {
                let slice =
                    std::slice::from_raw_parts(a.array as *const *const c_char, a.size as usize);
                slice.iter().map(|&p| StringWrapper(p)).collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        (song_entries, score_data_outer, score_data)
    };

    let word_entries_list = unsafe {
        song_entries
            .iter()
            .map(|se| {
                std::slice::from_raw_parts(
                    se.word_entries.array as *const WordEntry,
                    se.word_entries.size as usize,
                )
            })
            .collect::<Vec<_>>()
    };

    izip!(song_entries, word_entries_list, score_data)
        .map(|(song_entry, word_entries, score_data)| unsafe {
            let id = CStr::from_ptr(song_entry.id).to_str().unwrap();
            let id = Music::from_str(id).unwrap();

            let bpm = song_entry.music_entry.bpm;
            let offset = song_entry.music_entry.offset;
            let length = song_entry.music_entry.length;
            let dlc_index = song_entry.music_entry.dlc_idx;

            let area = CStr::from_ptr(song_entry.music_entry.area)
                .to_str()
                .unwrap();
            let area = Area::from_str(area).unwrap();

            let info_text = word_entries
                .iter()
                .map(|word_entry| {
                    let lang = CStr::from_ptr(word_entry.lang).to_str().unwrap();
                    let lang = Lang::from_str(lang).unwrap();

                    let title = CStr::from_ptr(word_entry.title)
                        .to_str()
                        .unwrap()
                        .to_owned();
                    let title_kana = CStr::from_ptr(word_entry.title_kana)
                        .to_str()
                        .unwrap()
                        .to_owned();
                    let sub_title = CStr::from_ptr(word_entry.sub_title)
                        .to_str()
                        .unwrap()
                        .to_owned();
                    let artist = CStr::from_ptr(word_entry.artist)
                        .to_str()
                        .unwrap()
                        .to_owned();
                    let artist2 = CStr::from_ptr(word_entry.artist2)
                        .to_str()
                        .unwrap()
                        .to_owned();
                    let artist_kana = CStr::from_ptr(word_entry.artist_kana)
                        .to_str()
                        .unwrap()
                        .to_owned();
                    let original = CStr::from_ptr(word_entry.original)
                        .to_str()
                        .unwrap()
                        .to_owned();

                    let info = SongInfoText {
                        title,
                        title_kana,
                        sub_title,
                        artist,
                        artist2,
                        artist_kana,
                        original,
                    };

                    (lang, info)
                })
                .collect::<HashMap<_, _>>();

            let beat = CStr::from_ptr(score_data[0].0).to_str().unwrap().to_owned();

            let bpm_changes = BpmChanges::from_script(&beat);
            let beats_layout = BeatsLayout::from_script(&beat);

            let mut map_scores = HashMap::new();
            let score_easy = CStr::from_ptr(score_data[1].0).to_str().unwrap().to_owned();
            let score_normal = CStr::from_ptr(score_data[2].0).to_str().unwrap().to_owned();
            let score_hard = CStr::from_ptr(score_data[3].0).to_str().unwrap().to_owned();

            map_scores.insert(Difficulty::Easy, MapScore::from_score(&score_easy));
            map_scores.insert(Difficulty::Normal, MapScore::from_score(&score_normal));
            map_scores.insert(Difficulty::Hard, MapScore::from_score(&score_hard));

            let map = Map {
                song_info: SongInfo {
                    id,
                    music_file: "".to_string(),
                    bpm,
                    offset,
                    length,
                    dlc_index,
                    area,
                    info_text,
                    prev_start_ms: 0,
                    bpm_changes,
                    beats_layout,
                },
                map_scores,
            };

            (map, score_easy, score_normal, score_hard)
        })
        .collect::<Vec<_>>()
}
