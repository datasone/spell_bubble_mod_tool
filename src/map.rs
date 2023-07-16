mod enums;
mod interop;

use std::{collections::HashMap, iter::zip, path::Path, str::FromStr};

use enums::{Area, Music};
use itertools::Itertools;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::{serde_as, DisplayFromStr};

use crate::map::interop::{patch_acb_file, patch_score_file, patch_share_data};

#[derive(thiserror::Error, Debug)]
pub enum InvalidMapError {
    #[error("Empty title provided in info_text")]
    EmptyTitle,
    #[error("Empty artist provided")]
    EmptyArtist,
    #[error("Unmatched BPM change info")]
    UnmatchedBPMChange,
    #[error("Empty song info text provided")]
    EmptySongInfoText,
    #[error("Empty map scores provided")]
    EmptyScores,
    #[error("Too long segments detected in map scores (Max 9), details (index, length): {0:?}")]
    TooLongSegments(Vec<(usize, usize)>),
}

#[derive(
    Eq, PartialEq, Hash, Clone, strum::Display, strum::EnumString, Serialize, Deserialize, Default,
)]
pub enum Lang {
    #[default]
    JA,
    EN,
    KO,
    Chs,
    Cht,
}

#[derive(Default, Serialize, Deserialize)]
pub struct SongInfoText {
    title:       String,
    title_kana:  String,
    sub_title:   String,
    artist:      String,
    artist2:     String,
    artist_kana: String,
    original:    String,
}

impl SongInfoText {
    fn validate(&self) -> Result<(), InvalidMapError> {
        if self.title.is_empty() {
            Err(InvalidMapError::EmptyTitle)
        } else if self.artist.is_empty() {
            Err(InvalidMapError::EmptyArtist)
        } else {
            Ok(())
        }
    }
}

/// (u16, u16) is Index, TargetBpm pair
#[derive(Default, Serialize, Deserialize)]
pub struct BpmChanges(Vec<(u16, u16)>);

impl BpmChanges {
    fn to_script(&self) -> String {
        let beats = self
            .beats_layout()
            .0
            .into_iter()
            .map(|(i, len)| format!("{i}:{len},"))
            .join("\n");

        let entry_pos = self.entry_pos();

        let bpm_changes = self
            .0
            .iter()
            .enumerate()
            .map(|(i, (_, bpm))| format!("[BPM]{}:{bpm}", entry_pos[i].0))
            .join("\n");

        format!("{}\n{}", beats, bpm_changes)
    }

    fn beats_layout(&self) -> BeatsLayout {
        let mut beats = HashMap::new();

        let mut remainder = 0;

        for (i, _) in &self.0 {
            let line = (i - remainder) / 4 + 1 + beats.len() as u16 / 2;
            let line_len = (i - remainder) % 4;
            remainder += line_len;

            if line_len != 0 {
                beats.insert(line, line_len);
                beats.insert(line + 1, 4);
            }
        }

        BeatsLayout(beats)
    }

    /// Returns (LineIdx, LinePos)
    fn entry_pos(&self) -> Vec<(u16, u16)> {
        let beats_layout = self.beats_layout();

        self.0
            .iter()
            .map(|(i, _)| *i)
            .map(|idx| {
                let mut idx = idx;
                let mut line_id = 0;
                let mut line_length = 4;

                while idx >= line_length {
                    idx -= line_length;

                    if let Some(&len) = beats_layout.0.get(&(line_id + 2)) {
                        line_length = len;
                    }

                    line_id += 1;
                }

                (line_id + 1, idx)
            })
            .collect()
    }
}

#[derive(Debug, Default)]
/// (u16, u16) is LineIdx, LineLength pair
struct BeatsLayout(HashMap<u16, u16>);

#[serde_as]
#[derive(Default, Serialize, Deserialize)]
pub struct SongInfo {
    pub id:            Music,
    pub music_file:    String,
    pub bpm:           u16,
    pub duration:      f32,
    pub offset:        f32,
    pub length:        u16,
    pub area:          Area,
    #[serde_as(as = "HashMap<DisplayFromStr, _>")]
    pub info_text:     HashMap<Lang, SongInfoText>,
    pub is_bpm_change: bool,
    pub bpm_changes:   Option<BpmChanges>,
}

impl SongInfo {
    fn validate(&self) -> Result<(), InvalidMapError> {
        for text in self.info_text.values() {
            text.validate()?
        }

        if self.is_bpm_change ^ self.bpm_changes.is_some() {
            Err(InvalidMapError::UnmatchedBPMChange)
        } else if self.info_text.is_empty() {
            Err(InvalidMapError::EmptySongInfoText)
        } else {
            Ok(())
        }
    }
}

