fn main() {
    println!("cargo:rerun-if-changed=resources/windows/omniview.rc");
    println!("cargo:rerun-if-changed=assets/icons/omniview.ico");
    println!("cargo:rerun-if-changed=assets/icons/omniview.svg");

    #[cfg(target_os = "windows")]
    {
        // GPUI loads resource ID 1 as the app/window icon on Windows.
        embed_resource::compile("resources/windows/omniview.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("embed Windows icon resource");
    }
}
