fn main() {
    let mut attributes = tauri_build::Attributes::new();

    #[cfg(target_os = "windows")]
    {
        let windows = tauri_build::WindowsAttributes::new()
            .app_manifest(include_str!("manifest.xml"));
        attributes = attributes.windows_attributes(windows);
    }

    if let Err(e) = tauri_build::try_build(attributes) {
        println!("{e:#}");
        std::process::exit(1);
    }
}
