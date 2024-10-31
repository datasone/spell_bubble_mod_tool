use std::path::Path;

use osu_file_parser::{
    HitObjects, OsuFile, TimingPoints,
    hitobjects::{HitObject, HitSound},
    osu_file::hitobjects::HitObjectParams,
    timingpoints::{Effects, SampleIndex, SampleSet, TimingPoint, Volume},
};
use rust_decimal::{
    Decimal,
    prelude::{FromPrimitive, ToPrimitive},
};

use crate::map::{BpmChanges, ScoreData, ScoreEntry};

#[derive(Debug)]
struct BpmEntry {
    /// Time **with** offset
    time: Decimal,
    bpm:  Decimal,
}

pub struct Osu {
    osu_file:  OsuFile,
    bpm_list:  Vec<BpmEntry>,
    /// Time points for entries in the map **with** offset, in milliseconds
    timecodes: Vec<Decimal>,
}

impl Osu {
    pub fn new(osu_file: &str) -> anyhow::Result<Self> {
        let osu_file = osu_file.parse::<OsuFile>()?;

        let timing_points = osu_file
            .timing_points
            .as_ref()
            .ok_or(anyhow::anyhow!("Invalid BPM"))?;
        let bpm_list = timing_points
            .0
            .iter()
            .filter(|tp| tp.uninherited())
            .map(|tp| {
                let bpm = tp.calc_bpm().ok_or(anyhow::anyhow!("Invalid BPM"))?;
                let mut offset = tp.time().clone();
                offset.try_make_decimal()?;
                let time = *offset
                    .get()
                    .as_ref()
                    .left()
                    .ok_or(anyhow::anyhow!("Invalid offset"))?;
                Ok::<BpmEntry, anyhow::Error>(BpmEntry { time, bpm })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let timecodes = Self::gen_timecodes(&bpm_list);

        Ok(Self {
            osu_file,
            bpm_list,
            timecodes,
        })
    }

    fn set_bpm_list(mut self, bpm_list: Vec<BpmEntry>) -> Self {
        self.timecodes = Self::gen_timecodes(&bpm_list);
        self.bpm_list = bpm_list;
        self
    }

    fn gen_timecodes(bpm_list: &[BpmEntry]) -> Vec<Decimal> {
        let mut timecodes = vec![];

        let mut bpm_list_iter = bpm_list.iter();
        let entry = bpm_list_iter.next().unwrap();

        let mut next_duration = TimingPoint::bpm_to_beat_duration_ms(entry.bpm);
        let mut curr_duration = next_duration;
        let mut curr_entry_time = entry.time;
        let mut curr_time = entry.time;
        timecodes.push(curr_time);

        loop {
            curr_time += curr_duration;
            if curr_time > curr_entry_time {
                match bpm_list_iter.next() {
                    None => break,
                    Some(entry) => {
                        curr_duration = next_duration;
                        next_duration = TimingPoint::bpm_to_beat_duration_ms(entry.bpm);
                        curr_entry_time = entry.time;
                    }
                }
            }
            timecodes.push(curr_time);
        }

        if curr_time - curr_entry_time < 100.into() {
            timecodes.push(curr_time);
        }

        timecodes
    }

    pub fn initial_bpm(&self) -> Decimal {
        self.bpm_list[0].bpm
    }

    pub fn offset(&self) -> Decimal {
        self.bpm_list[0].time
    }

    fn time_to_id(&self, time_ms: Decimal) -> usize {
        if time_ms > *self.timecodes.last().unwrap() {
            let last_entry = self.bpm_list.last().unwrap();
            let additional_idx =
                (time_ms - last_entry.time) / TimingPoint::bpm_to_beat_duration_ms(last_entry.bpm);
            let additional_idx = additional_idx.trunc().to_usize().unwrap();
            return self.timecodes.len() - 1 + additional_idx;
        }

        self.timecodes
            .iter()
            .position(|time| time_ms <= *time)
            .unwrap()
    }

    fn id_to_time(&self, id: usize) -> Decimal {
        match self.timecodes.get(id) {
            Some(time) => *time,
            None => {
                let last_entry = self.bpm_list.last().unwrap();
                let additional_idx = id - self.timecodes.len() + 1;
                last_entry.time
                    + Decimal::from(additional_idx)
                        * TimingPoint::bpm_to_beat_duration_ms(last_entry.bpm)
            }
        }
    }

    pub fn bpm_changes(&self) -> Option<BpmChanges> {
        if self.bpm_list.len() == 1 {
            return None;
        }

        let bpm_changes = self
            .bpm_list
            .iter()
            .map(|entry| {
                let id = self.time_to_id(entry.time) as u16;
                let bpm = entry.bpm.to_f32().unwrap();

                (id, bpm)
            })
            .collect::<Vec<_>>();
        Some(BpmChanges(bpm_changes))
    }

    pub fn score(&self) -> ScoreData {
        let hit_objs = &self.osu_file.hitobjects.as_ref().unwrap().0;

        let hit_entries = hit_objs
            .iter()
            .filter(|hit| matches!(&hit.obj_params, HitObjectParams::HitCircle))
            .map(|hit| {
                let mut time = hit.time.clone();
                time.try_make_decimal().unwrap();
                let id = self.time_to_id(*time.get().as_ref().left().unwrap());
                let entry = if hit.hitsound.finish() {
                    ScoreEntry::S
                } else {
                    ScoreEntry::O
                };
                (id, entry)
            })
            .collect::<Vec<_>>();

        let max_idx = hit_entries.iter().map(|(idx, _)| *idx).max().unwrap();
        let mut score = vec![ScoreEntry::B; max_idx + 1];

        for (idx, entry) in hit_entries {
            score[idx] = entry;
        }

        ScoreData(score)
    }

    #[allow(dead_code)]
    fn convert_from_map(
        map: &crate::map::Map,
        difficulty: crate::map::Difficulty,
        title: &str,
        artist: &str,
        id: &str,
        out_path: &Path,
    ) {
        let offset = map.song_info.offset * 1000.0;
        let initial_bpm = map.song_info.bpm;
        let initial_entry = BpmEntry {
            time: Decimal::from_f32(offset).unwrap(),
            bpm:  Decimal::from_f32(initial_bpm).unwrap(),
        };

        let mut bpm_list = vec![initial_entry];

        let bpm_changes = map.song_info.bpm_changes.as_ref().map(|bc| &bc.0);
        if let Some(bpm_changes) = bpm_changes {
            let mut last_idx = 0;
            let mut last_bpm = initial_bpm;
            let mut time = offset;
            for (idx, bpm) in bpm_changes {
                time += (*idx - last_idx) as f32 * (60_000.0 / last_bpm);
                last_idx = *idx;
                last_bpm = *bpm;

                bpm_list.push(BpmEntry {
                    time: Decimal::from_f32(time).unwrap(),
                    bpm:  Decimal::from_f32(*bpm).unwrap(),
                })
            }
        }

        let timing_points = bpm_list
            .iter()
            .map(|be| {
                TimingPoint::new_uninherited(
                    be.time.to_i32().unwrap(),
                    TimingPoint::bpm_to_beat_duration_ms(be.bpm).into(),
                    4,
                    SampleSet::BeatmapDefault,
                    SampleIndex::OsuDefaultHitsounds,
                    Volume::new(100, 14).unwrap(),
                    Effects::new(false, false),
                )
            })
            .collect::<Vec<_>>();

        let osu = Osu::new(include_str!("blank.osu")).unwrap();
        let mut osu = osu.set_bpm_list(bpm_list);
        let metadata = osu.osu_file.metadata.as_mut().unwrap();

        *metadata.artist_unicode.as_mut().unwrap() = artist.to_owned().into();
        *metadata.title_unicode.as_mut().unwrap() = title.to_owned().into();
        *metadata.title.as_mut().unwrap() = id.to_owned().into();

        let score = map.map_scores.get(&difficulty).unwrap();
        let hit_objs = score
            .scores
            .0
            .iter()
            .enumerate()
            .filter(|(_, e)| **e != ScoreEntry::B)
            .map(|(i, e)| {
                let mut hit = HitObject::hitcircle_default();
                hit.time = osu.id_to_time(i).into();

                let finish = *e == ScoreEntry::S;

                let hit_sound = HitSound::new(true, false, finish, false);
                hit.hitsound = hit_sound;
                hit
            })
            .collect::<Vec<_>>();

        osu.osu_file.timing_points = Some(TimingPoints(timing_points));
        osu.osu_file.hitobjects = Some(HitObjects(hit_objs));

        std::fs::write(out_path, osu.osu_file.to_string()).unwrap();
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
            let id = map.song_info.id.to_string();
            let title = &map.song_info.info_text.get(&Lang::JA).unwrap().title;
            let artist = &map.song_info.info_text.get(&Lang::JA).unwrap().artist;

            Osu::convert_from_map(
                &map,
                Difficulty::Hard,
                title,
                artist,
                &id,
                &PathBuf::from(format!(
                    "{}/src/external_map/{} - {} (a) [Easy].osu",
                    env!("CARGO_MANIFEST_DIR"),
                    artist,
                    title,
                )),
            )
        }
    }
}
