use std::{
    cell::RefCell,
    cmp::Ordering,
    collections::HashMap,
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    rc::Rc,
    str::FromStr,
};

use itertools::Itertools;
use maplit::hashmap;
use rust_decimal::prelude::ToPrimitive;
use slint::{Model, ModelRc, SharedString, StandardListViewItem, VecModel};

use crate::{
    exefs,
    map::{Area, BpmChanges, Difficulty::*, Lang, Lang::*, Map, MusicID, SongInfo, SongInfoText},
    song_info::get_song_info,
};

slint::include_modules!();

pub fn start_gui() -> anyhow::Result<()> {
    slint::init_translations!(concat!(env!("CARGO_MANIFEST_DIR"), "/ui/lang/"));

    let main_window = MainWindow::new()?;
    main_window.on_prompt_get_path(|| {
        let path = rfd::FileDialog::new()
            .set_title("Select root of dumped RomFS (the Data folder)")
            .pick_folder();
        let path = path
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        path.into()
    });

    init_utilities(&main_window);
    init_song_info_adapter(&main_window);
    init_custom_map_adapter(&main_window);
    init_custom_map_model(&main_window);

    main_window.run()?;
    Ok(())
}

fn init_utilities(main_window: &MainWindow) {
    main_window
        .global::<Utilities>()
        .on_is_empty(|str| str.is_empty());

    main_window
        .global::<Utilities>()
        .on_length(|str| str.len() as i32);
}

fn init_song_info_adapter(main_window: &MainWindow) {
    let main_window = main_window.as_weak();

    let row_data = Rc::new(VecModel::default());

    main_window
        .unwrap()
        .global::<SongInfoAdapter>()
        .on_load_data({
            let main_window = main_window.clone();
            let row_data = row_data.clone();
            move |lang_id| {
                let row_data = row_data.clone();

                let lang = match lang_id {
                    0 => JA,
                    1 => Chs,
                    2 => Cht,
                    3 => EN,
                    4 => KO,
                    _ => unreachable!(),
                };

                let path = main_window.unwrap().global::<SongInfoAdapter>().get_path();
                if path.is_empty() {
                    return;
                }

                let romfs_root = Path::new(path.as_str());
                let infos = get_song_info(romfs_root);

                let row_models = infos
                    .maps
                    .into_iter()
                    .map(|map_info| {
                        let song_info = &map_info.map.song_info;
                        let info_text = song_info.info_text.get(&lang).unwrap();

                        let row_items = [
                            song_info.id.to_string(),
                            info_text.title(),
                            info_text.artist(),
                            info_text.original(),
                            map_info.map.effective_bpm().to_string(),
                            song_info.is_bpm_change().to_string(),
                            map_info
                                .map
                                .level(Easy, Some(&map_info.score_e))
                                .to_string(),
                            map_info
                                .map
                                .level(Normal, Some(&map_info.score_n))
                                .to_string(),
                            map_info
                                .map
                                .level(Hard, Some(&map_info.score_h))
                                .to_string(),
                            song_info.length.to_string(),
                            song_info.area.to_string(),
                            if song_info.dlc_index == 0 {
                                "本体"
                            } else {
                                &infos.dlcs[song_info.dlc_index as usize - 1]
                            }
                            .to_string(),
                        ]
                        .into_iter()
                        .map(|item| StandardListViewItem::from(item.as_ref()))
                        .collect::<Vec<_>>();

                        ModelRc::new(VecModel::from(row_items))
                    })
                    .collect::<Vec<_>>();

                row_data.set_vec(row_models);

                main_window
                    .unwrap()
                    .global::<SongInfoAdapter>()
                    .set_row_data(row_data.into());
            }
        });

    main_window
        .unwrap()
        .global::<SongInfoAdapter>()
        .on_generate_csv({
            let row_data = row_data.clone();
            move || {
                let row_data = row_data.clone();

                let path = rfd::FileDialog::new()
                    .set_title("Path of output CSV")
                    .add_filter("CSV File", &["csv"])
                    .save_file();

                let Some(path) = path else { return };

                let mut writer = BufWriter::new(File::create(path).unwrap());
                if cfg!(windows) {
                    // Write BOM for Windows programs to recognize encoding
                    writer.write_all(&[0xEF, 0xBB, 0xBF]).unwrap();
                }
                let mut writer = csv::Writer::from_writer(writer);

                writer
                    .write_record([
                        "ID",
                        "Title",
                        "Artist",
                        "Original",
                        "Effective BPM",
                        "Has Tempo Changes",
                        "Levels - Easy",
                        "Levels - Normal",
                        "Levels - Hard",
                        "Length",
                        "Area",
                        "DLC",
                    ])
                    .unwrap();

                for row in row_data.iter() {
                    let row_text = row.iter().map(|item| item.text).collect::<Vec<_>>();
                    let row_strs = row_text.iter().map(|text| text.as_str());
                    writer.write_record(row_strs).unwrap();
                }
            }
        });

    main_window
        .unwrap()
        .global::<SongInfoAdapter>()
        .on_sort_ascending({
            let main_window = main_window.clone();
            let row_data = row_data.clone();

            move |index| {
                let row_data = row_data.clone();

                let sort_model = Rc::new(row_data.sort_by(move |r_a, r_b| {
                    let c_a = r_a.row_data(index as usize).unwrap();
                    let c_b = r_b.row_data(index as usize).unwrap();

                    c_a.text.cmp(&c_b.text)
                }));

                main_window
                    .unwrap()
                    .global::<SongInfoAdapter>()
                    .set_row_data(sort_model.into());
            }
        });

    main_window
        .unwrap()
        .global::<SongInfoAdapter>()
        .on_sort_descending({
            let main_window = main_window.clone();
            let row_data = row_data.clone();

            move |index| {
                let row_data = row_data.clone();

                let sort_model = Rc::new(row_data.sort_by(move |r_a, r_b| {
                    let c_a = r_a.row_data(index as usize).unwrap();
                    let c_b = r_b.row_data(index as usize).unwrap();

                    c_b.text.cmp(&c_a.text)
                }));

                main_window
                    .unwrap()
                    .global::<SongInfoAdapter>()
                    .set_row_data(sort_model.into());
            }
        });
}

