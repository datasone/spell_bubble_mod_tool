mod interop;
use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
    path::PathBuf,
};

use clap::Parser;
use interop::{initialize_assets, DualArrayWrapper, StringWrapper};

extern "C" {
    pub fn get_area_music_list(share_data_path: *const c_char) -> DualArrayWrapper;
}

#[derive(Parser, Debug)]
struct Args {
    class_package_path: PathBuf,
    share_data_path:    PathBuf,
    out_enum_rs_path:   PathBuf,
}

fn main() {
    let args = Args::parse();

    initialize_assets(args.class_package_path);

    let share_data_path = CString::new(args.share_data_path.to_str().unwrap()).unwrap();

    let (_result, _musics, _areas, music_array, area_array) = unsafe {
        let result = get_area_music_list(share_data_path.as_ptr());
        let musics =
            std::slice::from_raw_parts(result.array as *const *const c_char, result.size as usize);
        let musics = musics.iter().map(|&p| StringWrapper(p)).collect::<Vec<_>>();
        let music_array: Vec<&str> = musics
            .iter()
            .map(|p| {
                CStr::from_ptr(p.0 as *const c_char)
                    .to_str()
                    .unwrap_or_default()
            })
            .collect();

        let areas = std::slice::from_raw_parts(
            result.array2 as *const *const c_char,
            result.size2 as usize,
        );
        let areas = areas.iter().map(|&p| StringWrapper(p)).collect::<Vec<_>>();
        let area_array: Vec<&str> = areas
            .iter()
            .map(|p| {
                CStr::from_ptr(p.0 as *const c_char)
                    .to_str()
                    .unwrap_or_default()
            })
            .collect();
        (result, musics, areas, music_array, area_array)
    };

    let mut builder = String::from("");

    builder.push_str(
        "#[derive(strum::Display, strum::EnumString, serde::Serialize, serde::Deserialize, \
         PartialEq, Default, Clone, Copy)]\npub enum Area {\n",
    );
    area_array
        .iter()
        .for_each(|s| builder.push_str(&format!("    {},\n", s)));
    builder.push_str("    #[default]\n    #[strum(disabled)]\n    NotDefined,\n}\n\n");

    builder.push_str(
        "\n\n#[derive(strum::Display, strum::EnumString, serde::Serialize, serde::Deserialize, \
         PartialEq, Default, Clone, Copy)]\npub enum Music {\n",
    );
    music_array.iter().for_each(|&s| {
        if s == "Alice" {
            builder.push_str("    #[default]\n");
        }
        builder.push_str(&format!("    {},\n", s))
    });
    builder.push_str("}\n");

    std::fs::write(args.out_enum_rs_path, builder).unwrap();
}
