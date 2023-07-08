use std::{
    ffi::{c_void, CString},
    os::raw::{c_char, c_int},
};

#[repr(C)]
pub struct ArrayWrapper {
    pub size:  u32,
    pub array: *mut c_void,
}

extern "C" {
    pub fn initialize(class_package_path: *const c_char);
    pub fn patch_music_and_character(
        share_data_path: *const c_char,
        out_path: *const c_char,
        excluded_dlcs: ArrayWrapper,
        left_music_id: *const c_char,
        character_enabled: c_int, // C style bool, 0 for false, others for true
        character_target_dlc: c_int,
    );
    pub fn patch_special_rules(share_data_path: *const c_char, out_path: *const c_char);
}

pub fn initialize_assets() {
    let class_package_path = CString::new("C:\\Users\\datasone\\work\\classdata.tpk").unwrap();

    unsafe {
        initialize(class_package_path.as_ptr());
    }
}
