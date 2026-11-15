//! Device discovery, trust/pairing and pre-flight checks over usbmuxd.
//!
//! No root / Administrator needed anywhere: macOS and Linux talk to the
//! usbmuxd socket, Windows talks to Apple Mobile Device Service on
//! 127.0.0.1:27015 (installed by "Apple Devices" or iTunes).

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use idevice::{
    amfi::AmfiClient,
    lockdown::LockdownClient,
    pairing_file::PairingFile,
    provider::{IdeviceProvider, UsbmuxdProvider},
    usbmuxd::{Connection, UsbmuxdAddr, UsbmuxdConnection},
    Idevice, IdeviceError, IdeviceService,
};
use serde::Serialize;

use crate::error::{EngineError, Result};

pub const LABEL: &str = "cyber-runner";
const PAIR_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub udid: String,
    /// `usb` or `network`.
    pub connection: String,
    pub name: Option<String>,
    pub product_type: Option<String>,
    pub ios_version: Option<String>,
    /// Whether this computer already has a trust record for the device.
    pub trusted: bool,
}

impl DeviceInfo {
    pub fn ios_major(&self) -> Option<u32> {
        self.ios_version.as_deref().and_then(ios_major)
    }

    pub fn display_name(&self) -> String {
        self.name
            .clone()
            .or_else(|| self.product_type.clone())
            .unwrap_or_else(|| format!("设备 {}", &self.udid[..self.udid.len().min(8)]))
    }
}

/// A usbmuxd connection plus a pairing record we resolved ourselves.
///
/// `UsbmuxdProvider` always asks usbmuxd for the pair record. That's fine on
/// macOS, but on Windows Apple Mobile Device Service may refuse to hand it
/// out (or never stored one if iTunes was not used for pairing). We resolve
/// the record once — usbmuxd, then our own copy, then the system Lockdown
/// folder — and serve it from memory.
#[derive(Debug, Clone)]
pub struct DeviceProvider {
    pub inner: UsbmuxdProvider,
    pairing: PairingFile,
}

impl DeviceProvider {
    pub fn udid(&self) -> &str {
        &self.inner.udid
    }
}