macro_rules! obtain_text_field {
    ($text:expr, $field:ident) => {{
        $text
            .iter()
            .map(|t| t.$field)
            .filter(|s| !s.is_empty())
            .next()
            .unwrap_or_default()
    }};
}

#[derive(PartialEq)]
enum MapInfoSortKey {
    String(SharedString),
    Int(i32),
    Float(f32),
}

impl PartialOrd for MapInfoSortKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Self::String(s_s), Self::String(s_o)) => s_s.partial_cmp(s_o),
            (Self::Int(i_s), Self::Int(i_o)) => i_s.partial_cmp(i_o),
            (Self::Float(f_s), Self::Float(f_o)) => f_s.partial_cmp(f_o),
            _ => None,
        }
    }
}

fn get_key_by_column(index: i32, map_model: &MapInfo) -> MapInfoSortKey {
    match index {
        0 => MapInfoSortKey::String(map_model.id.to_owned()),
        1 => MapInfoSortKey::String(obtain_text_field!(map_model.info_text, title).to_owned()),
        2 => MapInfoSortKey::String(obtain_text_field!(map_model.info_text, artist).to_owned()),
        3 => MapInfoSortKey::String(obtain_text_field!(map_model.info_text, original).to_owned()),
        4 => MapInfoSortKey::Float(map_model.bpm),
        5 => MapInfoSortKey::String(SharedString::from(format!(
            "{}",
            Area::from(AreaModel {
                area_idx:   map_model.area_idx,
                area_night: map_model.area_night,
            })
        ))),
        6 => MapInfoSortKey::Int(map_model.level),
        7 => MapInfoSortKey::String(map_model.music_file.to_owned()),
        8 => MapInfoSortKey::Int(map_model.prev_start_ms),
        _ => unreachable!(),
    }
}

