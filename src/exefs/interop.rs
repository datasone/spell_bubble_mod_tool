use std::{
    ffi::{CString, c_char, c_int, c_void},
    path::Path,
};

use crate::interop::ArrayWrapper;

#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug)]
struct MetadataInformation {
    type_def_header_offset: u32,

    eMusicID_type_index:     u32,
    eMusicID_field_start:    u32,
    eMusicID_field_count:    u16,
    eMusicID_type_def_index: u32,

    eMusicID_Tutorial_value:     u32,
    // "Tutorial", "Menu", "Select", "Map", "Shop", "Calibration", "Result", "NUM", "NONE"
    eMusicID_value_data_offsets: ArrayWrapper,

    string_table_offset:         u32,
    string_table_length:         u32,
    string_offset_header_offset: u32,

    field_def_table_offset:         u32,
    field_def_table_length:         u32,
    field_def_offset_header_offset: u32,
    max_field_def_token:            u32,
    max_field_index:                u32,

    field_default_value_table_offset:         u32,
    field_default_value_table_length:         u32,
    field_default_value_offset_header_offset: u32,

    default_value_data_table_offset:         u32,
    default_value_data_table_length:         u32,
    default_value_data_offset_header_offset: u32,
}

/// Il2CppFieldDefinition
#[repr(C)]
struct FieldDefinition {
    name_index: u32,
    type_index: u32,
    token:      u32,
}

impl FieldDefinition {
    fn to_bytes(&self) -> Vec<u8> {
        [
            self.name_index.to_le_bytes(),
            self.type_index.to_le_bytes(),
            self.token.to_le_bytes(),
        ]
        .iter()
        .flatten()
        .cloned()
        .collect()
    }
}

/// Il2CppFieldDefaultValue
#[repr(C)]
struct FieldDefaultValue {
    field_index: u32,
    type_index:  u32,
    data_index:  u32,
}

impl FieldDefaultValue {
    fn to_bytes(&self) -> Vec<u8> {
        [
            self.field_index.to_le_bytes(),
            self.type_index.to_le_bytes(),
            self.data_index.to_le_bytes(),
        ]
        .iter()
        .flatten()
        .cloned()
        .collect()
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            field_index: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            type_index:  u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            data_index:  u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        }
    }
}

extern "C" {
    fn get_metadata_regions(global_metadata_path: *const c_char) -> MetadataInformation;
}

macro_rules! table_bytes_to_indices {
    ($table_append_bytes:ident, $table:ident) => {{
        let mut indices = $table_append_bytes
            .iter()
            .fold(vec![$table.len()], |mut vec, bytes| {
                vec.push(vec.last().unwrap() + bytes.len());
                vec
            });
        indices.pop();
        indices
    }};
}

// These values vary by il2cpp version
const IL2CPP_FIELD_DEFINITION_SIZE: u32 = 12;
const IL2CPP_TYPE_DEFINITION_SIZE: u32 = 88;

