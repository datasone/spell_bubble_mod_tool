use std::path::Path;

use serde::Deserialize;
use serde_json::json;

use crate::map::ScoreEntry;

#[derive(Deserialize)]
pub struct ADoFaIMap {
    #[serde(alias = "angleData")]
    angle_data:     Vec<u16>,
    settings:       MapSettings,
    actions:        Vec<MapAction>,
    #[serde(skip_deserializing)]
    parsed_actions: Option<Vec<ParsedAction>>,
}

#[derive(Deserialize)]
struct MapSettings {
    bpm:    f32,
    offset: i32,
}

#[derive(Deserialize)]
struct MapAction {
    floor:      u16,
    #[serde(alias = "eventType")]
    event_type: Option<String>,
    #[serde(alias = "hitsound")]
    hit_sound:  Option<String>,
    #[serde(alias = "speedType")]
    speed_type: Option<String>,
    #[serde(alias = "beatsPerMinute")]
    bpm:        Option<f32>,
    #[serde(alias = "bpmMultiplier")]
    multiplier: Option<f32>,
}

struct ParsedAction {
    pub id:     u16,
    pub action: ActionType,
}

enum ActionType {
    Note(ScoreEntry),
    BpmChange(BpmChangeType),
}

enum BpmChangeType {
    Exact(f32),
    Multiplier(f32),
}

impl MapAction {
    fn to_parsed(&self) -> Option<ParsedAction> {
        let action = match self.event_type.as_ref()?.as_str() {
            "PlaySound" => {
                let entry = match self.hit_sound.as_ref()?.as_str() {
                    "Hat" => ScoreEntry::O,
                    "Hammer" => ScoreEntry::S,
                    _ => return None,
                };

                ActionType::Note(entry)
            }
            "SetSpeed" => {
                let change = match self.speed_type.as_ref()?.as_str() {
                    "Bpm" => BpmChangeType::Exact(self.bpm?),
                    "Multiplier" => BpmChangeType::Multiplier(self.multiplier?),
                    _ => return None,
                };

                ActionType::BpmChange(change)
            }
            _ => return None,
        };

        ParsedAction {
            id: self.floor,
            action,
        }
        .into()
    }
}

impl ADoFaIMap {
    fn parse_actions(&mut self) {
        self.parsed_actions = self
            .actions
            .iter()
            .filter_map(|a| a.to_parsed())
            .collect::<Vec<_>>()
            .into();
    }

    pub fn length(&self) -> usize {
        self.angle_data.len()
    }

    pub fn bpm(&self) -> f32 {
        self.settings.bpm
    }

    pub fn offset(&self) -> f32 {
        self.settings.offset as f32 / 1000.0
    }

    pub fn scores(&mut self) -> Vec<ScoreEntry> {
        if self.parsed_actions.is_none() {
            self.parse_actions()
        }

        let mut scores = vec![ScoreEntry::B; self.length()];

        self.parsed_actions
            .as_ref()
            .unwrap()
            .iter()
            .for_each(|action| {
                if let ActionType::Note(e) = action.action {
                    scores[action.id as usize - 1] = e
                }
            });

        scores
    }

    pub fn bpm_changes(&mut self) -> Vec<(u16, f32)> {
        if self.parsed_actions.is_none() {
            self.parse_actions()
        }

        let mut tracked_bpm = self.settings.bpm;

        self.parsed_actions
            .as_ref()
            .unwrap()
            .iter()
            .filter_map(|action| match action.action {
                ActionType::BpmChange(BpmChangeType::Exact(bpm)) => {
                    tracked_bpm = bpm;
                    Some((action.id - 1, tracked_bpm))
                }
                ActionType::BpmChange(BpmChangeType::Multiplier(mul)) => {
                    tracked_bpm *= mul;
                    Some((action.id - 1, tracked_bpm))
                }
                _ => None,
            })
            .collect()
    }

    // This function is only intended to be used by tests for debug purposes
    #[allow(dead_code)]
    fn convert_from_map(
        map: &crate::map::Map,
        difficulty: crate::map::Difficulty,
        out_path: &Path,
    ) -> anyhow::Result<()> {
        let template_json = include_str!("template.adofai");
        let mut template_json: serde_json::Value =
            serde_json::from_str(template_json.trim_start_matches('\u{feff}')).unwrap();

        let score_len = map.map_scores.get(&difficulty).unwrap().scores.0.len();
        let angle_data = vec![0.into(); score_len];
        *template_json
            .pointer_mut("/angleData")
            .unwrap()
            .as_array_mut()
            .unwrap() = angle_data;

        let bpm = map.song_info.bpm;
        *template_json.pointer_mut("/settings/bpm").unwrap() = bpm.into();

        let offset = map.song_info.offset;
        let offset = (offset * 1000.0) as i64;
        *template_json.pointer_mut("/settings/offset").unwrap() = offset.into();

        let base_note_event = json!(
            {
                "floor": 0,
                "eventType": "PlaySound",
                "hitsound": "Hat",
                "hitsoundVolume": 100,
                "angleOffset": 0,
                "eventTag": ""
            }
        );

        let mut actions = map
            .map_scores
            .get(&difficulty)
            .unwrap()
            .scores
            .0
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match e {
                ScoreEntry::O => {
                    let mut note_event = base_note_event.clone();
                    *note_event.pointer_mut("/floor").unwrap() = (i + 1).into();
                    Some(note_event)
                }
                ScoreEntry::B => None,
                ScoreEntry::S => {
                    let mut note_event = base_note_event.clone();
                    *note_event.pointer_mut("/floor").unwrap() = (i + 1).into();
                    *note_event.pointer_mut("/hitsound").unwrap() = "Hammer".into();
                    Some(note_event)
                }
            })
            .collect::<Vec<_>>();

        if let Some(changes) = map.song_info.bpm_changes.as_ref() {
            let base_bpm_change_event = json!(
                {
                    "floor": 0,
                    "eventType": "SetSpeed",
                    "speedType": "Bpm",
                    "beatsPerMinute": 0,
                    "bpmMultiplier": 1,
                    "angleOffset": 0
                }
            );

            let changes = changes
                .0
                .iter()
                .map(|(i, bpm)| {
                    let mut bpm_change_event = base_bpm_change_event.clone();
                    *bpm_change_event.pointer_mut("/floor").unwrap() = (i + 1).into();
                    *bpm_change_event.pointer_mut("/beatsPerMinute").unwrap() = (*bpm).into();
                    bpm_change_event
                })
                .collect::<Vec<_>>();

            actions.extend(changes);
            actions.sort_by_key(|v| v.pointer("/floor").unwrap().as_u64());
        }

        *template_json
            .pointer_mut("/actions")
            .unwrap()
            .as_array_mut()
            .unwrap() = actions;
        std::fs::write(out_path, serde_json::to_string_pretty(&template_json)?)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::map::{Difficulty, Lang};

    #[test]
    fn test_conversion() {
        let maps_config = std::fs::read_to_string(format!(
            "{}/src/external_map/test.toml",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let config: crate::map::MapsConfig = toml::from_str(&maps_config).unwrap();

        for map in config.maps {
            ADoFaIMap::convert_from_map(
                &map,
                Difficulty::Hard,
                &PathBuf::from(format!(
                    "{}/src/external_map/{}.adofai",
                    env!("CARGO_MANIFEST_DIR"),
                    map.song_info.info_text.get(&Lang::JA).unwrap().title()
                )),
            )
            .unwrap();
        }
    }
}
