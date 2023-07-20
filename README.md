# spell_bubble_mod_tool

A helper tool for Touhou Spell Bubble modding. See help of mod_tool for detailed usages.

## Build Deps
- Rust toolchain
- .NET SDK 7 (the `dotnet.exe` executable must be present in `%PATH%`)

## Build
```
git submodule --init --recursive
# Build enum_generator
cargo build --bin enum_generator
# Generate enums.rs
./target/debug/enum_generator <PATH_TO_classdata.tpk> <PATH_TO_share_data> src/map/enums.rs
# Build project
cargo build --release
```

- `enums.rs` needs to be updated alongside the game if you want to use music IDs introduced in game updates.
- `classdata.tpk` can be downloaded from [AssetsTools.NET releases](https://github.com/nesrak1/AssetsTools.NET/releases).
- `share_data` is located in `/Data/StreamingAssets/Switch` inside the game RomFS.

## Credits
- [AssetsTools.NET](https://github.com/nesrak1/AssetsTools.NET)
- [SonicAudioTools](https://github.com/blueskythlikesclouds/SonicAudioTools)
- [VGAudio](https://github.com/Thealexbarney/VGAudio)
