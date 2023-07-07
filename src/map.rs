mod enums;
mod interop;

use std::collections::HashMap;
use std::str::FromStr;
use yaml_rust::Yaml;

use crate::ext_map_parser::osu::OsuMap;
use crate::ffmpeg_helper::get_duration;
use crate::map::interop::{patch_acb_file, patch_score_file, patch_share_data};
use enums::{Area, Music};
use std::cmp::max;
use std::fs;
use std::path::MAIN_SEPARATOR;
use std::process::exit;

#[derive(Eq, PartialEq, Hash, strum::Display)]
enum Lang {
    JA,
    EN,
    KO,
    CHS,
    CHT,
}

impl Default for Lang {
    fn default() -> Self {
        Lang::JA
    }
}

#[derive(Default)]
struct SongInfoText {
    title: String,
    title_kana: String,
    sub_title: String,
    artist: String,
    artist2: String,
    artist_kana: String,
    original: String,
    lang: Lang,
}

impl SongInfoText {
    fn validate(&self) -> bool {
        !self.artist.is_empty() && !self.artist.is_empty()
    }
}

#[derive(Default)]
struct SongInfoNum {
    bpm: u16,
    duration: f32,
    offset: f32,
    length: u16,
}

#[derive(Default)]
struct SongInfo {
    id: Music,
    music_file: String,
    info_num: SongInfoNum,
    area: Area,
    info_text: HashMap<Lang, SongInfoText>,
    is_bpm_change: bool,
}

impl SongInfo {
    fn validate(&self) -> bool {
        self.info_text
            .iter()
            .all(|(lang, text)| *lang == text.lang && text.validate())
    }

    // bpm, duration, length and offset is unfilled
    fn new_from_yaml(yml: &Yaml) -> SongInfo {
        let mut song_info = SongInfo::default();
        let mut info_text = SongInfoText::default();

        let id = yml["song_id"].as_str().unwrap();
        let id: Music = Music::from_str(id).unwrap();
        song_info.id = id;
        song_info.music_file = yml["music_file"].as_str().unwrap().to_owned();

        info_text.lang = Lang::JA;
        info_text.title = yml["title"].as_str().unwrap().to_owned();
        info_text.sub_title = yml["sub_title"].as_str().unwrap_or_default().to_owned();
        info_text.artist = yml["artist"].as_str().unwrap().to_owned();
        info_text.artist2 = yml["artist2"].as_str().unwrap_or_default().to_owned();
        info_text.original = yml["original"].as_str().unwrap_or_default().to_owned();

        let area = yml["area"].as_str().unwrap_or_default().to_owned();
        let area: Area = Area::from_str(&area).unwrap_or_default();
        song_info.area = area;

        song_info.info_text.insert(Lang::JA, info_text);
        song_info
    }
}

#[derive(PartialEq, Eq, Hash, Clone)]
enum Difficulty {
    EASY,
    NORMAL,
    HARD,
}

#[derive(strum::Display, Debug, Clone, PartialEq)]
enum MapEntry {
    // Normal
    O,
    // Blank (-)
    #[strum(serialize = "-")]
    B,
    // Heavy
    S,
}

struct MapItem {
    difficulty: Difficulty,
    stars: u8,
    map_data: Vec<MapEntry>,
}

impl MapItem {
    fn to_script(&self) -> String {
        let map_data_in_str: Vec<String> = self.map_data.iter().map(|e| e.to_string()).collect();
        let map_str_chunks: Vec<String> = map_data_in_str
            .chunks(4)
            .map(|ch| ch.join(", ") + ",")
            .collect();
        map_str_chunks.join("\n") + " "
    }

    fn new_from_yaml(
        yaml: &Yaml,
        duration: f32,
        is_first: bool,
        bpm: u16,
    ) -> (MapItem, SongInfoNum) {
        let difficulty = match yaml["level"].as_str().unwrap_or_default() {
            "easy" => Difficulty::EASY,
            "normal" => Difficulty::NORMAL,
            "hard" => Difficulty::HARD,
            _ => Difficulty::NORMAL,
        };

        let stars = yaml["stars"].as_i64().unwrap_or_default() as u8;

        let osu_file_path = yaml["ext_map_file"].as_str().unwrap_or_default();
        let osu_content = fs::read_to_string(osu_file_path).unwrap();
        let osu = OsuMap::from_str(&osu_content).unwrap();

        let bpm = if is_first {
            MapItem::calc_bpm(&osu)
        } else {
            bpm
        };
        let offset = max(osu.hit_objects.first().unwrap().time() - 100, 0);
        let offset = offset as f32 / 1000f32;

        let item = MapItem {
            difficulty,
            stars,
            map_data: MapItem::map_data_from_osu(&osu, bpm, duration, offset),
        };

        let info_num = SongInfoNum {
            bpm,
            duration,
            offset,
            length: item.map_data.len() as u16,
        };

        (item, info_num)
    }