fn init_custom_map_adapter(main_window: &MainWindow) {
    let main_window = main_window.as_weak();

    main_window
        .unwrap()
        .global::<CustomMapAdapter>()
        .on_to_row_data({
            |map| {
                let id = map.id;
                let title = obtain_text_field!(map.info_text, title);
                let artist = obtain_text_field!(map.info_text, artist);
                let original = obtain_text_field!(map.info_text, original);
                let bpm: SharedString = map.bpm.to_string().into();
                let area: SharedString = format!(
                    "{}",
                    Area::from(AreaModel {
                        area_idx:   map.area_idx,
                        area_night: map.area_night,
                    })
                )
                .into();
                let level: SharedString = map.level.to_string().into();
                let music_file = map.music_file;
                let preview_start: SharedString = map.prev_start_ms.to_string().into();

                let row = vec![
                    id,
                    title,
                    artist,
                    original,
                    bpm,
                    area,
                    level,
                    music_file,
                    preview_start,
                ]
                .into_iter()
                .map(StandardListViewItem::from)
                .collect::<Vec<_>>();
                ModelRc::new(VecModel::from(row))
            }
        });

    main_window
        .unwrap()
        .global::<CustomMapAdapter>()
        .on_generate_row_data({
            let main_window = main_window.clone();
            move |maps| {
                let row_data = maps
                    .iter()
                    .map(|m| {
                        main_window
                            .unwrap()
                            .global::<CustomMapAdapter>()
                            .invoke_to_row_data(m)
                    })
                    .collect::<Vec<_>>();

                ModelRc::new(VecModel::from(row_data))
            }
        });

    main_window
        .unwrap()
        .global::<CustomMapAdapter>()
        .on_update_row_data({
            let main_window = main_window.clone();
            move || {
                let maps_model = main_window.unwrap().global::<CustomMapAdapter>().get_maps();
                let row_data = main_window
                    .unwrap()
                    .global::<CustomMapAdapter>()
                    .invoke_generate_row_data(maps_model);
                main_window
                    .unwrap()
                    .global::<CustomMapAdapter>()
                    .set_row_data(row_data);
            }
        });

    let maps = load_local_config().unwrap_or_default();
    let maps = Rc::new(RefCell::new(maps));

    let maps_model = maps
        .borrow()
        .iter()
        .sorted_by_key(|(k, _)| *k)
        .map(|(_, m)| MapInfo::from(m))
        .collect::<Vec<_>>();
    let maps_model: Rc<VecModel<MapInfo>> = Rc::new(VecModel::from(maps_model));

    {
        let maps_model = maps_model.clone();
        main_window
            .unwrap()
            .global::<CustomMapAdapter>()
            .set_maps(maps_model.into());

        main_window
            .unwrap()
            .global::<CustomMapAdapter>()
            .invoke_update_row_data();
    }

    main_window
        .unwrap()
        .global::<CustomMapAdapter>()
        .on_sort_ascending({
            let main_window = main_window.clone();
            let maps_model = maps_model.clone();

            move |index| {
                let maps_model = maps_model.clone();
                let sort_model = Rc::new(maps_model.sort_by(move |a, b| {
                    let k_a = get_key_by_column(index, a);
                    let k_b = get_key_by_column(index, b);

                    k_a.partial_cmp(&k_b).unwrap()
                }));

                main_window
                    .unwrap()
                    .global::<CustomMapAdapter>()
                    .set_maps(sort_model.into());

                main_window
                    .unwrap()
                    .global::<CustomMapAdapter>()
                    .invoke_update_row_data();
            }
        });

    main_window
        .unwrap()
        .global::<CustomMapAdapter>()
        .on_sort_descending({
            let main_window = main_window.clone();
            let maps_model = maps_model.clone();

            move |index| {
                let maps_model = maps_model.clone();
                let sort_model = Rc::new(maps_model.sort_by(move |a, b| {
                    let k_a = get_key_by_column(index, a);
                    let k_b = get_key_by_column(index, b);

                    k_b.partial_cmp(&k_a).unwrap()
                }));

                main_window
                    .unwrap()
                    .global::<CustomMapAdapter>()
                    .set_maps(sort_model.into());

                main_window
                    .unwrap()
                    .global::<CustomMapAdapter>()
                    .invoke_update_row_data();
            }
        });

    main_window
        .unwrap()
        .global::<CustomMapAdapter>()
        .on_can_add_map(|maps| !maps.iter().any(|m| m.id.is_empty()));

    main_window
        .unwrap()
        .global::<CustomMapAdapter>()
        .on_add_map({
            let main_window = main_window.clone();
            let maps = maps.clone();
            let maps_model = maps_model.clone();

            move || {
                let maps_model = maps_model.clone();
                let map = main_window
                    .unwrap()
                    .global::<CustomMapModel>()
                    .invoke_create_map();

                maps_model.push(map);
                maps.borrow_mut().insert(String::new(), Map::default());

                main_window
                    .unwrap()
                    .global::<CustomMapAdapter>()
                    .set_maps(maps_model.into());

                main_window
                    .unwrap()
                    .global::<CustomMapAdapter>()
                    .invoke_update_row_data();
            }
        });

    main_window
        .unwrap()
        .global::<CustomMapAdapter>()
        .on_delete_map({
            let main_window = main_window.clone();
            let maps_model = maps_model.clone();
            let maps = maps.clone();

            move || {
                let maps_model = maps_model.clone();
                let map_model = main_window
                    .unwrap()
                    .global::<CustomMapAdapter>()
                    .invoke_get_selected_map();

                let map_id = map_model.id.as_str().to_owned();
                let model_idx = maps_model.iter().position(|m| m == map_model).unwrap();
                maps_model.remove(model_idx);
                maps.borrow_mut().remove(&map_id);

                save_local_config(&maps.borrow());

                main_window
                    .unwrap()
                    .global::<CustomMapAdapter>()
                    .set_maps(maps_model.into());

                main_window
                    .unwrap()
                    .global::<CustomMapAdapter>()
                    .invoke_update_row_data();
            }
        });

    main_window
        .unwrap()
        .global::<CustomMapAdapter>()
        .on_get_selected_map({
            let main_window = main_window.clone();

            move || {
                let idx = main_window
                    .unwrap()
                    .global::<CustomMapAdapter>()
                    .get_current_row();

                main_window
                    .unwrap()
                    .global::<CustomMapAdapter>()
                    .get_maps()
                    .row_data(idx as usize)
                    .unwrap()
            }
        });

    main_window
        .unwrap()
        .global::<CustomMapAdapter>()
        .on_update_selected_map({
            let main_window = main_window.clone();
            let maps = maps.clone();
            let maps_model = maps_model.clone();

            move |map_model| {
                let maps_model = maps_model.clone();

                let old_map = main_window
                    .unwrap()
                    .global::<CustomMapAdapter>()
                    .invoke_get_selected_map();

                let model_idx = maps_model.iter().position(|m| m.id == old_map.id).unwrap();
                let old_map = maps_model.remove(model_idx);
                let _old_map = maps.borrow_mut().remove(old_map.id.as_str()).unwrap();

                let mut new_id = map_model.id.as_str().to_owned();

                let mut append_idx = 1;
                while maps.borrow().contains_key(&new_id) {
                    new_id = format!("{new_id}{append_idx}");
                    append_idx += 1;
                }

                let mut map_model = map_model;
                map_model.id = new_id.clone().into();

                let map = Map::from(&map_model);
                map_model.level = map.level(Hard, None) as i32;

                maps.borrow_mut().insert(new_id, map);
                maps_model.insert(model_idx, map_model);

                save_local_config(&maps.borrow());

                main_window
                    .unwrap()
                    .global::<CustomMapAdapter>()
                    .set_maps(maps_model.into());

                main_window
                    .unwrap()
                    .global::<CustomMapAdapter>()
                    .invoke_update_row_data();
            }
        });

    main_window
        .unwrap()
        .global::<CustomMapAdapter>()
        .on_import_from_file({
            let main_window = main_window.clone();
            let maps = maps.clone();
            let maps_model = maps_model.clone();

            move || {
                let maps_model = maps_model.clone();
                let file = rfd::FileDialog::new()
                    .set_title("Maps config toml")
                    .add_filter("Config file", &["toml"])
                    .pick_file();
                if let Some(file) = file {
                    if let Ok(new_maps) = load_config(&file) {
                        let mut new_maps = new_maps.into_values().collect::<Vec<_>>();

                        for map in new_maps.iter_mut() {
                            let mut id = 1;
                            let music_id = map.song_info.id.to_string();
                            while maps.borrow().contains_key(&music_id) {
                                map.song_info.id = MusicID::New(format!("{music_id}{id}"));
                                id += 1;
                            }
                        }

                        let new_map_models = new_maps.iter().map(MapInfo::from);
                        maps_model.extend(new_map_models);
                        maps.borrow_mut().extend(
                            new_maps
                                .into_iter()
                                .map(|m| (m.song_info.id.to_string(), m)),
                        );

                        save_local_config(&maps.borrow());

                        main_window
                            .unwrap()
                            .global::<CustomMapAdapter>()
                            .set_maps(maps_model.into());

                        main_window
                            .unwrap()
                            .global::<CustomMapAdapter>()
                            .invoke_update_row_data();
                    }
                }
            }
        });

    main_window
        .unwrap()
        .global::<CustomMapAdapter>()
        .on_export_to_file({
            let maps = maps.clone();

            move || {
                let file = rfd::FileDialog::new()
                    .set_title("Maps config toml")
                    .add_filter("Config file", &["toml"])
                    .save_file();

                if let Some(file) = file {
                    save_config(&maps.borrow(), &file);
                }
            }
        });

    main_window
        .unwrap()
        .global::<CustomMapAdapter>()
        .on_generate_mod({
            let main_window = main_window.clone();
            let maps = maps.clone();

            move || {
                let out_dir = rfd::FileDialog::new()
                    .set_title("Mod output path")
                    .pick_folder();

                if let Some(out_dir) = out_dir {
                    let romfs_root = main_window
                        .unwrap()
                        .global::<CustomMapAdapter>()
                        .get_romfs_path();
                    let romfs_root = Path::new(romfs_root.as_str());
                    let exefs_root = main_window
                        .unwrap()
                        .global::<CustomMapAdapter>()
                        .get_exefs_path();
                    let mut main_exe_path = PathBuf::from(exefs_root.as_str());
                    main_exe_path.push("main");

                    let maps = maps.borrow();
                    let names = maps
                        .values()
                        .map(|m| m.song_info.id.to_string())
                        .collect::<Vec<_>>();

                    let _ = Map::patch_files(romfs_root, &out_dir, maps.values(), false);
                    exefs::patch_files(romfs_root, &main_exe_path, &out_dir, &names);
                }
            }
        })
}

