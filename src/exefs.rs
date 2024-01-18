#[cfg(unix)]
use std::os::unix::prelude::FileExt;
#[cfg(windows)]
use std::os::windows::prelude::FileExt;
use std::{fs::File, path::Path, str::FromStr};

use interop::patch_main_asset_bundle;
use serde::{Deserialize, Serialize};

mod interop;

fn get_build_id(main_exe: &Path) -> [u8; 16] {
    let mut build_id = [0; 16];

    let main_exe = File::open(main_exe).unwrap();
    #[cfg(unix)]
    main_exe.read_exact_at(&mut build_id, 0x40).unwrap();
    #[cfg(windows)]
    {
        let mut bytes_read = 0;
        while bytes_read < 16 {
            bytes_read += main_exe
                .seek_read(&mut build_id[bytes_read..], 0x40 + bytes_read as u64)
                .unwrap();
        }
    }

    build_id
}

#[derive(Serialize, Deserialize)]
struct IPConfig {
    patches: Vec<InstructionPatch>,
}

#[derive(Serialize, Deserialize)]
struct InstructionPatch {
    /// IPS32 file format only allows 4-bytes offset
    offset:         u32,
    /// Instruction in little endian bytes
    instruction:    AArch64Instruction,
    #[serde(default)]
    /// If the instruction is intended to be used as override
    /// where the patch_immediate returns directly the instruction
    override_patch: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
struct AArch64Instruction {
    op_code:       AArch64AssemblyOpCode,
    w_register_id: u8,
    immediate:     u16,
}

impl Default for AArch64Instruction {
    fn default() -> Self {
        "MOV W0, 0x0".try_into().unwrap()
    }
}

impl TryFrom<String> for AArch64Instruction {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        (&*value).try_into()
    }
}

impl TryFrom<&str> for AArch64Instruction {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let value = value.to_ascii_uppercase();

        if value.starts_with('B') {
            let addr = value.strip_prefix('B').unwrap().trim();
            let addr = if addr.starts_with("0X") {
                u16::from_str_radix(addr.strip_prefix("0X").unwrap(), 16)
                    .map_err(|e| format!("{:?}", e))?
            } else {
                addr.parse().map_err(|e| format!("{:?}", e))?
            };

            return Ok(Self {
                op_code:       AArch64AssemblyOpCode::B,
                w_register_id: 0,
                immediate:     addr / 4,
            });
        }

        let split = value
            .split(',')
            .flat_map(|s| s.trim().split(' '))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();

        let immediate = split[2].strip_prefix('#').unwrap();
        let immediate = if immediate.starts_with("0X") {
            u16::from_str_radix(immediate.strip_prefix("0X").unwrap(), 16)
                .map_err(|e| format!("{:?}", e))?
        } else {
            immediate.parse().map_err(|e| format!("{:?}", e))?
        };

        Ok(Self {
            op_code: AArch64AssemblyOpCode::from_str(split[0]).map_err(|e| format!("{:?}", e))?,
            w_register_id: split[1]
                .strip_prefix('W')
                .unwrap()
                .parse()
                .map_err(|e| format!("{:?}", e))?,
            immediate,
        })
    }
}

impl From<AArch64Instruction> for String {
    fn from(value: AArch64Instruction) -> Self {
        format!(
            "{} W{}, #0x{:x}",
            value.op_code, value.w_register_id, value.immediate
        )
    }
}

impl AArch64Instruction {
    fn to_bytes(&self) -> u32 {
        let bytes = self.op_code.instruction_skeleton();

        let w_register = self
            .op_code
            .register_position()
            .to_mask_value(self.w_register_id as u32);
        let immediate = self
            .op_code
            .immediate_position()
            .to_mask_value(self.immediate as u32);

        let bytes = bytes & (!self.op_code.register_position().to_mask());
        let bytes = bytes & (!self.op_code.immediate_position().to_mask());

        bytes | w_register | immediate
    }
}

#[derive(strum::Display, strum::EnumString, Clone, Copy)]
#[allow(clippy::upper_case_acronyms)]
enum AArch64AssemblyOpCode {
    /// CMP (immediate)
    CMP,
    /// MOV (wide immediate)
    MOV,
    /// B
    B,
}

impl AArch64AssemblyOpCode {
    fn immediate_position(&self) -> InstructionNumPosition {
        match self {
            AArch64AssemblyOpCode::CMP => InstructionNumPosition {
                bit_start: 10,
                length:    12,
            },
            AArch64AssemblyOpCode::MOV => InstructionNumPosition {
                bit_start: 5,
                length:    16,
            },
            AArch64AssemblyOpCode::B => InstructionNumPosition {
                bit_start: 0,
                length:    26,
            },
        }
    }

