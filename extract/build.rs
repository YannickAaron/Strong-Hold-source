// libvpx for VP8 decoding (src/vpx.rs), linked statically so the app needs no dylib: VPX_LIB_DIR, else Homebrew's.
fn main() {
    let dir = std::env::var("VPX_LIB_DIR").unwrap_or_else(|_| "/opt/homebrew/opt/libvpx/lib".into());
    println!("cargo:rerun-if-env-changed=VPX_LIB_DIR");
    println!("cargo:rustc-link-search=native={dir}");
    println!("cargo:rustc-link-lib=static=vpx");
}
