fn main() {
    slint_build::compile("ui/app.slint").expect("failed to compile Slint UI");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/branding/varkeep.ico")
            .compile()
            .expect("failed to compile VarKeep Windows resources");
    }
}
