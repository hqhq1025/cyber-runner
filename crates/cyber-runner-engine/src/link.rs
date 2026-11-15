//! The live connection to the device's location simulation service:
//! CoreDeviceProxy (over lockdown) → in-process TCP stack → RSD → DVT.
//!
//! This replaces `pymobiledevice3 lockdown start-tunnel`, which needed a TUN
//! interface and therefore root on macOS / Administrator on Windows.

use std::time::Duration;

use idevice::{
    core_device_proxy::CoreDeviceProxy, dvt::remote_server::RemoteServerClient, rsd::RsdHandshake,
    tcp::handle::AdapterHandle, IdeviceService, ReadWrite, RsdService,
};

use crate::device::DeviceProvider;
use crate::error::{EngineError, Result};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

pub struct Link {
    // Must outlive `server`: it drives the userspace TCP stack.
    _adapter: AdapterHandle,
    pub server: RemoteServerClient<Box<dyn ReadWrite>>,
}

pub async fn open(provider: &DeviceProvider) -> Result<Link> {
    tokio::time::timeout(CONNECT_TIMEOUT, open_inner(provider))
        .await
        .map_err(|_| EngineError::Tunnel("连接超时".into()))?
}

async fn open_inner(provider: &DeviceProvider) -> Result<Link> {
    let tunnel_err =
        |what: &str, e: &dyn std::fmt::Display| EngineError::Tunnel(format!("{what}: {e}"));
    let proxy = CoreDeviceProxy::connect(provider)
        .await
        .map_err(|e| tunnel_err("CoreDeviceProxy", &e))?;
    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let adapter = proxy
        .create_software_tunnel()
        .map_err(|e| tunnel_err("software tunnel", &e))?;
    let mut adapter = adapter.to_async_handle();
    let stream = adapter
        .connect(rsd_port)
        .await
        .map_err(|e| tunnel_err("RSD connect", &e))?;
    let mut handshake = RsdHandshake::new(stream)
        .await
        .map_err(|e| tunnel_err("RSD handshake", &e))?;
    let mut server =
        RemoteServerClient::<Box<dyn ReadWrite>>::connect_rsd(&mut adapter, &mut handshake)
            .await
            .map_err(|e| {
                EngineError::Tunnel(format!("找不到 DVT 服务（开发者磁盘镜像可能未挂载）: {e}"))
            })?;
    server
        .read_message(0)
        .await
        .map_err(|e| tunnel_err("DVT handshake", &e))?;
    Ok(Link {
        _adapter: adapter,
        server,
    })
}