fn local_config_path() -> Option<PathBuf> {
    let mut path = dirs::config_local_dir()?;
    path.push("spell_bubble_mod_tool");
    path.push("maps.toml");
    Some(path)
}

fn load_local_config() -> anyhow::Result<HashMap<String, Map>> {
    load_config(&local_config_path().ok_or(anyhow::anyhow!(""))?)
}

fn load_config(path: &Path) -> anyhow::Result<HashMap<String, Map>> {
    let maps: crate::map::MapsConfig = {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content)?
    };

    for map in maps.maps.iter() {
        map.validate(false)?
    }

    Ok(maps
        .maps
        .into_iter()
        .map(|m| (m.song_info.id.to_string(), m))
        .collect())
}

fn save_local_config(maps: &HashMap<String, Map>) {
    if let Some(local_config) = local_config_path() {
        save_config(maps, &local_config)
    }
}

fn save_config(maps: &HashMap<String, Map>, path: &Path) {
    let maps_config = crate::map::MapsConfig {
        maps: maps.values().cloned().collect(),
    };

    let mut config_path = path.to_owned();
    config_path.pop();

    let _ = std::fs::create_dir_all(config_path);
    let _ = std::fs::write(path, toml::to_string_pretty(&maps_config).unwrap());
}

struct AreaModel {
    area_idx:   i32,
    area_night: bool,
}

