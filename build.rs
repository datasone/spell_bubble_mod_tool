fn main() {
    // TODO: All-In-One build.rs after project code is put together.
    println!("cargo:rustc-link-lib=user32");
    println!("cargo:rustc-link-lib=ole32");
    println!("cargo:rustc-link-lib=bcrypt");
    println!("cargo:rustc-link-lib=ncrypt");
    println!("cargo:rustc-link-lib=crypt32");
    println!("cargo:rustc-link-lib=iphlpapi");

    println!("cargo:rustc-link-lib=static=bootstrapperdll");
    println!("cargo:rustc-link-lib=static=Runtime.ServerGC");
    println!("cargo:rustc-link-lib=static=System.IO.Compression.Native.Aot");

    println!("cargo:rustc-link-lib=static=SpellBubbleModToolHelper");

    println!("cargo:rustc-link-search=./lib");
}
