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
/// Default order (re-ranked by a speed probe at download time). CDN / proxy
/// mirrors cover networks where GitHub is blocked or slow. All of them serve the
/// same bytes, and the device only mounts an image whose signature Apple's
/// TSS server issued for it, so a tampered mirror cannot get anything mounted.
const SOURCES: [&str; 6] = [
    "https://raw.githubusercontent.com/doronz88/DeveloperDiskImage/main/PersonalizedImages/Xcode_iOS_DDI_Personalized",
    "https://cdn.jsdelivr.net/gh/doronz88/DeveloperDiskImage@main/PersonalizedImages/Xcode_iOS_DDI_Personalized",
    "https://ghfast.top/https://raw.githubusercontent.com/doronz88/DeveloperDiskImage/main/PersonalizedImages/Xcode_iOS_DDI_Personalized",
    "https://gh-proxy.com/https://raw.githubusercontent.com/doronz88/DeveloperDiskImage/main/PersonalizedImages/Xcode_iOS_DDI_Personalized",
    "https://gcore.jsdelivr.net/gh/doronz88/DeveloperDiskImage@main/PersonalizedImages/Xcode_iOS_DDI_Personalized",
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

/// Give up on a mirror after this long without receiving any bytes.
const STALL: Duration = Duration::from_secs(12);
/// Size of the speed probe (start of Image.dmg).
const PROBE_BYTES: u64 = 256 * 1024;

fn host_of(base: &str) -> &str {
    base.split('/').nth(2).unwrap_or(base)
}

/// Order mirrors by how fast they deliver the first 256 KB. Mirrors that
/// fail or don't answer within the probe window go last (still tried, in
/// case the probe itself was unlucky). Speed differs wildly by network: on
/// one line jsDelivr's IPv6 route ran at 0.2 MB/s and its IPv4 route at
/// 13 MB/s, and GitHub itself is often unreachable from mainland China.
async fn rank_sources(client: &reqwest::Client) -> Vec<&'static str> {
    let mut set = tokio::task::JoinSet::new();
    for (i, base) in SOURCES.iter().enumerate() {
        let client = client.clone();
        set.spawn(async move {
            let ok = async {
                let resp = client
                    .get(format!("{base}/{}", FILES[0]))
                    .header("Range", format!("bytes=0-{}", PROBE_BYTES - 1))
                    .send()
                    .await
                    .ok()?
                    .error_for_status()
                    .ok()?;
                let mut resp = resp;
                let mut got = 0u64;
                while got < PROBE_BYTES {
                    match resp.chunk().await.ok()? {
                        Some(c) => got += c.len() as u64,
                        None => break,
                    }
                }
                (got > 0).then_some(())
            };
            (i, tokio::time::timeout(Duration::from_secs(8), ok).await)
        });
    }
    let mut fast = Vec::new();
    while let Some(r) = set.join_next().await {
        if let Ok((i, Ok(Some(())))) = r {
            fast.push(i);
            // Two responsive mirrors are enough to start; the rest keep
            // their default order as fallbacks.
            if fast.len() >= 2 {
                set.abort_all();
                break;
            }
        }
    }
    let mut order: Vec<_> = fast.iter().map(|&i| SOURCES[i]).collect();
    order.extend(
        SOURCES
            .iter()
            .enumerate()
            .filter(|(i, _)| !fast.contains(i))
            .map(|(_, b)| *b),
    );
    order
}

fn mb(b: u64) -> f64 {
    b as f64 / 1_048_576.0
}