pub fn add_emusic_id_enums<T, U>(
    global_metadata_path: &Path,
    out_metadata_path: &Path,
    names: T,
) -> usize
where
    T: IntoIterator<Item = U>,
    U: AsRef<str>,
{
    let enums_to_add = names.into_iter().collect::<Vec<_>>();
    let enums_to_add = enums_to_add.iter().map(|s| s.as_ref()).collect::<Vec<_>>();

    let global_metadata_path_c =
        CString::new(global_metadata_path.to_string_lossy().as_ref()).unwrap();
    let metadata_info = unsafe { get_metadata_regions(global_metadata_path_c.as_ptr()) };

    let mut metadata_file = std::fs::read(global_metadata_path).unwrap();
    let mut string_table = metadata_file[metadata_info.string_table_offset as usize
        ..metadata_info.string_table_offset as usize + metadata_info.string_table_length as usize]
        .to_vec();
    let mut field_def_table = metadata_file[metadata_info.field_def_table_offset as usize
        ..metadata_info.field_def_table_offset as usize
            + metadata_info.field_def_table_length as usize]
        .to_vec();
    let mut field_default_value_table =
        metadata_file[metadata_info.field_default_value_table_offset as usize
            ..metadata_info.field_default_value_table_offset as usize
                + metadata_info.field_default_value_table_length as usize]
            .to_vec();
    let mut default_value_data_table = metadata_file[metadata_info.default_value_data_table_offset
        as usize
        ..metadata_info.default_value_data_table_offset as usize
            + metadata_info.default_value_data_table_length as usize]
        .to_vec();

    let field_default_values = field_default_value_table
        .chunks(12)
        .map(FieldDefaultValue::from_bytes)
        .collect::<Vec<_>>();

    let string_table_append_bytes_list = enums_to_add
        .iter()
        .map(|s| s.as_bytes().to_vec())
        .map(|mut bytes| {
            bytes.push(0);
            bytes
        })
        .collect::<Vec<_>>();
    let string_indices = table_bytes_to_indices!(string_table_append_bytes_list, string_table);

    let mut string_table_append = string_table_append_bytes_list
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    let original_field_offset =
        (IL2CPP_FIELD_DEFINITION_SIZE * metadata_info.eMusicID_field_start) as usize;
    let mut original_field_defs = field_def_table[original_field_offset
        ..original_field_offset
            + (IL2CPP_FIELD_DEFINITION_SIZE * metadata_info.eMusicID_field_count as u32) as usize]
        .to_vec();

    let mut field_def_table_append = enums_to_add
        .iter()
        .zip(string_indices.iter())
        .enumerate()
        .map(|(idx, (_, name_idx))| FieldDefinition {
            name_index: *name_idx as u32,
            type_index: metadata_info.eMusicID_type_index,
            token:      metadata_info.max_field_def_token + idx as u32 + 1,
        })
        .flat_map(|fd| fd.to_bytes())
        .collect::<Vec<_>>();

    original_field_defs.append(&mut field_def_table_append);
    let mut field_def_table_append = original_field_defs;

    let field_offset = metadata_info.max_field_index + 1 - metadata_info.eMusicID_field_start;

    // In eMusicID definition, NUM and NONE variants are guessed to be used as
    // special means. We choose to insert the new enum variants before NUM
    // variant.
    let default_value_data_table_append_bytes_list = enums_to_add
        .iter()
        .enumerate()
        .map(|(i, _)| (metadata_info.eMusicID_Tutorial_value + i as u32).to_le_bytes())
        .collect::<Vec<_>>();

    let default_value_data_indices = table_bytes_to_indices!(
        default_value_data_table_append_bytes_list,
        default_value_data_table
    );
    let mut default_value_data_table_append = default_value_data_table_append_bytes_list
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    let e_music_id_fdvs = field_default_values
        .into_iter()
        .filter(|fdv| {
            (metadata_info.eMusicID_field_start
                ..metadata_info.eMusicID_field_start + metadata_info.eMusicID_field_count as u32)
                .contains(&fdv.field_index)
        })
        .map(|mut fdv| {
            fdv.field_index += field_offset;
            fdv
        })
        .collect::<Vec<_>>();

    let field_default_value_type_index = e_music_id_fdvs[0].type_index;

    let mut e_music_id_fdvs = e_music_id_fdvs
        .into_iter()
        .flat_map(|fdv| fdv.to_bytes())
        .collect::<Vec<_>>();

    let mut field_default_value_table_append = enums_to_add
        .iter()
        .zip(default_value_data_indices.iter())
        .enumerate()
        .map(|(idx, (_, data_idx))| FieldDefaultValue {
            field_index: metadata_info.max_field_index
                + 1
                + metadata_info.eMusicID_field_count as u32
                + idx as u32,
            type_index:  field_default_value_type_index,
            data_index:  *data_idx as u32,
        })
        .flat_map(|fdv| fdv.to_bytes())
        .collect::<Vec<_>>();

    e_music_id_fdvs.append(&mut field_default_value_table_append);
    let mut field_default_value_table_append = e_music_id_fdvs;

    string_table.append(&mut string_table_append);
    field_def_table.append(&mut field_def_table_append);
    field_default_value_table.append(&mut field_default_value_table_append);
    default_value_data_table.append(&mut default_value_data_table_append);

    let e_music_id_type_def_offset = (metadata_info.type_def_header_offset
        + metadata_info.eMusicID_type_def_index * IL2CPP_TYPE_DEFINITION_SIZE)
        as usize;
    let total_field_count = metadata_info.field_def_table_length / IL2CPP_FIELD_DEFINITION_SIZE;
    metadata_file[e_music_id_type_def_offset + 8 * 4..e_music_id_type_def_offset + 8 * 4 + 4]
        .copy_from_slice(&total_field_count.to_le_bytes());
    metadata_file[e_music_id_type_def_offset + 17 * 4..e_music_id_type_def_offset + 17 * 4 + 2]
        .copy_from_slice(
            &(metadata_info.eMusicID_field_count + enums_to_add.len() as u16).to_le_bytes(),
        );

    let mut offset = metadata_file.len() as u32;
    metadata_file[metadata_info.string_offset_header_offset as usize
        ..metadata_info.string_offset_header_offset as usize + 4]
        .copy_from_slice(&offset.to_le_bytes());
    metadata_file[metadata_info.string_offset_header_offset as usize + 4
        ..metadata_info.string_offset_header_offset as usize + 8]
        .copy_from_slice(&(string_table.len() as u32).to_le_bytes());
    metadata_file.append(&mut string_table);

    offset = metadata_file.len() as u32;
    metadata_file[metadata_info.field_def_offset_header_offset as usize
        ..metadata_info.field_def_offset_header_offset as usize + 4]
        .copy_from_slice(&offset.to_le_bytes());
    metadata_file[metadata_info.field_def_offset_header_offset as usize + 4
        ..metadata_info.field_def_offset_header_offset as usize + 8]
        .copy_from_slice(&(field_def_table.len() as u32).to_le_bytes());
    metadata_file.append(&mut field_def_table);

    offset = metadata_file.len() as u32;
    metadata_file[metadata_info.field_default_value_offset_header_offset as usize
        ..metadata_info.field_default_value_offset_header_offset as usize + 4]
        .copy_from_slice(&offset.to_le_bytes());
    metadata_file[metadata_info.field_default_value_offset_header_offset as usize + 4
        ..metadata_info.field_default_value_offset_header_offset as usize + 8]
        .copy_from_slice(&(field_default_value_table.len() as u32).to_le_bytes());
    metadata_file.append(&mut field_default_value_table);

    offset = metadata_file.len() as u32;
    metadata_file[metadata_info.default_value_data_offset_header_offset as usize
        ..metadata_info.default_value_data_offset_header_offset as usize + 4]
        .copy_from_slice(&offset.to_le_bytes());
    metadata_file[metadata_info.default_value_data_offset_header_offset as usize + 4
        ..metadata_info.default_value_data_offset_header_offset as usize + 8]
        .copy_from_slice(&(default_value_data_table.len() as u32).to_le_bytes());
    metadata_file.append(&mut default_value_data_table);

    let value_data_offsets = metadata_info.eMusicID_value_data_offsets;
    let value_data_offsets = unsafe {
        std::slice::from_raw_parts(
            value_data_offsets.array as *const c_int,
            value_data_offsets.size as usize,
        )
    };

    value_data_offsets
        .iter()
        .enumerate()
        .for_each(|(i, &data_offset)| {
            let value_data_offset = offset as usize + data_offset as usize;
            let value_data_data =
                metadata_info.eMusicID_Tutorial_value + enums_to_add.len() as u32 + i as u32;
            let value_data_slice = &mut metadata_file[value_data_offset..value_data_offset + 4];
            value_data_slice.copy_from_slice(&value_data_data.to_le_bytes());
        });

    std::fs::write(out_metadata_path, metadata_file).unwrap();

    enums_to_add.len()
}

