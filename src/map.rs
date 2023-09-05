mod enums;
mod interop;

use std::{collections::HashMap, iter::zip, path::Path, str::FromStr};

use enums::{Area, Music};
pub use interop::get_song_info;
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
#[strum(ascii_case_insensitive)]
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

    pub fn title(&self) -> String {
        if self.sub_title.is_empty() {
            self.title.clone()
        } else {
            format!("{} {}", self.title, self.sub_title)
        }
    }

    pub fn artist(&self) -> String {
        if self.artist2.is_empty() {
            self.artist.clone()
        } else {
            format!("{} {}", self.artist, self.artist2)
        }
    }

    pub fn original(&self) -> String {
        self.original.clone()
    }
}

/// (u16, u16) is Index, TargetBpm pair
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct BpmChanges(pub Vec<(u16, f32)>);

impl BpmChanges {
    fn to_script(&self) -> String {
        let beats = self
            .beats_layout()
            .0
            .into_iter()
            .sorted_by_key(|(i, _)| *i)
            .map(|(i, len)| format!("{i}:{len},"))
            .join("\n");

        let entry_pos = self.entry_pos(&None);

        let bpm_changes = self
            .0
            .iter()
            .enumerate()
            .map(|(i, (_, bpm))| format!("[BPM]{}:{bpm:.1},", entry_pos[i].0))
            .join("\n");

        format!("{}\n{}\n", beats, bpm_changes)
    }

    fn beats_layout(&self) -> BeatsLayout {
        let mut beats = HashMap::new();

        let mut remainder = 0;
        let mut added_entries = 0;

        for (i, _) in &self.0 {
            let line = (i - remainder) / 4 + 1 + added_entries;
            let line_len = (i - remainder) % 4;
            remainder += line_len;

            if line_len != 0 {
                beats.insert(line, line_len);
                beats.insert(line + 1, 4);

                added_entries += 1;
            }
        }

        if beats.is_empty() {
            return BeatsLayout::default();
        }

        let mut duplicate_keys = vec![];

        let mut beats_iter = beats.iter().sorted_by_key(|(i, _)| *i);
        let (mut last_key, _) = beats_iter.next().unwrap();
        for (key, value) in beats_iter {
            if value == beats.get(last_key).unwrap() {
                duplicate_keys.push(*key);
            }

            last_key = key;
        }

        duplicate_keys.into_iter().for_each(|k| {
            beats.remove(&k);
        });
        BeatsLayout(beats)
    }

    /// Returns (LineIdx, LinePos)
    fn entry_pos(&self, beats_layout: &Option<BeatsLayout>) -> Vec<(u16, u16)> {
        let beats_layout = match beats_layout {
            Some(bl) => bl.clone(),
            None => self.beats_layout(),
        };

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

    fn from_script(script: impl AsRef<str>) -> Option<Self> {
        let beats_layout = BeatsLayout::from_script(script.as_ref()).unwrap_or_default();

        let bpm_changes = script
            .as_ref()
            .trim()
            .lines()
            .filter(|l| l.starts_with('['))
            .map(|s| {
                let s = s
                    .strip_prefix("[BPM]")
                    .and_then(|s| s.strip_suffix(','))
                    .unwrap();
                let mut split = s.split(':');
                let line = split.next().and_then(|s| s.parse::<u16>().ok()).unwrap();
                let bpm = split.next().and_then(|s| s.parse::<f32>().ok()).unwrap();

                let mut idx = 0;
                let mut cur_line = 1;
                let mut cur_line_len = 4;
                while cur_line < line {
                    idx += cur_line_len;

                    if let Some(&new_len) = beats_layout.0.get(&(cur_line + 1)) {
                        cur_line_len = new_len;
                    }

                    cur_line += 1;
                }

                (idx, bpm)
            })
            .sorted_by_key(|(i, _)| *i)
            .collect::<Vec<_>>();

        if bpm_changes.is_empty() {
            None
        } else {
            Some(Self(bpm_changes))
        }
    }
}

#[derive(Debug, Default, Clone)]
/// (u16, u16) is LineIdx, LineLength pair
struct BeatsLayout(HashMap<u16, u16>);

impl BeatsLayout {
    fn from_script(script: impl AsRef<str>) -> Option<Self> {
        let layout = script
            .as_ref()
            .trim()
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('['))
            .map(|s| {
                let s = s.strip_suffix(',').unwrap();
                let mut split = s.split(':');
                (
                    split.next().and_then(|s| s.parse::<u16>().ok()).unwrap(),
                    split.next().and_then(|s| s.parse::<u16>().ok()).unwrap(),
                )
            })
            .collect::<HashMap<_, _>>();

        if layout.is_empty() {
            None
        } else {
            Some(Self(layout))
        }
    }
}

