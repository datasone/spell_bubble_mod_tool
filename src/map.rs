mod enums;
mod interop;

use std::{
    collections::HashMap, path::MAIN_SEPARATOR, str::FromStr,
};
use itertools::Itertools;

use enums::{Area, Music};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::{serde_as, DisplayFromStr};


use crate::{
    map::interop::{patch_acb_file, patch_score_file, patch_share_data},
};

#[derive(
    Eq, PartialEq, Hash, Clone, strum::Display, strum::EnumString, Serialize, Deserialize, Default,
)]
enum Lang {
    #[default]
    JA,
    EN,
    KO,
    Chs,
    Cht,
}

#[derive(Default, Serialize, Deserialize)]
struct SongInfoText {
    title:       String,
    title_kana:  String,
    sub_title:   String,
    artist:      String,
    artist2:     String,
    artist_kana: String,
    original:    String,
}

impl SongInfoText {
    fn validate(&self) -> bool {
        !self.artist.is_empty() && !self.artist.is_empty()
    }
}

/// (u16, u16) is Index, TargetBpm pair
#[derive(Default, Serialize, Deserialize)]
struct BpmChanges(Vec<(u16, u16)>);

impl BpmChanges {
    fn to_script(&self) -> String {
        let beats = self.beats_layout().0.into_iter().map(|(i, len)| format!("{i}:{len},")).join("\n");
        let bpm_changes = self.0.iter().map(|(i, bpm)| format!("[BPM]{i}:{bpm}")).join("\n");

        format!("{}\n{}", beats, bpm_changes)
    }

    fn beats_layout(&self) -> BeatsLayout {
        let mut beats = vec![];

        let mut remainder = 0;

        for (i, _) in &self.0 {
            let line = (i - remainder) / 4 + 1 + beats.len() as u16 / 2;
            let line_len = (i - remainder) % 4;
            remainder += line_len;

            if line_len != 0 {
                beats.push((line, line_len));
                beats.push((line + 1, 4));
            }
        }

        BeatsLayout(beats)
    }
}

#[derive(Debug)]
/// (u16, u16) is Index, LineLength pair
struct BeatsLayout(Vec<(u16, u16)>);

#[serde_as]
#[derive(Default, Serialize, Deserialize)]
struct SongInfo {
    id:            Music,
    music_file:    String,
    bpm:           u16,
    duration:      f32,
    offset:        f32,
    length:        u16,
    area:          Area,
    #[serde_as(as = "HashMap<DisplayFromStr, _>")]
    info_text:     HashMap<Lang, SongInfoText>,
    is_bpm_change: bool,
    bpm_changes:   Option<BpmChanges>,
}

impl SongInfo {
    fn validate(&self) -> bool {
        let bpm_change_validate = self.is_bpm_change ^ self.bpm_changes.is_none();
        bpm_change_validate && self.info_text.iter().all(|(_lang, text)| text.validate())
    }
}

#[derive(strum::Display, strum::EnumString, Eq, PartialEq, Hash, Copy, Clone, Serialize, Deserialize)]
enum Difficulty {
    Easy,
    Normal,
    Hard,
}

#[derive(strum::Display, strum::EnumString, Debug, Clone, PartialEq)]
enum MapEntry {
    // Normal
    O,
    // Blank (-)
    #[strum(serialize = "-")]
    B,
    // Heavy
    S,
}

#[derive(Clone)]
struct ScoreData(Vec<MapEntry>);

impl ToString for ScoreData {
    fn to_string(&self) -> String {
        self.0.iter().map(|e| e.to_string()).collect()
    }
}

impl FromStr for ScoreData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.chars()
            .map(|c| MapEntry::from_str(&c.to_string()))
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }
}

impl Serialize for ScoreData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ScoreData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct MapScore {
    stars:  u8,
    scores: ScoreData,
}

impl MapScore {
    fn default_with_len(len: usize) -> Self {
        Self {
            stars: 0,
            scores: ScoreData(vec![MapEntry::B; len])
        }
    }

    fn to_script(&self) -> String {
        let map_data_in_str: Vec<String> = self.scores.0.iter().map(|e| e.to_string()).collect();
        let map_str_chunks: Vec<String> = map_data_in_str
            .chunks(4)
            .map(|ch| ch.join(", ") + ",")
            .collect();
        map_str_chunks.join("\n") + " "
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
        }

