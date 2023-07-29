use std::process::Command;
use build_target::{Arch, Os};

fn main() {
    let dotnet_version = Command::new("dotnet").arg("--version").output().unwrap();
    let dotnet_version = if !dotnet_version.status.success() {
        dotnet_version.stdout
    } else {
        panic!("This project requires .NET SDK to build")
    };
    let dotnet_version = dotnet_version[0] - b'0';

    let os = build_target::target_os().unwrap();
    let arch = build_target::target_arch().unwrap();

    if !matches!(os, Os::Windows | Os::Linux | Os::MacOs) {
        panic!("This OS {} is not supported by .NET NativeAOT", os.as_str())
    }

    if matches!(os, Os::MacOs) && dotnet_version < 8 {
        panic!(".NET NativeAOT on macOS is supported from .NET 8")
    }

    if !matches!(arch, Arch::X86_64 | Arch::AARCH64) {
        panic!("This architecture {} is not supported by .NET NativeAOT", arch.as_str())
    }

    let rid_os = match os {
        Os::Windows => "win",
        Os::Linux => "linux",
        Os::MacOs => "macos",
        _ => unreachable!(),
    };

    let rid_arch = match arch {
        Arch::X86_64 => "x64",
        Arch::AARCH64 => "arm64",
        _ => unreachable!(),
    };

    let rid = format!("{rid_os}-{rid_arch}");

    Command::new("dotnet")
        .args(&[
            "publish",
            "-r",
            &rid,
            "-c",
            "Release",
            "/p:SelfContained=true",
            "/p:NativeLib=static",
        ])
        .current_dir("deps/SpellBubbleModToolHelper")
        .status()
        .unwrap();

    println!("cargo:rustc-link-lib=user32");
    println!("cargo:rustc-link-lib=ole32");

    println!("cargo:rustc-link-arg=/INCLUDE:NativeAOT_StaticInitialization");

    let dotnet_ilcompiler_libs_path = format!(
        "{}/.nuget/packages/runtime.{}.microsoft.dotnet.ilcompiler/7.0.9/sdk",
        env!("USERPROFILE"),
        rid,
    );
    println!("cargo:rustc-link-search={}", dotnet_ilcompiler_libs_path);

    println!(
        "cargo:rustc-link-search=deps/SpellBubbleModToolHelper/SpellBubbleModToolHelper/bin/\
         Release/net7.0/{}/publish",
        rid
    );

    println!("cargo:rustc-link-lib=static=bootstrapperdll");
    println!("cargo:rustc-link-lib=static=Runtime.ServerGC");
    println!("cargo:rustc-link-lib=static=System.Globalization.Native.Aot");
    println!("cargo:rustc-link-lib=static=System.IO.Compression.Native.Aot");

    println!(
        "cargo:rerun-if-changed=deps/SpellBubbleModToolHelper/SpellBubbleModToolHelper/\
         BridgeLib.cs"
    );
    println!(
        "cargo:rerun-if-changed=deps/SpellBubbleModToolHelper/SpellBubbleModToolHelper/\
         SpellBubbleModToolHelper.csproj"
    );
    println!(
        "cargo:rerun-if-changed=deps/SpellBubbleModToolHelper/SpellBubbleModToolHelper/\
         SpellBubbleModToolHelper.csproj.user"
    );

    println!("cargo:rustc-link-lib=static=SpellBubbleModToolHelper");
}
