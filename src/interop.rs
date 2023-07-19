use std::{
    ffi::{c_void, CString},
    os::raw::{c_char, c_int},
    path::PathBuf,
};

#[repr(C)]
pub struct ArrayWrapper {
    pub size:  u32,
    pub array: *mut c_void,
}

extern "C" {
    pub fn initialize(class_package_path: *const c_char);
    pub fn patch_features(
        share_data_path: *const c_char,
        out_path: *const c_char,
        patch_music: c_int, // C style bool, 0 for false, others for true
        excluded_dlcs: ArrayWrapper,
        left_music_id: *const c_char, // Unused for now
        patch_characters: c_int,      // C style bool, 0 for false, others for true
        character_target_dlc: c_int,  // Unused for now
        patch_special_rules: c_int,   // C style bool, 0 for false, others for true
    );
}

pub fn initialize_assets(class_package_path: PathBuf) {
    let class_package_path = CString::new(class_package_path.to_str().unwrap()).unwrap();

    unsafe {
        initialize(class_package_path.as_ptr());
    }
}