    fn register_position(&self) -> InstructionNumPosition {
        match self {
            AArch64AssemblyOpCode::CMP => InstructionNumPosition {
                bit_start: 5,
                length:    5,
            },
            AArch64AssemblyOpCode::MOV => InstructionNumPosition {
                bit_start: 0,
                length:    5,
            },
            AArch64AssemblyOpCode::B => InstructionNumPosition {
                bit_start: 0,
                length:    0,
            },
        }
    }

    fn instruction_skeleton(&self) -> u32 {
        match self {
            AArch64AssemblyOpCode::CMP => 0x7100001F,
            AArch64AssemblyOpCode::MOV => 0x52800000,
            AArch64AssemblyOpCode::B => 0x14000000,
        }
    }
}

struct InstructionNumPosition {
    /// Start bit of immediate value as of ARM reference manual
    /// Lowest bit is marked as 0, in big endian bytes
    bit_start: u8,
    length:    u8,
}

impl InstructionNumPosition {
    fn to_mask(&self) -> u32 {
        let mask = (1 << self.length) - 1;
        mask << self.bit_start
    }

    fn to_mask_value(&self, value: u32) -> u32 {
        value << self.bit_start
    }
}

impl InstructionPatch {
    /// Returns patched instruction in big endian bytes
    fn patch_immediate(&self, immediate_offset: i16) -> u32 {
        let immediate = if self.override_patch {
            self.instruction.immediate
        } else {
            (self.instruction.immediate as i16 + immediate_offset) as u16
        };

        let instruction = AArch64Instruction {
            immediate,
            ..self.instruction
        };

        instruction.to_bytes()
    }
}

fn generate_ips_file(main_exe: &Path, out_dir: &Path, immediate_offset: i16) {
    let mod_name = out_dir.file_name().unwrap().to_string_lossy().to_string();
    let mut out_ips_path = out_dir.to_owned();
    out_ips_path.push("exefs_patches");
    out_ips_path.push(mod_name);
    std::fs::create_dir_all(&out_ips_path).unwrap();

    let build_id = get_build_id(main_exe);
    out_ips_path.push(format!("{}.ips", hex::encode_upper(build_id)));

    let patches: IPConfig = toml::from_str(include_str!("exefs_patches.toml")).unwrap();

    let mut ips_content = "IPS32".as_bytes().to_vec();

    let mut ips_patch_bytes = patches
        .patches
        .iter()
        .flat_map(|p| {
            let mut out_bytes = [0; 10];

            let offset = p.offset + 0x100;
            out_bytes[0..4].copy_from_slice(&offset.to_be_bytes());
            out_bytes[5] = 0x04;

            let instruction_be = p.patch_immediate(immediate_offset);
            out_bytes[6..].copy_from_slice(&instruction_be.to_le_bytes());

            out_bytes
        })
        .collect::<Vec<_>>();

    ips_content.append(&mut ips_patch_bytes);
    ips_content.extend_from_slice("EEOF".as_bytes());

    std::fs::write(out_ips_path, ips_content).unwrap();
}

pub fn patch_files(
    romfs_root: &Path,
    main_exe_path: &Path,
    outdir: &Path,
    names: &[impl AsRef<str>],
) {
    let mut metadata_path = romfs_root.to_owned();
    metadata_path.push("Managed/Metadata/global-metadata.dat");

    let mut out_base_path = outdir.to_owned();
    out_base_path.push("contents/0100E9D00D6C2000/romfs/Data");
    let mut out_metadata_path = out_base_path.to_owned();
    out_metadata_path.push("Managed/Metadata");
    std::fs::create_dir_all(&out_metadata_path).unwrap();
    out_metadata_path.push("global-metadata.dat");

    let entries_count = interop::add_emusic_id_enums(&metadata_path, &out_metadata_path, names);
    generate_ips_file(main_exe_path, outdir, entries_count as i16);

    let mut main_ab_path = romfs_root.to_owned();
    main_ab_path.push("StreamingAssets/Switch/Switch");
    let mut out_ab_path = out_base_path.to_owned();
    out_ab_path.push("StreamingAssets/Switch/Switch");

    patch_main_asset_bundle(&main_ab_path, &out_ab_path, names)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generate_example_config() {
        let config = IPConfig {
            patches: vec![InstructionPatch {
                offset:         0,
                instruction:    AArch64Instruction::default(),
                override_patch: false,
            }],
        };

        println!("{}", toml::to_string_pretty(&config).unwrap());
    }

    #[test]
    fn test_patch_instruction() {
        let ip = InstructionPatch {
            offset:         0, // Doesn't matter now
            instruction:    "cmp w20, #0x110".try_into().unwrap(),
            override_patch: false,
        };

        assert_eq!(ip.patch_immediate(5), 0x7104569F);
        assert_eq!(ip.patch_immediate(16), 0x7104829F);
    }

    #[test]
    fn test_b_instruction() {
        let ip = InstructionPatch {
            offset:         0,
            instruction:    "B          0xFC".try_into().unwrap(),
            override_patch: true,
        };

        assert_eq!(ip.patch_immediate(5), 0x1400003F);
    }
}
