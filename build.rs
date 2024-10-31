use std::process::Command;

use build_target::{Arch, Os};

fn main() {
    let dotnet_version = Command::new("dotnet").arg("--version").output();
    let dotnet_version = if let Ok(dotnet_version) = dotnet_version {
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
        panic!(
            "This architecture {} is not supported by .NET NativeAOT",
            arch.as_str()
        )
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
        .args([
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

    if let Os::Windows = os {
        println!("cargo:rustc-link-lib=user32");
        println!("cargo:rustc-link-lib=ole32");
    }

    let dotnet_ilcompiler_sdk_libs_path = format!(
        "{}/.nuget/packages/runtime.{}.microsoft.dotnet.ilcompiler/8.0.0/sdk",
        if let Os::Windows = os {
            std::env::var("USERPROFILE").unwrap()
        } else {
            std::env::var("HOME").unwrap()
        },
        rid,
    );
    println!(
        "cargo:rustc-link-search={}",
        dotnet_ilcompiler_sdk_libs_path
    );

    let dotnet_ilcompiler_framework_libs_path = format!(
        "{}/.nuget/packages/runtime.{}.microsoft.dotnet.ilcompiler/8.0.0/framework",
        if let Os::Windows = os {
            std::env::var("USERPROFILE").unwrap()
        } else {
            std::env::var("HOME").unwrap()
        },
        rid,
    );
    println!(
        "cargo:rustc-link-search={}",
        dotnet_ilcompiler_framework_libs_path
    );

    println!(
        "cargo:rustc-link-search=deps/SpellBubbleModToolHelper/SpellBubbleModToolHelper/bin/\
         Release/net8.0/{}/publish",
        rid
    );

    // Cargo can only link static library, while bootstrapperdll is provided as an
    // object file. And the bootstrapperdll library should be manually created
    // with ar. Also, linking with library file will not preserve symbols
    // required for .NET runtime initialization. So the "+whole-archive" is
    // required. (That's also why /INCLUDE:NativeAOT_StaticInitialization
    // argument is required for .NET 7 libraries)
    println!("cargo:rustc-link-lib=static:+whole-archive=bootstrapperdll");
    println!("cargo:rustc-link-lib=static=eventpipe-disabled");
    println!("cargo:rustc-link-lib=static=Runtime.ServerGC");

    match os {
        Os::Windows => {
            println!("cargo:rustc-link-lib=static=Runtime.VxsortEnabled");
            println!("cargo:rustc-link-lib=static=System.Globalization.Native.Aot");
            println!("cargo:rustc-link-lib=static=System.IO.Compression.Native.Aot");
        }
        Os::Linux => {
            println!("cargo:rustc-link-lib=static=System.Native");
            println!("cargo:rustc-link-lib=static=System.Globalization.Native");
            println!("cargo:rustc-link-lib=static=System.IO.Compression.Native");
            println!("cargo:rustc-link-lib=static=System.Net.Security.Native");
            println!("cargo:rustc-link-lib=static=System.Security.Cryptography.Native.OpenSsl");

            println!("cargo:rustc-link-lib=static=z");
            println!("cargo:rustc-flags=-l dylib=stdc++");
        }
        Os::MacOs => {
            // TODO
        }
        _ => unreachable!(),
    }

    let cs_helper_files = [
        "BridgeInfo.cs",
        "GetInfo.cs",
        "MainAssetBundle.cs",
        "MD4.cs",
        "PatchAudio.cs",
        "rd.xml",
        "ScoreFiles.cs",
        "ShareData.cs",
        "SpellBubbleModToolHelper.csproj",
        "SpellBubbleModToolHelper.csproj.user",
        "UnlockFeatures.cs",
        "Wrappers.cs",
    ];

    for file in cs_helper_files {
        println!(
            "cargo:rerun-if-changed=deps/SpellBubbleModToolHelper/SpellBubbleModToolHelper/{file}"
        );
    }

    if let Os::Windows = os {
        println!("cargo:rustc-link-lib=static=SpellBubbleModToolHelper");
    } else {
        println!("cargo:rustc-link-lib=static:+verbatim=SpellBubbleModToolHelper.a");
    }

    let mut slint_root = "src/ui";

    if let Os::Windows = os {
        if let Some("zh_CN") = option_env!("BUILD_LANG") {
            slint_root = "src/ui/win-zh_CN";
        }
    }

    slint_build::compile(format!("{slint_root}/MainWindow.slint")).unwrap();
}