#[serde_as]
#[derive(Default, Serialize, Deserialize)]
pub struct SongInfo {
    pub id:            Music,
    pub music_file:    String,
    pub bpm:           f32,
    pub offset:        f32,
    pub length:        u16,
    pub area:          Area,
    #[serde_as(as = "HashMap<DisplayFromStr, _>")]
    pub info_text:     HashMap<Lang, SongInfoText>,
    pub prev_start_ms: u32,
    pub bpm_changes:   Option<BpmChanges>,
    #[serde(skip)]
    beats_layout:      Option<BeatsLayout>,
    #[serde(skip)]
    pub dlc_index:         u16,
}

impl SongInfo {
    fn validate(&self) -> Result<(), InvalidMapError> {
        for text in self.info_text.values() {
            text.validate()?
        }

        if self.info_text.is_empty() {
            Err(InvalidMapError::EmptySongInfoText)
        } else {
            Ok(())
        }
    }

    pub fn is_bpm_change(&self) -> bool {
        self.bpm_changes.is_some()
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
#[strum(ascii_case_insensitive)]
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
    pub scores: ScoreData,
}

impl MapScore {
    fn default_with_len(len: usize) -> Self {
        Self {
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

        if !current_vec.is_empty() {
            map_str_chunks.push(current_vec.join(", ") + ",");
        }

        map_str_chunks.join("\n") + " "
    }

    fn validate(&self) -> Result<(), InvalidMapError> {
        self.scores.validate()
    }

    fn from_score(score: impl AsRef<str>) -> Self {
        let score = score.as_ref().trim().lines().join("");
        let score_data = score
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| match s.chars().next().unwrap() {
                '-' => ScoreEntry::B,
                'O' => ScoreEntry::O,
                'S' => ScoreEntry::S,
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();

        Self {
            scores: ScoreData(score_data),
        }
    }
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

        let mut share_scores_dir = out_base_path.clone();
        share_scores_dir.push("StreamingAssets/Switch/share_scores");

        let mut sounds_dir = out_base_path.clone();
        sounds_dir.push("StreamingAssets/Sounds");

        [&scores_dir, &share_scores_dir, &sounds_dir]
            .iter()
            .map(std::fs::create_dir_all)
            .collect::<Result<Vec<_>, _>>()?;

        for map in &maps {
            let song_id = map.song_info.id.to_string();

            let mut acb_path = game_files_dir.to_owned();
            // The corresponding acb file was used for patching, but that causes many
            // problems (unable to play, early stop freeze, not stopping freeze), a fixed
            // DLC music is used instead now.

            // acb_path.push(format!(
            //     "StreamingAssets/Sounds/BGM_{}.acb",
            //     song_id.to_uppercase()
            // ));
            acb_path.push("StreamingAssets/Sounds/BGM_KARISUMA.acb");

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
                map.song_info.prev_start_ms,
            )?;

            patch_score_file(
                &score_path,
                &out_score_path,
                &song_id,
                &map.map_scores,
                &map.song_info.bpm_changes,
            );
        }
        // Oddly, they introduce a duplicate share_scores directory, with all score
        // files repacked remaining untouched
        let copy_opts = fs_extra::dir::CopyOptions::new()
            .overwrite(true)
            .content_only(true);
        // Users should handle file operation errors themselves
        fs_extra::dir::copy(scores_dir, share_scores_dir, &copy_opts).unwrap();

        patch_share_data(&share_data_path, &out_share_data_path, &maps);

        Ok(())
    }

    pub fn effective_bpm(&self) -> f32 {
        if self.song_info.is_bpm_change() {
            let beats_count = self.map_scores.values().next().unwrap().scores.0.len();
            beats_count as f32 / self.duration() * 60.0
        } else {
            self.song_info.bpm
        }
    }

    fn duration(&self) -> f32 {
        let score_len = self
            .map_scores
            .get(&Difficulty::Hard)
            .unwrap()
            .scores
            .0
            .len();
        let init_bpm = self.song_info.bpm;

        match &self.song_info.bpm_changes {
            Some(bpm_changes) => {
                let mut duration_sum = 0.0;

                let (first_id, _) = bpm_changes.0.first().unwrap();
                duration_sum += (first_id + 1) as f32 / init_bpm * 60.0;

                bpm_changes.0.windows(2).for_each(|w| {
                    let (left_id, left_bpm) = w[0];
                    let (right_id, _) = w[1];

                    duration_sum += (right_id - left_id) as f32 / left_bpm * 60.0;
                });

                let (last_id, last_bpm) = bpm_changes.0.last().unwrap();
                duration_sum += (score_len as u16 - *last_id - 1) as f32 / *last_bpm * 60.0;

                duration_sum
            }
            None => (score_len - 1) as f32 / init_bpm * 60.0,
        }
    }

    pub fn levels(&self) -> (u8, u8, u8) {
        (
            self.level(Difficulty::Easy, None),
            self.level(Difficulty::Normal, None),
            self.level(Difficulty::Hard, None),
        )
    }

