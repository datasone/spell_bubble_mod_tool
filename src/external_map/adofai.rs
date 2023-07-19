use serde::Deserialize;

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
    bpm:    u16,
    offset: u32,
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
    bpm:        Option<u16>,
}

struct ParsedAction {
    pub id:     u16,
    pub action: ActionType,
}

enum ActionType {
    Note(ScoreEntry),
    BpmChange(u16),
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
                if self.speed_type.as_ref()? == "Bpm" {
                    ActionType::BpmChange(self.bpm?)
                } else {
                    return None;
                }
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

    pub fn bpm(&self) -> u16 {
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

    pub fn bpm_changes(&mut self) -> Vec<(u16, u16)> {
        if self.parsed_actions.is_none() {
            self.parse_actions()
        }

        self.parsed_actions
            .as_ref()
            .unwrap()
            .iter()
            .filter_map(|action| {
                if let ActionType::BpmChange(bpm) = action.action {
                    Some((action.id - 1, bpm))
                } else {
                    None
                }
            })
            .collect()
    }
}
