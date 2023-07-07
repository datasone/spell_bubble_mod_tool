use crate::ffmpeg_helper::convert_file;
use crate::interop::ArrayWrapper;
use crate::map::enums::Area;
use crate::map::{Difficulty, Map, MapItem};
use std::collections::HashMap;
use std::env::temp_dir;
use std::ffi::CString;
use std::mem;
use std::os::raw::c_char;
use std::path::{Path, MAIN_SEPARATOR};

#[repr(C)]
struct SongEntry {
    id: *const c_char,
    music_entry: MusicEntry,
    word_entry: ArrayWrapper,
}

#[repr(C)]
struct MusicEntry {
    area: *const c_char,
    stars_easy: u8,
    stars_normal: u8,
    stars_hard: u8,
    is_bpm_change: u8,
    bpm: u16,
    length: u16,
    duration_sec: f32,
    offset: f32,
}

#[repr(C)]
struct WordEntry {
    lang: *const c_char,
    title: *const c_char,
    sub_title: *const c_char,
    title_kana: *const c_char,
    artist: *const c_char,
    artist2: *const c_char,
    artist_kana: *const c_char,
    original: *const c_char,
}

extern "C" {
    fn patch_acb(wav_path: *const c_char, acb_path: *const c_char, out_acb_path: *const c_char, out_awb_path: *const c_char);
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
}

pub(super) fn patch_acb_file(music_file: &str, acb_path: &str, out_acb_path: &str, out_awb_path: &str) {
    let mut wav_path = format!("{}hca_convert_tmp.wav", temp_dir().to_str().unwrap());
    let mut i = 0;
    while Path::new(&wav_path).is_file() {
        wav_path = format!(
            "{}{}hca_convert_tmp{}.wav",
            temp_dir().to_str().unwrap(),
            MAIN_SEPARATOR,
            i
        );
        i += 1;
    }

    convert_file(music_file, &wav_path).unwrap();

    let wav_path_c = CString::new(wav_path.clone()).unwrap();
    let acb_path_c = CString::new(acb_path).unwrap();
    let out_acb_path_c = CString::new(out_acb_path).unwrap();
    let out_awb_path_c = CString::new(out_awb_path).unwrap();

    unsafe {
        patch_acb(
            wav_path_c.as_ptr(),
            acb_path_c.as_ptr(),
            out_acb_path_c.as_ptr(),
            out_awb_path_c.as_ptr(),
        );
    }

    let _ = std::fs::remove_file(&wav_path);
}

pub(super) fn patch_score_file(
    score_file: &str,
    out_path: &str,
    song_id: &str,
    items: &HashMap<Difficulty, MapItem>,
) {
    let mut params: Vec<CString> = vec![];

    for item in items.values() {
        let difficulty = match item.difficulty {
            Difficulty::EASY => "Easy",
            Difficulty::NORMAL => "Normal",
            Difficulty::HARD => "Hard",
        };
        let difficulty = CString::new(difficulty).unwrap();

        let score = CString::new(item.to_script()).unwrap();
        params.push(difficulty);
        params.push(score);
    }

    let param_ptrs: Vec<*const c_char> = params.iter().map(|s| s.as_ptr()).collect();

    let score_file_c = CString::new(score_file).unwrap();
    let out_path_c = CString::new(out_path).unwrap();
    let song_id_c = CString::new(song_id).unwrap();

    unsafe {
        let param = ArrayWrapper {
            size: param_ptrs.len() as u32,
            array: mem::transmute(param_ptrs.as_ptr()),
        };
        patch_score(
            score_file_c.as_ptr(),
            out_path_c.as_ptr(),
            song_id_c.as_ptr(),
            param,
        );
    }
}

pub(super) fn patch_share_data(share_data_file: &str, out_path: &str, maps: &[Map]) {
    let share_data_c = CString::new(share_data_file).unwrap();
    let out_path_c = CString::new(out_path).unwrap();

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

        let mut stars = vec![0u8; 3];
        for item in map.map_items.values() {
            match item.difficulty {
                Difficulty::EASY => {
                    stars[0] = item.stars;
                }
                Difficulty::NORMAL => {
                    stars[1] = item.stars;
                }
                Difficulty::HARD => {
                    stars[2] = item.stars;
                }
            }
        }

        let music_entry = MusicEntry {
            area: plus_1s_cstring[area_idx].as_ptr(),
            stars_easy: stars[0],
            stars_normal: stars[1],
            stars_hard: stars[2],
            is_bpm_change: if map.song_info.is_bpm_change {1} else {0},
            bpm: map.song_info.info_num.bpm,
            length: map.song_info.info_num.length,
            duration_sec: map.song_info.info_num.duration,
            offset: 0.0, // map.song_info.info_num.offset,
        };

        let mut word_entries: Vec<WordEntry> = vec![];

        for text in map.song_info.info_text.values() {
            let lang_c = CString::new(text.lang.to_string().to_lowercase()).unwrap();
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
                lang: plus_1s_cstring[lang_idx].as_ptr(),
                title: plus_1s_cstring[title_idx].as_ptr(),
                sub_title: plus_1s_cstring[sub_title_idx].as_ptr(),
                title_kana: plus_1s_cstring[title_kana_idx].as_ptr(),
                artist: plus_1s_cstring[artist_idx].as_ptr(),
                artist2: plus_1s_cstring[artist2_idx].as_ptr(),
                artist_kana: plus_1s_cstring[artist_kana_idx].as_ptr(),
                original: plus_1s_cstring[original_idx].as_ptr(),
            };

            word_entries.push(word_entry);
        }

        let word_entries_idx = vec_push_idx(&mut plus_1s_vec, word_entries);

        let wrapper = unsafe {
            ArrayWrapper {
                size: plus_1s_vec[word_entries_idx].len() as u32,
                array: mem::transmute(plus_1s_vec[word_entries_idx].as_ptr()),
            }
        };

        let song_id_c = CString::new(map.song_info.id.to_string()).unwrap();
        let song_id_idx = vec_push_idx(&mut plus_1s_cstring, song_id_c);

        let song_entry = SongEntry {
            id: plus_1s_cstring[song_id_idx].as_ptr(),
            music_entry,
            word_entry: wrapper,
        };

        song_entries.push(song_entry);
    }

    unsafe {
        let wrapper = ArrayWrapper {
            size: song_entries.len() as u32,
            array: mem::transmute(song_entries.as_ptr()),
        };
        patch_share_data_music_data(share_data_c.as_ptr(), out_path_c.as_ptr(), wrapper);
    }
}

fn vec_push_idx<T>(vec: &mut Vec<T>, element: T) -> usize {
    vec.push(element);
    vec.len() - 1
}
