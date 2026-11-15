//! User-facing errors. Every variant carries a stable `code` (for the UI to
//! pick an icon / action) and a Chinese message + hint that tells the user
//! exactly what to do next.

use idevice::IdeviceError;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("找不到苹果设备服务（usbmuxd）")]
    UsbmuxdUnavailable(String),
    #[error("没有检测到 iPhone / iPad")]
    NoDevice,
    #[error("设备已断开")]
    DeviceGone,
    #[error("手机处于锁屏状态")]
    DeviceLocked,
    #[error("等待在手机上点「信任」超时")]
    TrustTimeout,
    #[error("手机上选择了「不信任」")]
    TrustDenied,
    #[error("iOS {0} 不受支持，需要 iOS 17 及以上")]
    UnsupportedIos(String),
    #[error("未开启开发者模式")]
    DeveloperModeOff,
    #[error("开发者磁盘镜像下载失败：{0}")]
    DdiDownload(String),
    #[error("开发者磁盘镜像挂载失败：{0}")]
    DdiMount(String),
    #[error("建立设备隧道失败：{0}")]
    Tunnel(String),
    #[error("路线无效：{0}")]
    Route(#[from] cyber_runner_core::RouteError),
    #[error("已经在运行中")]
    Busy,
    #[error("引擎已退出")]
    EngineGone,
    #[error("和手机的连接出错了")]
    Device(String),
}

impl EngineError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UsbmuxdUnavailable(_) => "usbmuxd",
            Self::NoDevice => "no_device",
            Self::DeviceGone => "device_gone",
            Self::DeviceLocked => "locked",
            Self::TrustTimeout | Self::TrustDenied => "trust",
            Self::UnsupportedIos(_) => "unsupported_ios",
            Self::DeveloperModeOff => "developer_mode",
            Self::DdiDownload(_) => "ddi_download",
            Self::DdiMount(_) => "ddi_mount",
            Self::Tunnel(_) => "tunnel",
            Self::Route(_) => "route",
            Self::Busy => "busy",
            Self::EngineGone => "engine",
            Self::Device(_) => "device",
        }
    }

    pub fn hint(&self) -> &'static str {
        match self {
            Self::UsbmuxdUnavailable(_) => {
                if cfg!(windows) {
                    "Windows 需要苹果的设备驱动：从微软商店安装「Apple Devices」或 iTunes，装好后打开一次再重试。"
                } else {
                    "系统的 usbmuxd 服务没有响应。拔插一次数据线；仍不行请重启电脑。"
                }
            }
            Self::NoDevice | Self::DeviceGone => {
                "用数据线把手机连到电脑（要能传数据的线，不是仅充电线），解锁手机后再试。"
            }
            Self::DeviceLocked => "解锁手机，保持在主屏幕后再试。",
            Self::TrustTimeout | Self::TrustDenied => {
                "重新拔插数据线，手机弹出「要信任此电脑吗？」时点「信任」并输入锁屏密码。"
            }
            Self::UnsupportedIos(_) => "请把设备升级到 iOS 17 或更新版本。",
            Self::DeveloperModeOff => {
                "打开 设置 → 隐私与安全性 → 开发者模式（已为你唤出这个开关），按提示重启并输入密码，然后再点开始。"
            }
            Self::DdiDownload(_) => {
                "首次使用需要联网下载约 17 MB 的开发者磁盘镜像。已自动尝试国内镜像线路仍失败：检查网络后重试，或在设置里手动选择镜像文件夹。"
            }
            Self::DdiMount(_) => "保持手机解锁并联网（需要向 Apple 服务器签名），然后重试。重启手机通常也能解决。",
            Self::Tunnel(_) => "拔插数据线后重试；若持续失败，重启手机。",
            Self::Route(_) => "换一条路线，或检查导入的文件格式与坐标系。",
            Self::Busy => "先停止当前的跑步。",
            Self::EngineGone => "请重启 Cyber Runner。",
            Self::Device(_) => "用数据线连接手机（无线连接不行），解锁后重试。",
        }
    }

    /// Raw technical detail for the log (empty when the message already says it all).
    pub fn detail(&self) -> String {
        match self {
            Self::Device(d) => format!("（{d}）"),
            _ => String::new(),
        }
    }

    pub fn info(&self) -> ErrorInfo {
        ErrorInfo {
            code: self.code().into(),
            message: self.to_string(),
            hint: self.hint().into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
    pub hint: String,
}

impl From<IdeviceError> for EngineError {
    fn from(e: IdeviceError) -> Self {
        match e {
            IdeviceError::DeviceLocked | IdeviceError::PasswordProtected => Self::DeviceLocked,
            IdeviceError::UserDeniedPairing => Self::TrustDenied,
            IdeviceError::DeviceNotFound => Self::DeviceGone,
            other => Self::Device(other.to_string()),
        }
    }
}

pub type Result<T, E = EngineError> = std::result::Result<T, E>;