extern "C" {
    fn patch_main_asset_bundle_internal(
        main_ab_path: *const c_char,
        out_ab_path: *const c_char,
        added_song_ids: ArrayWrapper,
    );
}

pub fn patch_main_asset_bundle<T, U>(main_ab_path: &Path, out_ab_path: &Path, added_song_ids: T)
where
    T: IntoIterator<Item = U>,
    U: AsRef<str>,
{
    let main_ab_path = CString::new(main_ab_path.to_string_lossy().to_string()).unwrap();
    let out_ab_path = CString::new(out_ab_path.to_string_lossy().to_string()).unwrap();
    let added_song_ids = added_song_ids
        .into_iter()
        .map(|s| CString::new(s.as_ref()).unwrap())
        .collect::<Vec<_>>();
    let added_song_ids = added_song_ids
        .iter()
        .map(|cs| cs.as_ptr())
        .collect::<Vec<_>>();

    unsafe {
        let added_song_ids = ArrayWrapper {
            managed: 0,
            size:    added_song_ids.len() as u32,
            array:   std::mem::transmute::<*const *const i8, *mut c_void>(added_song_ids.as_ptr()),
        };
        patch_main_asset_bundle_internal(
            main_ab_path.as_ptr(),
            out_ab_path.as_ptr(),
            added_song_ids,
        )
    }
}
