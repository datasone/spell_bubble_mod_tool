use std::{
    ffi::{CStr, CString},
    os::raw::{c_char, c_void},
    process::exit,
};

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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out_path = args.get(1).unwrap_or_else(|| exit(-1));
    if out_path.is_empty() {
        exit(-1)
    }

    let class_package_path = CString::new("C:\\Users\\datasone\\work\\classdata.tpk").unwrap();
    let share_data_path = CString::new(
        "C:\\Users\\datasone\\work\\TouhouSB Hack\\RomFS\\TOUHOU Spell Bubble v2359296 \
         (0100E9D00D6C2800) (UPD)\\Data\\StreamingAssets\\Switch\\share_data",
    )
    .unwrap();

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

    builder.push_str("#[derive(strum::Display, strum::EnumString, PartialEq)]\npub enum Area {\n");
    area_array
        .iter()
        .for_each(|s| builder.push_str(&format!("    {},\n", s)));
    builder.push_str("    #[strum(disabled)]\n    NotDefined,\n}\n\n");

    builder.push_str(
        r"impl Default for Area {
    fn default() -> Self {
        Area::NotDefined
    }
}",
    );

    builder.push_str("\n\n#[derive(strum::Display, strum::EnumString)]\npub enum Music {\n");
    music_array
        .iter()
        .for_each(|s| builder.push_str(&format!("    {},\n", s)));
    builder.push_str("}\n");

    builder.push_str(
        r"impl Default for Music {
    fn default() -> Self {
        Music::Alice
    }
}",
    );

    std::fs::write(out_path, builder).unwrap();

    free_area_music_list(result);
}
