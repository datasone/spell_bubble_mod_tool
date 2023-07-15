fn main() {
    // TODO: All-In-One build.rs after project code is put together.
    println!("cargo:rustc-link-lib=user32");
    println!("cargo:rustc-link-lib=ole32");

    println!("cargo:rustc-link-arg=/INCLUDE:NativeAOT_StaticInitialization");
    println!(
        "cargo:rustc-link-search=C:/Users/datasone/.nuget/packages/runtime.win-x64.microsoft.\
         dotnet.ilcompiler/7.0.8/sdk"
    );
    println!(
        "cargo:rustc-link-search=C:/Users/datasone/work/UnityAssetBundleHelper/\
         SpellBubbleModToolHelper/bin/Release/net7.0/win-x64/publish"
    );

    println!("cargo:rustc-link-lib=static=bootstrapperdll");
    println!("cargo:rustc-link-lib=static=Runtime.ServerGC");
    println!("cargo:rustc-link-lib=static=System.Globalization.Native.Aot");
    println!("cargo:rustc-link-lib=static=System.IO.Compression.Native.Aot");

    println!(
        "cargo:rerun-if-changed=C:/Users/datasone/work/UnityAssetBundleHelper/\
         SpellBubbleModToolHelper/bin/Release/net7.0/win-x64/publish/SpellBubbleModToolHelper.lib"
    );

    println!("cargo:rustc-link-lib=static=SpellBubbleModToolHelper");
}
