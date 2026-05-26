import logging
import multiprocessing

from pymobiledevice3.lockdown import create_using_usbmux, LockdownClient

from pymobiledevice3.cli.remote import RemoteServiceDiscoveryService
from pymobiledevice3.cli.remote import start_tunnel
from pymobiledevice3.cli.remote import verify_tunnel_imports

from pymobiledevice3.services.amfi import AmfiService

from pymobiledevice3.exceptions import NoDeviceConnectedError

def get_usbmux_lockdownclient():
    try:
        lockdown = create_using_usbmux()
    except NoDeviceConnectedError as exc:
        raise RuntimeError("未检测到 iPhone/iPad，请连接设备并信任这台电脑后再开始。") from exc

    if lockdown.all_values.get("PasswordProtected"):
        raise RuntimeError("设备当前已锁定，请先解锁 iPhone/iPad 后再开始。")

    return create_using_usbmux()

def get_version(lockdown: LockdownClient):
    return lockdown.all_values.get("ProductVersion")

def get_developer_mode_status(lockdown: LockdownClient):
    return lockdown.developer_mode_status

def reveal_developer_mode(lockdown: LockdownClient):
    AmfiService(lockdown).reveal_developer_mode_option_in_ui()

def enable_developer_mode(lockdown: LockdownClient):
    AmfiService(lockdown).enable_developer_mode()

async def tunnel(rsd: RemoteServiceDiscoveryService, queue: multiprocessing.Queue):
    async with start_tunnel(rsd, None) as tunnel_result:
        queue.put((tunnel_result.address, tunnel_result.port))
        await tunnel_result.client.wait_closed()