impl From<Area> for AreaModel {
    fn from(area: Area) -> Self {
        match area {
            Area::Arena => AreaModel {
                area_idx:   0,
                area_night: false,
            },
            Area::ArenaNight => AreaModel {
                area_idx:   0,
                area_night: true,
            },
            Area::HakugyokuRo => AreaModel {
                area_idx:   1,
                area_night: false,
            },
            Area::HakureiJinjya => AreaModel {
                area_idx:   2,
                area_night: false,
            },
            Area::HakureiJinjyaNight => AreaModel {
                area_idx:   2,
                area_night: true,
            },
            Area::KiriNoMizuumi => AreaModel {
                area_idx:   3,
                area_night: false,
            },
            Area::KiriNoMizuumiNight => AreaModel {
                area_idx:   3,
                area_night: true,
            },
            Area::KoumaKan => AreaModel {
                area_idx:   4,
                area_night: false,
            },
            Area::MahouNoMori => AreaModel {
                area_idx:   5,
                area_night: false,
            },
            Area::MayoiNoTikurin => AreaModel {
                area_idx:   6,
                area_night: false,
            },
            Area::MoriyaJinjya => AreaModel {
                area_idx:   7,
                area_night: false,
            },
            Area::TireiDen => AreaModel {
                area_idx:   8,
                area_night: false,
            },
            Area::YoukaiNoYama => AreaModel {
                area_idx:   9,
                area_night: false,
            },
            Area::YoukaiNoYamaNight => AreaModel {
                area_idx:   9,
                area_night: true,
            },
            _ => unreachable!(),
        }
    }
}