        segments
    }

    fn split_segments(beats: &mut [MapEntry], max_length: u8, ratio: f32) {
        let mut segments: Vec<(usize, usize)> = MapScore::find_segments(beats, false);

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
            segments = MapScore::find_segments(beats, false);
        }
    }

    fn fill_gap(beats: &mut [MapEntry], gap_length: f32, bpm: u16) {
        let blank_segments: Vec<(usize, usize)> = MapScore::find_segments(beats, true);
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

    fn refine_beats(beats: &mut [MapEntry], bpm: u16) {
        // TODO: What should be done here? Split longer than 9, prevent too much long
        // segment, putting padding between large space
        MapScore::split_segments(beats, 9, 1f32);
        MapScore::split_segments(beats, 5, 0.75);

        MapScore::fill_gap(beats, 3f32, bpm);
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

#[serde_as]
#[derive(Default, Serialize, Deserialize)]
pub struct Map {
    song_info:  SongInfo,
    #[serde_as(as = "HashMap<DisplayFromStr, _>")]
    map_scores: HashMap<Difficulty, MapScore>,
}

impl Map {
    fn validate(&self) -> bool {
        self.map_scores
            .iter()
            .all(|(_difficulty, score)| score.scores.0.len() == self.song_info.length as usize)
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
                &map.map_scores,
                &map.song_info.bpm_changes,
            );
        }

        patch_share_data(
            &format!("{}{}{}", game_files_dir, MAIN_SEPARATOR, share_data_path),
            &format!("{}{}{}", out_base_path, MAIN_SEPARATOR, share_data_path),
            &maps,
        )
    }
}

#[derive(Serialize, Deserialize)]
struct MapsConfig {
    maps: Vec<Map>,
}

#[cfg(test)]
mod test {
    use maplit::hashmap;

    use super::*;

    #[test]
    fn generate_example_toml() {
        let map1 = Map {
            song_info:  SongInfo {
                id:            Music::Agepoyo,
                music_file:    "file_path".to_string(),
                bpm:           150,
                duration:      150.0,
                offset:        0.01,
                length:        1500,
                area:          Area::Arena,
                info_text:     hashmap! {
                    Lang::JA => SongInfoText {
                        title: "Title".to_string(),
                        title_kana: "TitleKana".to_string(),
                        sub_title: "SubTitle".to_string(),
                        artist: "Artist".to_string(),
                        artist2: "Artist2".to_string(),
                        artist_kana: "ArtistKana".to_string(),
                        original: "Original".to_string(),
                    }
                },
                is_bpm_change: false,
                bpm_changes: None,
            },
            map_scores: hashmap! {
                Difficulty::Hard => MapScore {
                    stars: 10,
                    scores: ScoreData::from_str("SO-SO-SO-SO-SO----SOS-OO").unwrap(),
                }
            },
        };

        let map2 = Map {
            song_info:  SongInfo {
                id:            Music::Alice,
                music_file:    "file_path2".to_string(),
                bpm:           152,
                duration:      152.0,
                offset:        0.02,
                length:        1502,
                area:          Area::ArenaNight,
                info_text:     hashmap! {
                    Lang::JA => SongInfoText {
                        title: "Title2".to_string(),
                        title_kana: "TitleKana2".to_string(),
                        sub_title: "SubTitle2".to_string(),
                        artist: "Artist2".to_string(),
                        artist2: "Artist2_2".to_string(),
                        artist_kana: "ArtistKana2".to_string(),
                        original: "Original2".to_string(),
                    }
                },
                is_bpm_change: true,
                bpm_changes: Some(BpmChanges(vec![(100, 150), (150, 50)])),
            },
            map_scores: hashmap! {
                Difficulty::Hard => MapScore {
                    stars: 10,
                    scores: ScoreData::from_str("--SO---SO-SSSOOSOO-OOOS---").unwrap(),
                }
            },
        };

        let maps = MapsConfig {
            maps: vec![map1, map2],
        };

        println!("{}", toml::to_string_pretty(&maps).unwrap());
    }

    #[test]
    fn test_beats_layout() {
        let bpm_changes = BpmChanges(
            vec![
                (118 * 4, 200),
                (130 * 4, 400),
                (206 * 4, 200),
                (207 * 4, 400),
                (209 * 4, 200),
                (210 * 4, 400),
                (212 * 4, 200),
                (213 * 4, 400),
                (215 * 4, 200),
                (216 * 4, 400),
                (236 * 4, 200),
                (240 * 4, 400),
                (346 * 4, 200),
                (347 * 4, 400),
                (403 * 4, 200),
                (407 * 4, 400),
                (415 * 4, 50),
                (415 * 4 + 1, 200),
                (424 * 4 - 3, 400),
                (438 * 4 - 3, 200),
                (439 * 4 - 3, 400),
                (479 * 4 - 3, 200),
                (483 * 4 - 3, 400),
                (491 * 4 - 3, 200),
                (492 * 4 - 3, 400),
                (503 * 4 - 3, 200),
                (503 * 4 - 1, 400),
                (536 * 4 - 5, 100),
                (536 * 4 - 3, 400),
                (567 * 4 - 7, 200),
            ]
        );

        println!("{:?}", bpm_changes.beats_layout())
    }
}
