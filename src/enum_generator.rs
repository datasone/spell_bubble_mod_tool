use std::{
    ffi::{CStr, CString},
    os::raw::{c_char, c_void},
    path::PathBuf,
};

use clap::Parser;

#[repr(C)]
pub struct DualArrayWrapper {
    pub size:   u32,
    pub array:  *mut usize,
    pub size2:  u32,
    pub array2: *mut usize,
}

extern "C" {
    pub fn initialize(class_package_path: *const c_char);
    pub fn get_area_music_list(share_data_path: *const c_char) -> DualArrayWrapper;
    pub fn free_dotnet(pointer: *mut c_void);
}

fn free_area_music_list(wrapper: DualArrayWrapper) {
    unsafe {
        let music_array = std::slice::from_raw_parts(wrapper.array, wrapper.size as usize);
        music_array
            .iter()
            .for_each(|p| free_dotnet(*p as *mut c_void));
        free_dotnet(wrapper.array as *mut c_void);
        let area_array = std::slice::from_raw_parts(wrapper.array2, wrapper.size2 as usize);
        area_array
            .iter()
            .for_each(|p| free_dotnet(*p as *mut c_void));
        free_dotnet(wrapper.array2 as *mut c_void);
    }
}

#[derive(Parser, Debug)]
struct Args {
    class_package_path: PathBuf,
    share_data_path:    PathBuf,
    out_enum_rs_path:   PathBuf,
}

fn main() {
    let args = Args::parse();

    let class_package_path = CString::new(args.class_package_path.to_str().unwrap()).unwrap();
    let share_data_path = CString::new(args.share_data_path.to_str().unwrap()).unwrap();

    let (result, music_array, area_array) = unsafe {
        initialize(class_package_path.as_ptr());
        let result = get_area_music_list(share_data_path.as_ptr());
        let music_array = std::slice::from_raw_parts(result.array, result.size as usize);
        let music_array: Vec<&str> = music_array
            .iter()
            .map(|p| {
                CStr::from_ptr(*p as *const c_char)
                    .to_str()
                    .unwrap_or_default()
            })
            .collect();
        let area_array = std::slice::from_raw_parts(result.array2, result.size2 as usize);
        let area_array: Vec<&str> = area_array
            .iter()
            .map(|p| {
                CStr::from_ptr(*p as *const c_char)
                    .to_str()
                    .unwrap_or_default()
            })
            .collect();
        (result, music_array, area_array)
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
         Default, Clone, Copy)]\npub enum Music {\n",
    );
    music_array.iter().for_each(|&s| {
        if s == "Alice" {
            builder.push_str("    #[default]\n");
        }
        builder.push_str(&format!("    {},\n", s))
    });
    builder.push_str("}\n");

    std::fs::write(args.out_enum_rs_path, builder).unwrap();

    free_area_music_list(result);
}