impl From<AreaModel> for Area {
    fn from(area: AreaModel) -> Self {
        match area.area_idx {
            0 => {
                if area.area_night {
                    Area::ArenaNight
                } else {
                    Area::Arena
                }
            }
            1 => Area::HakugyokuRo,
            2 => {
                if area.area_night {
                    Area::HakureiJinjyaNight
                } else {
                    Area::HakureiJinjya
                }
            }
            3 => {
                if area.area_night {
                    Area::KiriNoMizuumiNight
                } else {
                    Area::KiriNoMizuumi
                }
            }
            4 => Area::KoumaKan,
            5 => Area::MahouNoMori,
            6 => Area::MayoiNoTikurin,
            7 => Area::MoriyaJinjya,
            8 => Area::TireiDen,
            9 => {
                if area.area_night {
                    Area::YoukaiNoYamaNight
                } else {
                    Area::YoukaiNoYama
                }
            }
            _ => unreachable!(),
        }
    }
}

impl From<&SongInfoText> for MapInfoText {
    fn from(text: &SongInfoText) -> Self {
        Self {
            artist:      text.artist.as_str().into(),
            artist_kana: text.artist_kana.as_str().into(),
            artist2:     text.artist2.as_str().into(),
            original:    text.original.as_str().into(),
            sub_title:   text.sub_title.as_str().into(),
            title:       text.title.as_str().into(),
            title_kana:  text.title_kana.as_str().into(),
        }
    }
}

impl From<MapInfoText> for SongInfoText {
    fn from(text: MapInfoText) -> Self {
        Self {
            title:       text.title.into(),
            title_kana:  text.title_kana.into(),
            sub_title:   text.sub_title.into(),
            artist:      text.artist.into(),
            artist2:     text.artist2.into(),
            artist_kana: text.artist_kana.into(),
            original:    text.original.into(),
        }
    }
}

impl From<BpmChanges> for Vec<BpmChange> {
    fn from(value: BpmChanges) -> Self {
        (&value).into()
    }
}

impl From<&BpmChanges> for Vec<BpmChange> {
    fn from(value: &BpmChanges) -> Self {
        value
            .0
            .iter()
            .map(|(idx, bpm)| BpmChange {
                idx: *idx as i32,
                bpm: *bpm,
            })
            .collect()
    }
}

impl From<Vec<BpmChange>> for BpmChanges {
    fn from(value: Vec<BpmChange>) -> Self {
        Self(
            value
                .into_iter()
                .map(|bc| (bc.idx as u16, bc.bpm))
                .collect(),
        )
    }
}

impl From<&ModelRc<BpmChange>> for BpmChanges {
    fn from(value: &ModelRc<BpmChange>) -> Self {
        Self(value.iter().map(|bc| (bc.idx as u16, bc.bpm)).collect())
    }
}

