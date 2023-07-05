use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Default)]
pub struct TimingPoint {
    time: u32,
    beat_length: f32,
}

impl TimingPoint {
    pub fn is_inherited(&self) -> bool {
        self.beat_length < 0f32
    }

    pub fn velocity(&self) -> f32 {
        if self.is_inherited() {
            100f32 / self.beat_length.abs()
        } else {
            1f32
        }
    }

    pub fn bpm(&self) -> Result<u16, String> {
        if self.is_inherited() {
            Err(String::from(
                "BPM should be calculated by uninherited timing points",
            ))
        } else {
            Ok((60000f32 / self.beat_length).round() as u16)
        }
    }
}

pub trait HitObject {
    fn time(&self) -> u32;
    fn duration_time(&self) -> u32;
    fn strong_point(&self) -> i8;
}

#[derive(Default)]
struct HitCircle {
    time: u32,
}

impl HitObject for HitCircle {
    fn time(&self) -> u32 {
        self.time
    }
    fn duration_time(&self) -> u32 {
        0
    }
    fn strong_point(&self) -> i8 {
        0
    }
}

#[derive(Default)]
struct Slider {
    time: u32,
    duration_time: u32,
    slides: u8,
}

impl HitObject for Slider {
    fn time(&self) -> u32 {
        self.time
    }
    fn duration_time(&self) -> u32 {
        self.duration_time
    }
    fn strong_point(&self) -> i8 {
        self.slides as i8
    }
}

#[derive(Default)]
struct Spinner {
    time: u32,
    duration_time: u32,
}

impl HitObject for Spinner {
    fn time(&self) -> u32 {
        self.time
    }
    fn duration_time(&self) -> u32 {
        self.duration_time
    }
    fn strong_point(&self) -> i8 {
        -1
    }
}

#[derive(Default)]
pub struct OsuMap {
    slider_multiplier: f32,
    pub timing_points: Vec<TimingPoint>,
    pub hit_objects: Vec<Box<dyn HitObject>>,
}

#[derive(Debug)]
pub struct OsuParseError {
    detail: String,
}

impl OsuParseError {
    fn new(detail: &str) -> OsuParseError {
        OsuParseError {
            detail: detail.to_owned(),
        }
    }

    fn err_ff() -> OsuParseError {
        OsuParseError::new("Wrong file format")
    }
    fn err_tp() -> OsuParseError {
        OsuParseError::new("Wrong timing point format")
    }
    fn err_ho() -> OsuParseError {
        OsuParseError::new("Wrong hit object format")
    }
    fn err_nt_fh() -> OsuParseError {
        OsuParseError::new("No corresponding timing point for hit object")
    }
}

impl Display for OsuParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.detail)
    }
}

impl Error for OsuParseError {}

const HIT_CIRCLE_BIT: u8 = 0;
const SLIDER_BIT: u8 = 1;
const NEW_COMBO_BIT: u8 = 2;
const SPINNER_BIT: u8 = 3;
const COLOR_BIT_1: u8 = 4;
const COLOR_BIT_2: u8 = 5;
const COLOR_BIT_3: u8 = 6;
const MANIA_HOLD_BIT: u8 = 7;

impl OsuMap {
    pub fn from_str(string: &str) -> Result<OsuMap, Box<dyn Error>> {
        let mut map = OsuMap::default();

        let lines: Vec<&str> = string.split('\n').map(|s| s.trim_end()).collect();

        let mut i: usize = 0;
        while i < lines.len() {
            let line = lines[i];

            if line.starts_with("SliderMultiplier") {
                let last_part = line.split(':').last();
                map.slider_multiplier = last_part.ok_or_else(OsuParseError::err_ff)?.parse()?;
            }

            if line == "[TimingPoints]" {
                i += 1;
                let mut line;
                loop {
                    let mut timing = TimingPoint::default();
                    line = lines[i];

                    if line.is_empty() {
                        break;
                    }

                    let line_split: Vec<&str> = line.split(',').collect();
                    timing.time = line_split
                        .get(0)
                        .ok_or_else(OsuParseError::err_tp)?
                        .parse()?;
                    timing.beat_length = line_split
                        .get(1)
                        .ok_or_else(OsuParseError::err_tp)?
                        .parse()?;
                    map.timing_points.push(timing);
                    i += 1;
                }
            }

            if line == "[HitObjects]" {
                i += 1;
                let mut line;
                while i < lines.len() {
                    line = lines[i];

                    if line.is_empty() {
                        break;
                    }

                    map.hit_objects
                        .push(OsuMap::parse_hit_obj_line(line, &map)?);
                    i += 1;
                }
            }

            i += 1;
        }

        Ok(map)
    }

    fn parse_hit_obj_line(line: &str, map: &OsuMap) -> Result<Box<dyn HitObject>, Box<dyn Error>> {
        let line_split: Vec<&str> = line.split(',').collect();
        let time: u32 = line_split
            .get(2)
            .ok_or_else(OsuParseError::err_ho)?
            .parse()?;
        let hit_type: u8 = line_split
            .get(3)
            .ok_or_else(OsuParseError::err_ho)?
            .parse()?;

        match hit_type {
            hit_type if (hit_type & (1 << HIT_CIRCLE_BIT)) != 0 => Ok(Box::new(HitCircle { time })),
            hit_type if (hit_type & (1 << SLIDER_BIT)) != 0 => {
                let slides: u8 = line_split
                    .get(6)
                    .ok_or_else(OsuParseError::err_ho)?
                    .parse()?;
                let length: f32 = line_split
                    .get(7)
                    .ok_or_else(OsuParseError::err_ho)?
                    .parse()?;

                let timing = map
                    .timing_points
                    .iter()
                    .rfind(|e| e.time <= time)
                    .ok_or_else(OsuParseError::err_nt_fh)?;
                let velocity = timing.velocity();
                let px_per_beat = map.slider_multiplier * 100f32 * velocity;
                let beats_num = length / px_per_beat;
                let duration = beats_num * timing.beat_length.abs() * (slides as f32);
                let duration_time = duration.round() as u32;

                Ok(Box::new(Slider {
                    time,
                    slides,
                    duration_time,
                }))
            }
            hit_type if (hit_type & (1 << SPINNER_BIT)) != 0 => {
                let end_time: u32 = line_split
                    .get(5)
                    .ok_or_else(OsuParseError::err_ho)?
                    .parse()?;

                Ok(Box::new(Spinner {
                    time,
                    duration_time: end_time - time,
                }))
            }
            _ => Err(Box::new(OsuParseError::err_ff())),
        }
    }
}