    fn calc_bpm(osu: &OsuMap) -> u16 {
        let osu_bpm = osu
            .timing_points
            .iter()
            .find(|tp| !tp.is_inherited())
            .unwrap()
            .bpm()
            .unwrap();
        let mut distances: Vec<u32> = vec![];

        let mut i = 0;
        while i < osu.hit_objects.len() - 1 {
            distances.push(osu.hit_objects[i + 1].time() - osu.hit_objects[i].time());
            i += 1
        }

        let min_distance = *distances.iter().min().unwrap();
        let max_npm = (60000f32 / min_distance as f32).round() as u16;

        let mut bpm = osu_bpm;

        if max_npm > osu_bpm {
            println!("Osu BPM: {}\nLargest NPM: {}", osu_bpm, max_npm);
            println!(
                "Input desired BPM, it's recommended to be the integral multiple of the osu BPM."
            );
            let possible_bpm_list: Vec<u16> =
                (1..(max_npm / osu_bpm)).map(|i| i * osu_bpm).collect();
            println!("Possible values: {:?}", possible_bpm_list);

            let mut line = String::new();
            std::io::stdin().read_line(&mut line).unwrap();
            bpm = line.trim_end().parse().unwrap();
        }

        bpm
    }

    fn map_data_from_osu(osu: &OsuMap, bpm: u16, duration: f32, offset: f32) -> Vec<MapEntry> {
        let step_in_ms = 60000f32 / bpm as f32;

        let length = (duration / 60f32 * bpm as f32).round() as u32;
        let mut map_data = vec![MapEntry::B; length as usize];

        for hit_obj in &osu.hit_objects {
            let idx_range = hit_obj.time()..hit_obj.time() + hit_obj.duration_time() + 1;
            let idx_range = (idx_range.start as f32 / step_in_ms) as usize
                ..(idx_range.end as f32 / step_in_ms) as usize + 1;

            for idx in idx_range.clone() {
                if hit_obj.strong_point() == -1 {
                    // Spinner
                    map_data[idx] = MapEntry::S;
                } else {
                    map_data[idx] = MapEntry::O;
                }
            }

            if hit_obj.strong_point() > 0 {
                // Slider
                map_data[idx_range.start] = MapEntry::S;
                map_data[idx_range.end] = MapEntry::S;

                let segment = idx_range.len() / hit_obj.strong_point() as usize;
                let strong_idxs: Vec<usize> = (1..hit_obj.strong_point())
                    .map(|i| idx_range.start + segment * i as usize)
                    .collect();
                for idx in strong_idxs {
                    map_data[idx] = MapEntry::S;
                }
            }
        }

        MapItem::refine_beats(&mut map_data, bpm);

        map_data
    }

    fn find_segments(beats: &[MapEntry], find_blank: bool) -> Vec<(usize, usize)> {
        let mut segments: Vec<(usize, usize)> = vec![]; // (start, count)

        let mut count = 0;
        let mut start = 0;
        for (i, beat) in beats.iter().enumerate() {
            if (*beat != MapEntry::B) != find_blank {
                if count == 0 {
                    start = i
                }
                count += 1
            } else if count != 0 {
                segments.push((start, count));
                count = 0;
            }
        }

        if count != 0 {
            segments.push((start, count));
            count = 0;
        }

        segments
    }

    fn split_segments(beats: &mut Vec<MapEntry>, max_length: u8, ratio: f32) {
        let mut segments: Vec<(usize, usize)> = MapItem::find_segments(beats, false);

        while segments.iter().any(|e| e.1 > max_length as usize) {
            let long_segments: Vec<&(usize, usize)> = segments
                .iter()
                .filter(|s| s.1 > max_length as usize)
                .collect();
            let segment_count = (long_segments.len() as f32 * ratio).round() as usize;
            let step = long_segments.len() / segment_count;
            let mut indices: Vec<usize> = vec![];
            let mut i = 0;
            while i < long_segments.len() {
                indices.push(i);
                i += step;
            }

            for i in indices {
                let (start, length) = long_segments[i];
                let scores: Vec<(i32, usize, usize, usize)> =
                    (2..max_length).map(|s| split_score(*length, s)).collect();
                let &max = scores.iter().max_by_key(|p| p.0).unwrap();

                let mut i = 0;
                while i < max.2 {
                    beats[start + i * (max.1 + 1)] = MapEntry::B;
                    i += 1;
                }

                if max.3 == 1 {
                    beats[start + length - 1] = MapEntry::S;
                }
            }

            if ratio != 1f32 {
                break;
            }
            segments = MapItem::find_segments(beats, false);
        }
    }

    fn fill_gap(beats: &mut Vec<MapEntry>, gap_length: f32, bpm: u16) {
        let blank_segments: Vec<(usize, usize)> = MapItem::find_segments(beats, true);
        let gap_length = gap_length / 60.0 * bpm as f32;
        let gap_length = gap_length.round() as usize;

        let long_blank_segments: Vec<&(usize, usize)> =
            blank_segments.iter().filter(|s| s.1 > gap_length).collect();
        for (start, length) in long_blank_segments {
            let mut i = gap_length;

            while i < start + length {
                beats[i] = MapEntry::O;
                i += 5;
            }
        }
    }

