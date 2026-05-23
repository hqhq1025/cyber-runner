fn main() {
    #[cfg(target_os = "windows")]
    {
        // Embed the UAC manifest into the produced .exe so Windows shows a
        // single elevation prompt at launch instead of letting pymobiledevice3
        // fail later when it tries to open a USB tunnel.
        embed_manifest::embed_manifest_file("manifest.xml")
            .expect("failed to embed UAC manifest into cyber-runner-app binary");
        println!("cargo:rerun-if-changed=manifest.xml");
    }

    tauri_build::build();
}
