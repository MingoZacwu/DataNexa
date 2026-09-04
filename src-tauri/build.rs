fn main() {
    println!("cargo:rerun-if-changed=../jdbc-sidecar/src");
    println!("cargo:rerun-if-changed=../jdbc-sidecar/pom.xml");
    println!("cargo:rerun-if-changed=../resources/jdbc-runtime");
    tauri_build::build();

    #[cfg(target_os = "macos")]
    cc::Build::new()
        .file("bridge.m")
        .flag("-fobjc-arc")
        .compile("datanexa_macos_login_item");

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=ServiceManagement");
        println!("cargo:rustc-link-lib=framework=ApplicationServices");
        println!("cargo:rustc-link-search=framework=/System/Library/Frameworks");
    }
}