    fn refine_beats(beats: &mut Vec<MapEntry>, bpm: u16) {
        // TODO: What should be done here? Split longer than 9, prevent too much long segment, putting padding between large space
        MapItem::split_segments(beats, 9, 1f32);
        MapItem::split_segments(beats, 5, 0.75);

        MapItem::fill_gap(beats, 3f32, bpm);
    }
}

fn split_score(length: usize, split: u8) -> (i32, usize, usize, usize) {
    let split = split as usize;
    let mut quotient = length / split;
    let mut remainder = length % split;
    if remainder < quotient {
        quotient -= 1;
    }
    quotient = (length - quotient) / split;
    remainder = (length - quotient) % split;

    (
        split as i32 - quotient as i32 - remainder as i32,
        split,
        quotient,
        remainder,
    )
}

#[derive(Default)]
pub struct Map {
    song_info: SongInfo,
    map_items: HashMap<Difficulty, MapItem>,
}

impl Map {
    fn validate(&self) -> bool {
        self.map_items.iter().all(|(difficulty, item)| {
            *difficulty == item.difficulty
                && item.map_data.len() == self.song_info.info_num.length as usize
        })
    }

    pub fn new_from_yaml(yaml: &Yaml) -> Vec<Map> {
        let mut maps: Vec<Map> = vec![];

        let songs = yaml["songs"].as_vec().unwrap();
        for song in songs {
            let mut map = Map::default();

            let mut song_info = SongInfo::new_from_yaml(song);

            let music_file = &song_info.music_file;
            let duration = get_duration(music_file);
            let duration = match duration {
                Ok(duration) => duration,
                Err(err) => {
                    println!("Error processing music file: {err}!");
                    exit(3);
                }
            };

            let levels = song["levels"].as_vec().unwrap();
            for level in levels {
                let is_first = level == levels.get(0).unwrap();
                let (map_item, info_num) =
                    MapItem::new_from_yaml(level, duration, is_first, song_info.info_num.bpm);
                if is_first {
                    song_info.info_num = info_num
                };

                map.map_items.insert(map_item.difficulty.clone(), map_item);
            }

            map.song_info = song_info;

            if map.validate() {
                maps.push(map)
            }
        }

        maps
    }

    pub fn patch_files(game_files_dir: &str, out_dir: &str, maps: Vec<Map>) {
        let share_data_path = format!(
            "StreamingAssets{}Switch{}share_data",
            MAIN_SEPARATOR, MAIN_SEPARATOR
        );

        let out_base_path = format!(
            "{}{}contents{}0100E9D00D6C2000{}romfs{}Data",
            out_dir, MAIN_SEPARATOR, MAIN_SEPARATOR, MAIN_SEPARATOR, MAIN_SEPARATOR
        );

        let directories = vec![
            format!(
                "{}{}StreamingAssets{}Switch{}scores",
                &out_base_path, MAIN_SEPARATOR, MAIN_SEPARATOR, MAIN_SEPARATOR
            ),
            format!(
                "{}{}StreamingAssets{}Sounds",
                &out_base_path, MAIN_SEPARATOR, MAIN_SEPARATOR
            ),
        ];
        directories
            .iter()
            .for_each(|d| std::fs::create_dir_all(d).unwrap());

        for map in &maps {
            let song_id = map.song_info.id.to_string();

            let score_path = format!(
                "StreamingAssets{}Switch{}scores{}score_{}",
                MAIN_SEPARATOR,
                MAIN_SEPARATOR,
                MAIN_SEPARATOR,
                song_id.to_lowercase()
            );
            let acb_path = format!(
                "StreamingAssets{}Sounds{}BGM_{}.acb",
                MAIN_SEPARATOR,
                MAIN_SEPARATOR,
                song_id.to_uppercase()
            );
            let awb_path = format!(
                "StreamingAssets{}Sounds{}BGM_{}.awb",
                MAIN_SEPARATOR,
                MAIN_SEPARATOR,
                song_id.to_uppercase()
            );

            patch_acb_file(
                &map.song_info.music_file,
                &format!("{}{}{}", game_files_dir, MAIN_SEPARATOR, acb_path),
                &format!("{}{}{}", out_base_path, MAIN_SEPARATOR, acb_path),
                &format!("{}{}{}", out_base_path, MAIN_SEPARATOR, awb_path),
            );

            patch_score_file(
                &format!("{}{}{}", game_files_dir, MAIN_SEPARATOR, score_path),
                &format!("{}{}{}", out_base_path, MAIN_SEPARATOR, score_path),
                &song_id,
                &map.map_items,
            );
        }

        patch_share_data(
            &format!("{}{}{}", game_files_dir, MAIN_SEPARATOR, share_data_path),
            &format!("{}{}{}", out_base_path, MAIN_SEPARATOR, share_data_path),
            &maps,
        )
    }
}