#[derive(
    strum::Display,
    strum::EnumString,
    Eq,
    PartialEq,
    Hash,
    Copy,
    Clone,
    Debug,
    Serialize,
    Deserialize,
)]
pub enum Difficulty {
    Easy,
    Normal,
    Hard,
}

#[derive(strum::Display, strum::EnumString, Debug, Copy, Clone, PartialEq)]
pub enum ScoreEntry {
    // Normal
    O,
    // Blank (-)
    #[strum(serialize = "-")]
    B,
    // Heavy
    S,
}

#[derive(Clone)]
pub struct ScoreData(pub Vec<ScoreEntry>);

impl ScoreData {
    fn validate(&self) -> Result<(), InvalidMapError> {
        let segment_lengths = self
            .0
            .split(|&e| e == ScoreEntry::B)
            .map(|chunk| chunk.len())
            .collect::<Vec<_>>();
        if segment_lengths.iter().cloned().max().unwrap_or_default() >= 10 {
            let mut segment_indices = self
                .0
                .iter()
                .enumerate()
                .filter(|(_, &e)| e == ScoreEntry::B)
                .map(|(i, _)| i + 1)
                .collect::<Vec<_>>();
            segment_indices.insert(0, 0);
            let err_info = zip(segment_indices, segment_lengths)
                .filter(|(_, l)| *l >= 10)
                .collect::<Vec<_>>();
            Err(InvalidMapError::TooLongSegments(err_info))
        } else {
            Ok(())
        }
    }
}

impl ToString for ScoreData {
    fn to_string(&self) -> String {
        self.0.iter().map(|e| e.to_string()).collect()
    }
}

impl FromStr for ScoreData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.chars()
            .map(|c| ScoreEntry::from_str(&c.to_string()))
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
pub struct MapScore {
    pub stars:  u8,
    pub scores: ScoreData,
}

impl MapScore {
    fn default_with_len(len: usize) -> Self {
        Self {
            stars:  0,
            scores: ScoreData(vec![ScoreEntry::B; len]),
        }
    }

    fn to_script(&self, beats_layout: &BeatsLayout) -> String {
        let map_data_in_str: Vec<String> = self.scores.0.iter().map(|e| e.to_string()).collect();

        let mut map_str_chunks = Vec::new();

        let mut line_length = 4;
        let mut current_vec = Vec::new();

        let mut line_id = 0;
        let mut line_pos = 0;

        for entry_s in map_data_in_str {
            current_vec.push(entry_s);

            if line_pos < line_length - 1 {
                line_pos += 1;
            } else {
                map_str_chunks.push(current_vec.join(", ") + ",");
                current_vec = Vec::new();

                if let Some(&len) = beats_layout.0.get(&(line_id + 2)) {
                    line_length = len;
                }

                line_id += 1;
                line_pos = 0;
            }
        }

        map_str_chunks.join("\n") + " "
    }

    fn validate(&self) -> Result<(), InvalidMapError> {
        self.scores.validate()
    }

    fn find_segments(beats: &[ScoreEntry], find_blank: bool) -> Vec<(usize, usize)> {
        let mut segments: Vec<(usize, usize)> = vec![]; // (start, count)

        let mut count = 0;
        let mut start = 0;
        for (i, beat) in beats.iter().enumerate() {
            if (*beat != ScoreEntry::B) != find_blank {
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

    fn split_segments(beats: &mut [ScoreEntry], max_length: u8, ratio: f32) {
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
                    beats[start + i * (max.1 + 1)] = ScoreEntry::B;
                    i += 1;
                }

                if max.3 == 1 {
                    beats[start + length - 1] = ScoreEntry::S;
                }
            }

            if ratio != 1f32 {
                break;
            }
            segments = MapScore::find_segments(beats, false);
        }
    }

    fn fill_gap(beats: &mut [ScoreEntry], gap_length: f32, bpm: u16) {
        let blank_segments: Vec<(usize, usize)> = MapScore::find_segments(beats, true);
        let gap_length = gap_length / 60.0 * bpm as f32;
        let gap_length = gap_length.round() as usize;

        let long_blank_segments: Vec<&(usize, usize)> =
            blank_segments.iter().filter(|s| s.1 > gap_length).collect();
        for (start, length) in long_blank_segments {
            let mut i = gap_length;

            while i < start + length {
                beats[i] = ScoreEntry::O;
                i += 5;
            }
        }
    }