impl IdeviceProvider for DeviceProvider {
    fn connect(
        &self,
        port: u16,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Idevice, IdeviceError>> + Send>> {
        self.inner.connect(port)
    }

    fn label(&self) -> &str {
        self.inner.label()
    }

    fn get_pairing_file(
        &self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<PairingFile, IdeviceError>> + Send>> {
        let p = self.pairing.clone();
        Box::pin(async move { Ok(p) })
    }
}

fn cached_record_path(pair_dir: &Path, udid: &str) -> Option<PathBuf> {
    // UDIDs are hex + dashes; refuse anything else so it can't escape the dir.
    if udid.is_empty() || !udid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    Some(pair_dir.join(format!("{udid}.plist")))
}

fn system_record_paths(udid: &str) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if cfg!(windows) {
        if let Ok(pd) = std::env::var("ProgramData") {
            v.push(
                PathBuf::from(pd)
                    .join("Apple")
                    .join("Lockdown")
                    .join(format!("{udid}.plist")),
            );
        }
    } else if cfg!(target_os = "linux") {
        v.push(PathBuf::from(format!("/var/lib/lockdown/{udid}.plist")));
    }
    v
}

/// Look up an existing pairing record without user interaction.
pub async fn find_pairing(provider: &UsbmuxdProvider, pair_dir: &Path) -> Option<PairingFile> {
    if let Ok(pf) = provider.get_pairing_file().await {
        return Some(pf);
    }
    let mut paths: Vec<PathBuf> = cached_record_path(pair_dir, &provider.udid)
        .into_iter()
        .collect();
    paths.extend(system_record_paths(&provider.udid));
    for p in paths {
        if let Ok(bytes) = tokio::fs::read(&p).await {
            if let Ok(pf) = PairingFile::from_bytes(&bytes) {
                return Some(pf);
            }
        }
    }
    None
}

/// Numeric iOS major version ("17.4.1" → 17). The old Python code compared
/// strings, so "9" > "17".
pub fn ios_major(version: &str) -> Option<u32> {
    version.split('.').next()?.trim().parse().ok()
}

async fn usbmuxd() -> Result<UsbmuxdConnection> {
    let addr = UsbmuxdAddr::from_env_var().unwrap_or_default();
    addr.connect(1)
        .await
        .map_err(|e| EngineError::UsbmuxdUnavailable(e.to_string()))
}

/// All attached devices, USB first.
pub async fn list_devices(pair_dir: &Path) -> Result<Vec<DeviceInfo>> {
    list_devices_cached(&mut Default::default(), pair_dir).await
}

/// Like [`list_devices`] but reuses name/version lookups from `cache`.
pub async fn list_devices_cached(
    cache: &mut std::collections::HashMap<String, DeviceInfo>,
    pair_dir: &Path,
) -> Result<Vec<DeviceInfo>> {
    let mut mux = usbmuxd().await?;
    let mut devices = mux
        .get_devices()
        .await
        .map_err(|e| EngineError::UsbmuxdUnavailable(e.to_string()))?;
    devices.sort_by_key(|d| !matches!(d.connection_type, Connection::Usb));
    let mut out: Vec<DeviceInfo> = Vec::new();
    for d in devices {
        // The same phone can show up both over USB and Wi-Fi.
        if out.iter().any(|o| o.udid == d.udid) {
            continue;
        }
        let provider = d.to_provider(UsbmuxdAddr::from_env_var().unwrap_or_default(), LABEL);
        let trusted = find_pairing(&provider, pair_dir).await.is_some();
        let connection = match d.connection_type {
            Connection::Usb => "usb",
            _ => "network",
        }
        .to_string();
        let info = match cache.get(&d.udid) {
            Some(c) if c.trusted == trusted && c.name.is_some() => DeviceInfo {
                connection,
                ..c.clone()
            },
            _ => {
                let (name, product_type, ios_version) = basic_values(&provider).await;
                DeviceInfo {
                    udid: d.udid.clone(),
                    connection,
                    name,
                    product_type,
                    ios_version,
                    trusted,
                }
            }
        };
        cache.insert(d.udid.clone(), info.clone());
        out.push(info);
    }
    cache.retain(|k, _| out.iter().any(|o| &o.udid == k));
    Ok(out)
}

async fn basic_values(
    provider: &UsbmuxdProvider,
) -> (Option<String>, Option<String>, Option<String>) {
    let Ok(mut lockdown) = LockdownClient::connect(provider).await else {
        return (None, None, None);
    };
    let name = string_value(&mut lockdown, "DeviceName").await;
    let product = string_value(&mut lockdown, "ProductType").await;
    let version = string_value(&mut lockdown, "ProductVersion").await;
    (name, product, version)
}

async fn string_value(lockdown: &mut LockdownClient, key: &'static str) -> Option<String> {
    lockdown
        .get_value(Some(key), None)
        .await
        .ok()
        .and_then(|v| v.as_string().map(str::to_owned))
}

/// Find the requested device (or the first USB one).
pub async fn provider_for(udid: Option<&str>) -> Result<UsbmuxdProvider> {
    let mut mux = usbmuxd().await?;
    let addr = UsbmuxdAddr::from_env_var().unwrap_or_default();
    if let Some(udid) = udid {
        return match mux.get_device(udid).await {
            Ok(d) => Ok(d.to_provider(addr, LABEL)),
            Err(_) => Err(EngineError::DeviceGone),
        };
    }
    let mut devices = mux
        .get_devices()
        .await
        .map_err(|e| EngineError::UsbmuxdUnavailable(e.to_string()))?;
    devices.sort_by_key(|d| !matches!(d.connection_type, Connection::Usb));
    devices
        .first()
        .map(|d| d.to_provider(addr, LABEL))
        .ok_or(EngineError::NoDevice)
}

/// Resolve (or create, by asking the user to tap "Trust") the pairing
/// record and return a provider that always has it.
pub async fn ensure_trusted(
    provider: UsbmuxdProvider,
    pair_dir: &Path,
    on_wait: impl FnOnce(),
) -> Result<DeviceProvider> {
    if let Some(pairing) = find_pairing(&provider, pair_dir).await {
        return Ok(DeviceProvider {
            inner: provider,
            pairing,
        });
    }
    on_wait();
    let mut mux = usbmuxd().await?;
    let buid = mux.get_buid().await.map_err(EngineError::from)?;
    let mut lockdown = LockdownClient::connect(&provider).await?;
    let host_id = uuid::Uuid::new_v4().to_string().to_uppercase();
    let pf = tokio::time::timeout(
        PAIR_TIMEOUT,
        lockdown.pair(host_id, buid, Some("Cyber Runner")),
    )
    .await
    .map_err(|_| EngineError::TrustTimeout)?
    .map_err(|e| match e {
        IdeviceError::UserDeniedPairing => EngineError::TrustDenied,
        IdeviceError::PasswordProtected | IdeviceError::DeviceLocked => EngineError::DeviceLocked,
        e => EngineError::Device(format!("配对失败：{e}")),
    })?;
    let bytes = pf
        .clone()
        .serialize()
        .map_err(|e| EngineError::Device(format!("保存配对记录失败：{e}")))?;
    // Keep our own copy first — it's what makes Windows work even when
    // Apple Mobile Device Service won't store or return records.
    if let Some(path) = cached_record_path(pair_dir, &provider.udid) {
        let _ = tokio::fs::create_dir_all(pair_dir).await;
        if let Err(e) = tokio::fs::write(&path, &bytes).await {
            tracing::warn!("could not cache pair record: {e}");
        }
    }
    // Also hand it to usbmuxd so other tools (and Wi-Fi sync) see it.
    if let Err(e) = mux.save_pair_record(&provider.udid, bytes).await {
        tracing::info!("usbmuxd did not accept the pair record ({e}); using our own copy");
    }
    Ok(DeviceProvider {
        inner: provider,
        pairing: pf,
    })
}

/// Facts we need before starting a session.
#[derive(Debug, Clone)]
pub struct Preflight {
    pub info: DeviceInfo,
    pub unique_chip_id: u64,
}

pub async fn preflight(provider: &DeviceProvider) -> Result<Preflight> {
    let mut lockdown = LockdownClient::connect(provider).await?;
    lockdown.start_session(&provider.pairing).await?;
    let version = string_value(&mut lockdown, "ProductVersion")
        .await
        .unwrap_or_default();
    let name = string_value(&mut lockdown, "DeviceName").await;
    let product = string_value(&mut lockdown, "ProductType").await;
    let ucid = lockdown
        .get_value(Some("UniqueChipID"), None)
        .await?
        .as_unsigned_integer()
        .ok_or_else(|| EngineError::Device("读取 UniqueChipID 失败".into()))?;

    if ios_major(&version).is_none_or(|m| m < 17) {
        return Err(EngineError::UnsupportedIos(version));
    }

    let mut amfi = AmfiClient::connect(provider).await?;
    if !amfi.get_developer_mode_status().await? {
        // Make the toggle visible in Settings so the user can find it.
        let _ = amfi.reveal_developer_mode_option_in_ui().await;
        return Err(EngineError::DeveloperModeOff);
    }

    Ok(Preflight {
        info: DeviceInfo {
            udid: provider.inner.udid.clone(),
            connection: "usb".into(),
            name,
            product_type: product,
            ios_version: Some(version),
            trusted: true,
        },
        unique_chip_id: ucid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_record_path_rejects_traversal() {
        let d = Path::new("/tmp/x");
        assert!(cached_record_path(d, "00008110-001A2B3C4D5E").is_some());
        assert!(cached_record_path(d, "../../etc/passwd").is_none());
        assert!(cached_record_path(d, "").is_none());
    }

    #[test]
    fn ios_major_is_numeric() {
        assert_eq!(ios_major("17.4.1"), Some(17));
        assert_eq!(ios_major("26.0"), Some(26));
        assert_eq!(ios_major("9.3"), Some(9));
        assert_eq!(ios_major(""), None);
        assert!(ios_major("9.3").unwrap() < 17);
    }
}
