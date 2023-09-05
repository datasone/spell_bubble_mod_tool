use std::{
    ffi::{c_void, CString},
    os::raw::c_char,
    path::PathBuf,
};

#[repr(C)]
pub struct ArrayWrapper {
    /// 0 for structs from Rust, 1 for C#
    pub managed: u32,
    pub size:    u32,
    pub array:   *mut c_void,
}

impl Drop for ArrayWrapper {
    fn drop(&mut self) {
        if self.managed == 1 {
            unsafe {
                free_dotnet(self.array);
            }
        }
    }
}

#[repr(C)]
pub struct DualArrayWrapper {
    pub size:   u32,
    pub array:  *mut c_void,
    pub size2:  u32,
    pub array2: *mut c_void,
}

impl Drop for DualArrayWrapper {
    fn drop(&mut self) {
        unsafe {
            free_dotnet(self.array);
            free_dotnet(self.array2);
        }
    }
}

pub struct StringWrapper(pub *const c_char);

impl Drop for StringWrapper {
    fn drop(&mut self) {
        unsafe { free_dotnet(self.0 as *mut c_void) }
    }
}

extern "C" {
    pub fn initialize(class_package_path: *const c_char);
    pub fn free_dotnet(pointer: *mut c_void);
}

pub fn initialize_assets(class_package_path: PathBuf) {
    let class_package_path = CString::new(class_package_path.to_str().unwrap()).unwrap();

    unsafe {
        initialize(class_package_path.as_ptr());
    }
}