impl From<&Map> for MapInfo {
    fn from(map: &Map) -> Self {
        let area_model: AreaModel = map.song_info.area.into();

        let default_text = SongInfoText::default();

        let get_info_text = |s: Lang| map.song_info.info_text.get(&s).unwrap_or(&default_text);
        let info_text = vec![
            get_info_text(JA),
            get_info_text(Chs),
            get_info_text(Cht),
            get_info_text(EN),
            get_info_text(KO),
        ]
        .into_iter()
        .map(MapInfoText::from)
        .collect::<Vec<_>>();
        let info_text = ModelRc::new(VecModel::from(info_text));

        let bpm_changes_default = BpmChanges::default();
        let bpm_changes: Vec<BpmChange> = map
            .song_info
            .bpm_changes
            .as_ref()
            .unwrap_or(&bpm_changes_default)
            .into();
        let score = map.map_scores.get(&Hard).unwrap();

        let score = MapScore {
            bpm_changes: ModelRc::new(VecModel::from(bpm_changes)),
            score:       score.scores.to_string().into(),
        };

        Self {
            area_idx: area_model.area_idx,
            area_night: area_model.area_night,
            bpm: map.song_info.bpm,
            id: map.song_info.id.to_string().into(),
            info_text,
            length: map.song_info.length as i32,
            level: map.level(Hard, None) as i32,
            music_file: map.song_info.music_file.as_str().into(),
            offset: map.song_info.offset,
            prev_start_ms: map.song_info.prev_start_ms as i32,
            score,
        }
    }
}

impl From<&MapInfo> for Map {
    fn from(map: &MapInfo) -> Self {
        let area_model = AreaModel {
            area_idx:   map.area_idx,
            area_night: map.area_night,
        };

        let id_to_lang = |id: usize| match id {
            0 => JA,
            1 => Chs,
            2 => Cht,
            3 => EN,
            4 => KO,
            _ => unreachable!(),
        };
        let info_text = map
            .info_text
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let lang = id_to_lang(i);
                let text: SongInfoText = t.into();
                (lang, text)
            })
            .filter(|(_, t)| *t != SongInfoText::default())
            .collect::<HashMap<_, _>>();

        let map_score = &map.score;
        let bpm_changes: BpmChanges = (&map_score.bpm_changes).into();
        let bpm_changes = if bpm_changes.0.is_empty() {
            None
        } else {
            Some(bpm_changes)
        };
        let map_score = crate::map::ScoreData::from_str(map_score.score.as_str()).unwrap();
        let map_score = crate::map::MapScore {
            scores: map_score.clone(),
        };

        Self {
            song_info:  SongInfo {
                id: MusicID::New(map.id.as_str().to_owned()),
                music_file: map.music_file.as_str().into(),
                bpm: map.bpm,
                offset: map.offset,
                length: map.score.score.len() as u16,
                area: area_model.into(),
                info_text,
                prev_start_ms: map.prev_start_ms as u32,
                bpm_changes,
                beats_layout: None,
                dlc_index: 0,
            },
            map_scores: hashmap! { Hard => map_score },
        }
    }
}

