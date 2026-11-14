//! On windows-gnu, keeps MinGW's default manifest out of udm-gui, which
//! holds the window's manifest (`crates/ui/windows-app-manifest.xml`).
//! udm-ui's build.rs explains why and makes the folder this passes to GCC.

fn main() {
    println!("cargo:rerun-if-env-changed=DEP_UDM_UI_NO_DEFAULT_MANIFEST");
    // Set only on windows-gnu, and only with the window (feature tauri-window).
    if let Ok(dir) = std::env::var("DEP_UDM_UI_NO_DEFAULT_MANIFEST") {
        println!("cargo:rustc-link-arg-bin=udm-gui=-B{dir}/");
    }
}
