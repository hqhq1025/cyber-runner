//! Personalized Developer Disk Image (iOS 17+).
//!
//! DVT services (location simulation lives there) only exist once the DDI
//! is mounted. The old app never mounted it — it silently relied on Xcode
//! having done so since the phone's last reboot. We mount it automatically:
//! the image is looked up locally (cache → Xcode install → user folder) and
//! otherwise downloaded once (~17 MB) and cached. Mounting requires a
//! signature from Apple's TSS server, which `idevice` fetches.

use std::path::{Path, PathBuf};
use std::time::Duration;

use idevice::{mobile_image_mounter::ImageMounter, IdeviceService};

use crate::device::DeviceProvider;
use crate::error::{EngineError, Result};

const FILES: [&str; 3] = ["Image.dmg", "Image.dmg.trustcache", "BuildManifest.plist"];
const SOURCES: [&str; 2] = [
    "https://raw.githubusercontent.com/doronz88/DeveloperDiskImage/main/PersonalizedImages/Xcode_iOS_DDI_Personalized",
    "https://github.com/doronz88/DeveloperDiskImage/raw/main/PersonalizedImages/Xcode_iOS_DDI_Personalized",
];

/// Is a personalized image already mounted?
pub async fn is_mounted(provider: &DeviceProvider) -> Result<bool> {
    let mut mounter = ImageMounter::connect(provider).await?;
    let devices = mounter.copy_devices().await.unwrap_or_default();
    if !devices.is_empty() {
        return Ok(true);
    }
    Ok(mounter
        .lookup_image("Personalized")
        .await
        .map(|sig| !sig.is_empty())
        .unwrap_or(false))
}

fn has_all(dir: &Path) -> bool {
    FILES
        .iter()
        .all(|f| dir.join(f).metadata().map(|m| m.len() > 0).unwrap_or(false))
}

/// Locate a usable image directory without network.
pub fn find_local(cache_dir: &Path, extra: Option<&Path>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(e) = extra {
        candidates.push(e.to_path_buf());
    }
    candidates.push(cache_dir.to_path_buf());
    // Xcode 15+ keeps the personalized image here on macOS.
    candidates.push("/Library/Developer/DeveloperDiskImages/iOS_DDI/Restore".into());
    candidates.into_iter().find(|d| has_all(d))
}

async fn download(
    cache_dir: &Path,
    on_progress: &(dyn Fn(String) + Send + Sync),
) -> Result<PathBuf> {
    tokio::fs::create_dir_all(cache_dir)
        .await
        .map_err(|e| EngineError::DdiDownload(e.to_string()))?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| EngineError::DdiDownload(e.to_string()))?;
    let mut last_err = String::new();
    'sources: for base in SOURCES {
        for file in FILES {
            let dest = cache_dir.join(file);
            if dest.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                continue;
            }
            on_progress(format!("下载开发者磁盘镜像：{file}"));
            let res = async {
                let resp = client
                    .get(format!("{base}/{file}"))
                    .send()
                    .await?
                    .error_for_status()?;
                resp.bytes().await
            }
            .await;
            match res {
                Ok(bytes) if !bytes.is_empty() => {
                    // Write atomically so an interrupted download never
                    // leaves a truncated file that looks complete.
                    let tmp = cache_dir.join(format!("{file}.part"));
                    let w = async {
                        tokio::fs::write(&tmp, &bytes).await?;
                        tokio::fs::rename(&tmp, &dest).await
                    };
                    if let Err(e) = w.await {
                        return Err(EngineError::DdiDownload(e.to_string()));
                    }
                }
                Ok(_) => {
                    last_err = format!("{file} 为空");
                    continue 'sources;
                }
                Err(e) => {
                    last_err = e.to_string();
                    continue 'sources;
                }
            }
        }
        if has_all(cache_dir) {
            return Ok(cache_dir.to_path_buf());
        }
    }
    Err(EngineError::DdiDownload(last_err))
}

/// Make sure the DDI is mounted, downloading it if needed.
pub async fn ensure_mounted(
    provider: &DeviceProvider,
    unique_chip_id: u64,
    cache_dir: &Path,
    extra_dir: Option<&Path>,
    on_progress: &(dyn Fn(String) + Send + Sync),
) -> Result<()> {
    if is_mounted(provider).await? {
        return Ok(());
    }
    let dir = match find_local(cache_dir, extra_dir) {
        Some(d) => d,
        None => download(cache_dir, on_progress).await?,
    };
    on_progress("挂载开发者磁盘镜像（向 Apple 请求签名）…".into());
    let read = |f: &str| {
        let p = dir.join(f);
        async move {
            tokio::fs::read(&p)
                .await
                .map_err(|e| EngineError::DdiMount(e.to_string()))
        }
    };
    let image = read(FILES[0]).await?;
    let trust_cache = read(FILES[1]).await?;
    let manifest = read(FILES[2]).await?;
    let mut mounter = ImageMounter::connect(provider).await?;
    mounter
        .mount_personalized(
            provider,
            image,
            trust_cache,
            &manifest,
            None,
            unique_chip_id,
        )
        .await
        .map_err(|e| {
            // A corrupt cached image would fail forever; drop it so the next
            // attempt re-downloads.
            if dir == cache_dir {
                for f in FILES {
                    let _ = std::fs::remove_file(cache_dir.join(f));
                }
            }
            EngineError::DdiMount(e.to_string())
        })?;
    Ok(())
}