fn init_custom_map_model(main_window: &MainWindow) {
    let main_window = main_window.as_weak();

    main_window
        .unwrap()
        .global::<CustomMapModel>()
        .on_create_map({
            || {
                let text = || MapInfoText {
                    artist:      Default::default(),
                    artist_kana: Default::default(),
                    artist2:     Default::default(),
                    original:    Default::default(),
                    sub_title:   Default::default(),
                    title:       Default::default(),
                    title_kana:  Default::default(),
                };

                let info_text = vec![text(), text(), text(), text(), text()];
                let info_text = ModelRc::new(VecModel::from(info_text));

                MapInfo {
                    area_idx: 0,
                    area_night: false,
                    bpm: 0.0,
                    id: Default::default(),
                    info_text,
                    length: 0,
                    level: 0,
                    music_file: Default::default(),
                    offset: 0.0,
                    prev_start_ms: 0,
                    score: Default::default(),
                }
            }
        });

    main_window
        .unwrap()
        .global::<CustomMapModel>()
        .on_get_text(|map, index| map.info_text.row_data(index as usize).unwrap_or_default());

    main_window
        .unwrap()
        .global::<CustomMapModel>()
        .on_update_text({
            let main_window = main_window.clone();

            move |label_id, value| {
                if label_id.is_empty() {
                    return;
                }
                let map = main_window
                    .unwrap()
                    .global::<CustomMapModel>()
                    .get_current_map();
                let lang = main_window
                    .unwrap()
                    .global::<CustomMapModel>()
                    .get_current_lang();

                let mut row_data = map.info_text.row_data(lang as usize).unwrap();

                let field_ref = match label_id.as_ref() {
                    "title" => &mut row_data.title,
                    "sub_title" => &mut row_data.sub_title,
                    "title_kana" => &mut row_data.title_kana,
                    "artist" => &mut row_data.artist,
                    "artist2" => &mut row_data.artist2,
                    "artist_kana" => &mut row_data.artist_kana,
                    "original" => &mut row_data.original,
                    _ => unreachable!(),
                };

                if *field_ref != value {
                    *field_ref = value;

                    map.info_text.set_row_data(lang as usize, row_data);
                    main_window
                        .unwrap()
                        .global::<CustomMapModel>()
                        .invoke_set_map(map);
                }
            }
        });

    main_window
        .unwrap()
        .global::<CustomMapModel>()
        .on_update_map({
            let main_window = main_window.clone();
            move |id, music_file, bpm, offset, area_idx, area_night, prev_start_ms, score| {
                let mut map = main_window
                    .unwrap()
                    .global::<CustomMapModel>()
                    .get_current_map();

                let m_id = MusicID::from(id.as_str());
                let id = if let MusicID::Existing(_) = m_id {
                    format!("{id}1").into()
                } else {
                    id
                };

                map.id = id;
                map.music_file = music_file;
                map.bpm = bpm.as_str().parse().unwrap();
                map.offset = offset.as_str().parse().unwrap();
                map.area_idx = area_idx;
                map.area_night = area_night;
                map.prev_start_ms = prev_start_ms.as_str().parse().unwrap();
                map.score = score;

                main_window
                    .unwrap()
                    .global::<CustomMapModel>()
                    .invoke_set_map(map);
            }
        });

    main_window
        .unwrap()
        .global::<CustomMapModel>()
        .on_from_osu({
            let main_window = main_window.clone();
            move || {
                let file = rfd::FileDialog::new()
                    .set_title("Choose Osu map")
                    .add_filter("Osu Map", &["osu"])
                    .pick_file();

                let osu: anyhow::Result<crate::external_map::Osu> = try {
                    let content = std::fs::read_to_string(file.as_ref().unwrap())?;
                    crate::external_map::Osu::new(&content)?
                };
                let osu = osu.unwrap();

                let bpm = osu.initial_bpm().to_f32().unwrap();
                main_window
                    .unwrap()
                    .global::<CustomMapModel>()
                    .set_bpm(bpm.to_string().into());
                let offset = osu.offset().to_f32().unwrap() / 1000.0;
                main_window
                    .unwrap()
                    .global::<CustomMapModel>()
                    .set_offset(offset.to_string().into());

                let bpm_changes = osu
                    .bpm_changes()
                    .unwrap_or_default()
                    .0
                    .into_iter()
                    .map(|(idx, bpm)| BpmChange {
                        idx: idx as i32,
                        bpm,
                    })
                    .collect::<Vec<_>>();
                let bpm_changes = ModelRc::new(VecModel::from(bpm_changes));

                let score = osu.score().to_string().into();
                MapScore { bpm_changes, score }
            }
        });

    main_window
        .unwrap()
        .global::<CustomMapModel>()
        .on_from_adofai({
            let main_window = main_window.clone();
            move || {
                let file = rfd::FileDialog::new()
                    .set_title("Choose ADoFaI map")
                    .add_filter("ADoFaI Map", &["adofai"])
                    .pick_file();

                let adofai: anyhow::Result<crate::external_map::ADoFaIMap> = try {
                    let content = std::fs::read_to_string(file.as_ref().unwrap())?;
                    serde_json::from_str(content.trim_start_matches('\u{feff}'))?
                };
                let mut adofai = adofai.unwrap();

                let bpm = adofai.bpm();
                main_window
                    .unwrap()
                    .global::<CustomMapModel>()
                    .set_bpm(bpm.to_string().into());
                let offset = adofai.offset();
                main_window
                    .unwrap()
                    .global::<CustomMapModel>()
                    .set_offset(offset.to_string().into());

                let bpm_changes = adofai
                    .bpm_changes()
                    .into_iter()
                    .map(|(idx, bpm)| BpmChange {
                        idx: idx as i32,
                        bpm,
                    })
                    .collect::<Vec<_>>();
                let bpm_changes = ModelRc::new(VecModel::from(bpm_changes));

                let score = crate::map::ScoreData(adofai.scores()).to_string().into();
                MapScore { bpm_changes, score }
            }
        });
}