    pub fn level(&self, difficulty: Difficulty, score_str: Option<&str>) -> u8 {
        let calculated_score;
        let score = match score_str {
            Some(score) => score,
            None => {
                if let Some(score) = self.map_scores.get(&difficulty) {
                    calculated_score = score.to_script(
                        &self
                            .song_info
                            .bpm_changes
                            .as_ref()
                            .map(|bc| bc.beats_layout())
                            .unwrap_or_default(),
                    );
                    &calculated_score
                } else {
                    return 0
                }
            }
        };

        let default_bpm_changes = BpmChanges::default();
        let bpm_changes = &self.song_info.bpm_changes;
        let bpm_changes_entries = bpm_changes
            .as_ref()
            .map(|bc| bc.entry_pos(&self.song_info.beats_layout))
            .unwrap_or_default();
        let mut bpm_line_changes = zip(
            bpm_changes
                .as_ref()
                .unwrap_or(&default_bpm_changes)
                .0
                .iter(),
            bpm_changes_entries.iter(),
        )
        .map(|((_, bpm), (line, _))| (*line, *bpm));

        // u16::MAX is used to form an (impossible to match) change point
        let mut next_bpm_change = bpm_line_changes.next().unwrap_or((u16::MAX, 0.));
        let mut tracked_bpm = self.song_info.bpm;

        let lines_beat_and_duration = score
            .lines()
            .enumerate()
            .map(|(i, l)| {
                let line_beats = l
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>();
                let line_beat_counts = line_beats.iter().filter(|&&s| s != "-").count();

                let line_duration = if i + 1 == next_bpm_change.0 as usize {
                    let duration = 60.0 / tracked_bpm;

                    tracked_bpm = next_bpm_change.1;
                    next_bpm_change = bpm_line_changes.next().unwrap_or((u16::MAX, 0.));

                    duration + (line_beats.len() - 1) as f32 * 60.0 / tracked_bpm
                } else {
                    line_beats.len() as f32 * 60.0 / tracked_bpm
                };

                (line_beat_counts, line_duration)
            })
            .collect::<Vec<_>>();

        let densities = lines_beat_and_duration
            .windows(8)
            .map(|w| {
                let (beat_counts, durations): (Vec<_>, Vec<_>) = w.iter().copied().unzip();
                beat_counts.into_iter().sum::<usize>() as f32 / durations.into_iter().sum::<f32>()
            })
            .sorted_by(|a, b| a.partial_cmp(b).unwrap())
            .collect::<Vec<_>>();

        let take_len = (densities.len() - 1) / 5;
        let take_from = densities.len() - take_len - 1;
        let level: f32 = densities[take_from..].iter().sum();
        let level = ((level / take_len as f32) - 1.0) * 4.2;
        level.ceil() as u8
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
                bpm:           150.0,
                offset:        0.01,
                length:        1500,
                dlc_index:     0,
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
                bpm_changes:   None,
                beats_layout:  None,
                prev_start_ms: 0,
            },
            map_scores: hashmap! {
                Difficulty::Hard => MapScore {
                    scores: ScoreData::from_str("SO-SO-SO-SO-SO----SOS-OO").unwrap(),
                }
            },
        };

        let map2 = Map {
            song_info:  SongInfo {
                id:            Music::Alice,
                music_file:    "file_path2".to_string(),
                bpm:           152.0,
                offset:        0.02,
                length:        1502,
                dlc_index:     0,
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
                bpm_changes:   BpmChanges(vec![(100, 150.), (150, 50.)]).into(),
                beats_layout:  None,
                prev_start_ms: 0,
            },
            map_scores: hashmap! {
                Difficulty::Hard => MapScore {
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
            (118 * 4, 200.),
            (130 * 4, 400.),
            (206 * 4, 200.),
            (207 * 4, 400.),
            (209 * 4, 200.),
            (210 * 4, 400.),
            (212 * 4, 200.),
            (213 * 4, 400.),
            (215 * 4, 200.),
            (216 * 4, 400.),
            (236 * 4, 200.),
            (240 * 4, 400.),
            (346 * 4, 200.),
            (347 * 4, 400.),
            (403 * 4, 200.),
            (407 * 4, 400.),
            (415 * 4, 50.),
            (415 * 4 + 1, 200.),
            (424 * 4 - 3, 400.),
            (438 * 4 - 3, 200.),
            (439 * 4 - 3, 400.),
            (479 * 4 - 3, 200.),
            (483 * 4 - 3, 400.),
            (491 * 4 - 3, 200.),
            (492 * 4 - 3, 400.),
            (503 * 4 - 3, 200.),
            (503 * 4 - 1, 400.),
            (536 * 4 - 5, 100.),
            (536 * 4 - 3, 400.),
            (567 * 4 - 7, 200.),
        ]);

        println!("{:?}", bpm_changes.beats_layout())
    }

    #[test]
    fn test_map_score_to_script() {
        let map_score = MapScore {
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
        let bpm_changes = BpmChanges(vec![(1428, 100.), (1430, 150.)]);

        assert_eq!(
            bpm_changes.beats_layout().0,
            hashmap! { 358 => 2, 359 => 4 }
        );
        assert_eq!(bpm_changes.entry_pos(&None), vec![(358, 0), (359, 0)]);
    }
}
