use std::process::Command;

fn main() {
    // TODO: Cross-platform

    Command::new("dotnet")
        .args(&[
            "publish",
            "-r",
            "win-x64",
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
        "{}/.nuget/packages/runtime.win-x64.microsoft.dotnet.ilcompiler/7.0.9/sdk",
        env!("USERPROFILE")
    );
    println!("cargo:rustc-link-search={}", dotnet_ilcompiler_libs_path);

    println!(
        "cargo:rustc-link-search=deps/SpellBubbleModToolHelper/SpellBubbleModToolHelper/bin/\
         Release/net7.0/win-x64/publish"
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