/// Stream one file to `dest` (atomically via `.part`), reporting progress.
async fn fetch(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    label: &str,
    on_progress: &(dyn Fn(String) + Send + Sync),
) -> std::result::Result<(), String> {
    use tokio::io::AsyncWriteExt;
    let mut resp = tokio::time::timeout(STALL, client.get(url).send())
        .await
        .map_err(|_| "无响应".to_string())?
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?;
    let total = resp.content_length();
    let tmp = dest.with_extension("part");
    let mut out = tokio::fs::File::create(&tmp)
        .await
        .map_err(|e| e.to_string())?;
    let started = std::time::Instant::now();
    let mut last_report = started;
    let mut got = 0u64;
    loop {
        let chunk = tokio::time::timeout(STALL, resp.chunk())
            .await
            .map_err(|_| format!("{} 秒没有收到数据", STALL.as_secs()))?
            .map_err(|e| e.to_string())?;
        let Some(chunk) = chunk else { break };
        out.write_all(&chunk).await.map_err(|e| e.to_string())?;
        got += chunk.len() as u64;
        if last_report.elapsed() >= Duration::from_millis(400) {
            last_report = std::time::Instant::now();
            let speed = mb(got) / started.elapsed().as_secs_f64().max(0.001);
            on_progress(match total {
                Some(t) if t > 0 => format!(
                    "{label} {:.0}%（{:.1}/{:.1} MB · {speed:.1} MB/s）",
                    got as f64 * 100.0 / t as f64,
                    mb(got),
                    mb(t)
                ),
                _ => format!("{label} {:.1} MB（{speed:.1} MB/s）", mb(got)),
            });
        }
    }
    if got == 0 || total.is_some_and(|t| t != got) {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(format!("文件不完整（{got} 字节）"));
    }
    out.flush().await.map_err(|e| e.to_string())?;
    drop(out);
    tokio::fs::rename(&tmp, dest)
        .await
        .map_err(|e| e.to_string())
}

async fn download(
    cache_dir: &Path,
    on_progress: &(dyn Fn(String) + Send + Sync),
) -> Result<PathBuf> {
    tokio::fs::create_dir_all(cache_dir)
        .await
        .map_err(|e| EngineError::DdiDownload(e.to_string()))?;
    // IPv4 first: on real networks the IPv6 route to GitHub / CDNs is often
    // the bad one (measured: IPv6 connects and completes TLS, then stalls at
    // 0 bytes, while IPv4 to the same host runs at 7+ MB/s). Connect-time
    // happy-eyeballs can't catch a stall after connecting, so we pin IPv4
    // and fall back to "any family" only for IPv6-only networks.
    let build = |v4: bool| {
        let b = reqwest::Client::builder().connect_timeout(Duration::from_secs(6));
        let b = if v4 {
            b.local_address(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
        } else {
            b
        };
        b.build()
            .map_err(|e| EngineError::DdiDownload(e.to_string()))
    };
    let clients = [build(true)?, build(false)?];
    on_progress("准备下载开发者磁盘镜像（约 17 MB，仅首次）：选择最快线路…".into());
    let order = rank_sources(&clients[0]).await;
    let mut last_err = String::new();
    for (ci, client) in clients.iter().enumerate() {
        'sources: for base in &order {
            let host = host_of(base);
            for file in FILES {
                let dest = cache_dir.join(file);
                if dest.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                    continue;
                }
                let label = format!("下载开发者磁盘镜像 · {host}");
                on_progress(format!("{label}…"));
                if let Err(e) = fetch(
                    client,
                    &format!("{base}/{file}"),
                    &dest,
                    &label,
                    on_progress,
                )
                .await
                {
                    let fam = if ci == 0 { "IPv4" } else { "any" };
                    tracing::warn!("DDI download {file} from {host} ({fam}) failed: {e}");
                    last_err = format!("{host}：{e}");
                    continue 'sources;
                }
            }
            if has_all(cache_dir) {
                return Ok(cache_dir.to_path_buf());
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "network: cargo test -p cyber-runner-engine ddi_download -- --ignored --nocapture"]
    async fn ddi_download() {
        let dir = std::env::temp_dir().join(format!("cr-ddi-{}", uuid::Uuid::new_v4()));
        let t = std::time::Instant::now();
        let got = download(&dir, &|m| eprintln!("{m}")).await.unwrap();
        assert!(has_all(&got));
        eprintln!("done in {:.1}s", t.elapsed().as_secs_f64());
        let _ = std::fs::remove_dir_all(dir);
    }
}