    fn refine_beats(beats: &mut [ScoreEntry], bpm: u16) {
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
    pub song_info:  SongInfo,
    #[serde_as(as = "HashMap<DisplayFromStr, _>")]
    pub map_scores: HashMap<Difficulty, MapScore>,
}

impl Map {
    pub fn validate(&self) -> Result<(), InvalidMapError> {
        self.song_info.validate()?;

        if self.map_scores.is_empty() {
            Err(InvalidMapError::EmptyScores)?
        }

        for score in self.map_scores.values() {
            score.validate()?
        }

        Ok(())
    }

    pub fn patch_files(
        game_files_dir: &Path,
        out_dir: &Path,
        maps: Vec<Map>,
    ) -> std::io::Result<()> {
        let mut share_data_path = game_files_dir.to_owned();
        share_data_path.push("StreamingAssets/Switch/share_data");

        let mut out_base_path = out_dir.to_owned();
        out_base_path.push("contents/0100E9D00D6C2000/romfs/Data");

        let mut out_share_data_path = out_base_path.to_owned();
        out_share_data_path.push("StreamingAssets/Switch/share_data");

        let mut scores_dir = out_base_path.clone();
        scores_dir.push("StreamingAssets/Switch/scores");

        let mut sounds_dir = out_base_path.clone();
        sounds_dir.push("StreamingAssets/Sounds");

        [scores_dir, sounds_dir]
            .iter()
            .map(std::fs::create_dir_all)
            .collect::<Result<Vec<_>, _>>()?;

        for map in &maps {
            let song_id = map.song_info.id.to_string();

            let mut acb_path = game_files_dir.to_owned();
            acb_path.push(format!(
                "StreamingAssets/Sounds/BGM_{}.acb",
                song_id.to_uppercase()
            ));

            let mut out_acb_path = out_base_path.to_owned();
            out_acb_path.push(format!(
                "StreamingAssets/Sounds/BGM_{}.acb",
                song_id.to_uppercase()
            ));

            let mut out_awb_path = out_base_path.to_owned();
            out_awb_path.push(format!(
                "StreamingAssets/Sounds/BGM_{}.awb",
                song_id.to_uppercase()
            ));

            let mut score_path = game_files_dir.to_owned();
            score_path.push(format!(
                "StreamingAssets/Switch/scores/score_{}",
                song_id.to_lowercase()
            ));

            let mut out_score_path = out_base_path.to_owned();
            out_score_path.push(format!(
                "StreamingAssets/Switch/scores/score_{}",
                song_id.to_lowercase()
            ));

            patch_acb_file(
                &map.song_info.music_file,
                &acb_path,
                &out_acb_path,
                &out_awb_path,
            )?;

            patch_score_file(
                &score_path,
                &out_score_path,
                &song_id,
                &map.map_scores,
                &map.song_info.bpm_changes,
            );
        }

        patch_share_data(&share_data_path, &out_share_data_path, &maps);

        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct MapsConfig {
    pub maps: Vec<Map>,
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
                bpm_changes:   None,
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
                bpm_changes:   BpmChanges(vec![(100, 150), (150, 50)]).into(),
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
        let bpm_changes = BpmChanges(vec![
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
        ]);

        println!("{:?}", bpm_changes.beats_layout())
    }

    #[test]
    fn test_map_score_to_script() {
        let map_score = MapScore {
            stars:  0,
            scores: ScoreData(vec![
                ScoreEntry::O,
                ScoreEntry::B,
                ScoreEntry::O,
                ScoreEntry::B,
                ScoreEntry::O,
                ScoreEntry::B,
                ScoreEntry::O,
                ScoreEntry::B,
                ScoreEntry::O,
                ScoreEntry::B,
                ScoreEntry::O,
                ScoreEntry::B,
                ScoreEntry::O,
                ScoreEntry::B,
                ScoreEntry::B,
                ScoreEntry::B,
                ScoreEntry::S,
                ScoreEntry::S,
                ScoreEntry::S,
                ScoreEntry::B,
                ScoreEntry::O,
                ScoreEntry::B,
                ScoreEntry::O,
                ScoreEntry::O,
                ScoreEntry::O,
                ScoreEntry::O,
            ]),
        };
        let beats_layout = BeatsLayout(hashmap! { 5 => 2, 6 => 4 });

        assert_eq!(
            map_score.to_script(&beats_layout),
            "O, -, O, -,\nO, -, O, -,\nO, -, O, -,\nO, -, -, -,\nS, S,\nS, -, O, -,\nO, O, O, O, "
        );
    }

    #[test]
    fn test_bpm_changes() {
        let bpm_changes = BpmChanges(vec![(1428, 100), (1430, 150)]);

        assert_eq!(
            bpm_changes.beats_layout().0,
            hashmap! { 358 => 2, 359 => 4 }
        );
        assert_eq!(bpm_changes.entry_pos(), vec![(358, 0), (359, 0)]);
    }
}
