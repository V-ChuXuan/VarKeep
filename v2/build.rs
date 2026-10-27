fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/branding/varkeep.ico")
            .set_manifest_file("app.manifest")
            .compile()
            .expect("failed to compile VarKeep Windows resources");
    }
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=assets/branding/varkeep.ico");
}
