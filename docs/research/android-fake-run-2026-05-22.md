# Android 虚拟校园跑方案调研

> 调研日期：2026-05-22
> 目的：评估 Android 侧成熟方案，探索与本项目（iOS Tauri fake-run）的结合可能性

---

## 1. 执行摘要

Android 侧已形成以"ADB Mock Location（免 root）"为主流、"LSPosed/Xposed Hook（需 root）"为补充的双轨方案，均可对接步道乐跑、闪动校园等校园跑 APP。结合路线可行，**推荐走架构 B：在现有 Tauri 桌面端增加 Android 子模块，通过 `adb` 喂假 GPS**——与 iOS 侧的 `pymobiledevice3` 方案对称，共用核心路径算法，改动范围最小，交付最快。Android 子模块无需在手机上安装任何 APK（仅开启开发者模式），与 iOS 方案的使用门槛基本持平。传感器（加速度/步频）的模拟是 Android 侧相比 iOS 侧的额外复杂度，需要借助 Xposed 层或提前录制真实传感器数据回放。

---

## 2. 候选项目清单

| # | 仓库 | 最近 commit | Stars | 主语言 | 绕过方式 |
|---|------|-------------|-------|--------|----------|
| 1 | [zhufucdev/MotionEmulator](https://github.com/zhufucdev/MotionEmulator) | 2024-03-08 | 400 | Kotlin | LSPosed Xposed Hook；同时支持 Android 调试 API（免 root）；通过 WebSocket + ProtoBuf 插件实时喂位置 + 传感器数据；专为校园跑设计 |
| 2 | [ZCShou/GoGoGo（影梭）](https://github.com/ZCShou/GoGoGo) | 2025-08-09 | 10.2k | Java | Android 调试 API（`LocationManager` mock，无 root）；百度地图 SDK；摇杆控制移动速度；不绕过 mock 标志，校园跑 APP 若检测 `isFromMockProvider` 会被识别 |
| 3 | [noobexon1/XposedFakeLocation](https://github.com/noobexon1/XposedFakeLocation) | 2025-10-24 | 457 | Kotlin | LSPosed Hook `LocationManager`；强制将 `isFromMockProvider` 置 false；支持自定义海拔、精度、传感器；Android 11+ |
| 4 | [cxOrz/AnyWhere](https://github.com/cxOrz/AnyWhere) | 2026-03-22 | 356 | Java | 无 root 时走调试 API；可选启用 LSPosed 模块隐藏 mock 标志；基于 GoGoGo 重构，换用 OpenStreetMap，持续维护 |
| 5 | [LangQi99/dlut-FakeRun](https://github.com/LangQi99/dlut-FakeRun) | 2026-03-05 | 60 | Java | 调试 API 免 root；内置 KML 路径；支持真机和 BlueStacks 模拟器；校园跑专用，含速度控制 |
| 6 | [LiuYiGL/RunWorldSchoolMod](https://github.com/LiuYiGL/RunWorldSchoolMod) | 2025-11-05 | — | Java | 直接 Hook 运动世界校园 APP；修改点位检测逻辑（有序/无序点位）、关闭夜间限制、跳过 GPS 信号检测；LSPosed/LSPatch，无需源码重签 |
| 7 | [Entity-Now/WJFakeLocation](https://github.com/Entity-Now/WJFakeLocation) | 2026-03-09 | — | Kotlin | LSPosed Hook；支持 GCJ-02/WGS-84/BD-09 三坐标互转；基站/WiFi 信息伪造；Android 13–16；含 KML 路径导入 |
| 8 | [limejuiceOwO/CloudRunner](https://github.com/limejuiceOwO/CloudRunner) | 2023-09-15 | 1 | Java | Xposed Hook；同时模拟 GPS + 加速度计 + 步数计；正弦波加速度曲线；仅支持 Android 9 及以下，已停维护，有参考价值 |

---

## 3. 反作弊维度对比表

| 方案 | 传感器加速度 | 心率 | 运动状态（步频） | 轨迹平滑 | 海拔 | 配速波动 | 隐藏 mock 标志 |
|------|:-----------:|:----:|:---------------:|:--------:|:----:|:-------:|:-------------:|
| MotionEmulator (Xposed 插件) | 支持 | 不支持 | 支持（录制回放） | 支持路径插值 | 支持 | 可配置 | 是 |
| GoGoGo / AnyWhere（调试 API） | 不支持 | 不支持 | 不支持 | 摇杆手动控速 | 不支持 | 手动 | 否 |
| XposedFakeLocation | 部分（自定义传感器） | 不支持 | 不支持 | 随机半径偏移 | 支持 | 不支持 | 是 |
| dlut-FakeRun | 不支持 | 不支持 | 不支持 | KML 路径 | 不支持 | 固定速度 | 否（调试 API） |
| WJFakeLocation | 不支持 | 不支持 | 不支持 | KML + 随机偏移 | 支持 | 可配置 | 是 |
| RunWorldSchoolMod（APP Hook） | 不适用 | 不适用 | 绕过检测 | 绕过点位验证 | 不适用 | 不适用 | 绕过 APP 逻辑 |
| CloudRunner（Xposed，旧） | 支持（正弦波） | 不支持 | 支持 | 自动生成路径 | 不支持 | 随机波动 | 是 |

> 注：步道乐跑等 APP 主要检测 `isFromMockProvider` 标志和步频一致性；乐动力额外校验加速度曲线。心率在主流校园跑 APP 中非必检项。

---

## 4. 三种结合架构

### 架构 A：完全独立的 Android 客户端

单独开发一个 Android APK，用户装在手机上，手机独立运行假跑。UI 和算法全部在安卓端，不依赖桌面端。

- **优点**：无需连电脑，最终用户体验最简单；可以直接访问传感器 API，传感器模拟最彻底。
- **缺点**：与 iOS 侧代码完全独立，维护两套；路径算法（BD-09 转换、平滑插值）需重写为 Kotlin/Java；对于需要 LSPosed 的功能，仍然要求 root，门槛未必低于 iOS 方案。项目规模最大。

---

### 架构 B：Tauri 应用扩展 Android 子模块，ADB 控制连接的安卓设备

在现有 Tauri 桌面端新增一个 Android 控制模块，通过 `adb shell` 调用 `am start-activity` 加 intent 参数，或直接通过 ADB 的 `appops set <pkg> android:mock_location allow` 授权加自研 APK 代理，让桌面端喂坐标给安卓设备。APK 代理非常轻量（仅负责接收 TCP/ADB 坐标流并调用 `LocationManager.setTestProviderLocation`），无需 root。

- **优点**：与 iOS 侧方案高度对称（iOS 用 pymobiledevice3 via USB，Android 用 adb via USB）；桌面端统一管理路径、速度、坐标系，不重复造轮子；用户体验一致（插线，点开始）；核心算法复用最彻底。
- **缺点**：传感器模拟受限（调试 API 无法 hook 加速度计），对检测步频的 APP（如乐动力高检测版）效果弱于 LSPosed 方案；需要在安卓设备上安装一个轻量代理 APK；若 APP 检测 `isFromMockProvider`，需配合 LSPosed 或重新分析绕过方式。

---

### 架构 C：抽出核心算法库，双端 UI 各做各的

将 `fake-run-core`（Rust crate）中的 BD-09/GCJ-02/WGS-84 坐标转换、路径平滑、速度波动算法发布为独立 crate 或 FFI 库，iOS 侧的 Tauri 桌面端和未来的 Android 端各自调用，UI 完全独立。

- **优点**：算法高度复用，正确性保证在一处；Rust 的 crate 也可被 Android 通过 JNI/UniFFI 调用，做纯 native 运算。
- **缺点**：架构最重，前期工程量最大；两套 UI 需独立迭代；对于当前阶段属于过度设计，适合产品稳定后的长期演进，不适合短期交付。

---

## 5. 推荐路线

**推荐架构 B**。在 Tauri 桌面端新增 `android` 子模块，通过 ADB 驱动一个轻量安卓代理 APK 喂假 GPS，与 iOS 侧（pymobiledevice3 + USB）完全对称。核心路径算法无需改动，只需在 Rust 侧增加 `adb` 命令封装。

**立即可执行的下一步**：clone `LangQi99/dlut-FakeRun`，在本地编译并确认 ADB mock location 在步道乐跑上跑通；然后将其 `LocationManager.setTestProviderLocation` 调用剥离为一个极简 TCP socket 代理，供 Tauri 端通过 `adb forward` 推送坐标序列。

---

## 6. 风险与法律灰区

- **学术诚信**：虚拟校园跑直接违反学校体育必修课诚信要求，被查实可能记违纪处分，部分学校列入综测扣分项。
- **APP 服务条款**：步道乐跑、乐动力、运动世界校园均在 ToS 中明确禁止虚假数据提交，账号可能被封禁或成绩清零。
- **设备 root 风险**：刷入 Magisk/LSPosed 会使手机保修失效，部分机型（如华为鸿蒙）root 后无法回退；root 后系统安全边界降低，银行 APP 等可能拒绝运行。
- **ADB 免 root 方案相对安全**，但仍有被 APP 端检测到 `isFromMockProvider` 的风险；若 APP 加固（梆加固、腾讯乐固）并检测 ADB/调试状态，绕过复杂度显著上升。
- **分发灰区**：将绕过工具以 GitHub 公开分发，在中国境内存在一定法律模糊性（《网络安全法》第 27 条对"专门用于从事危害网络安全活动的工具"的限制，解读空间较大）。
- **本项目定位**：若以"开发者测试工具"定位，风险相对可控；若明确宣传"刷校园跑学分"，法律风险和平台封禁风险均会提升。

---

## 引用 URL

1. https://github.com/zhufucdev/MotionEmulator — 传感器 + 位置模拟双模，校园跑专用
2. https://www.zhufucdev.com/article/G1lNhmtzI5-RQnVmYEbXm — MotionEmulator 官方使用文档
3. https://github.com/ZCShou/GoGoGo — 影梭，10k star，ADB 调试 API，免 root
4. https://github.com/noobexon1/XposedFakeLocation — LSPosed，隐藏 mock 标志，Android 11+
5. https://github.com/cxOrz/AnyWhere — GoGoGo 重构版，2026 年仍在维护
6. https://github.com/LangQi99/dlut-FakeRun — 校园跑专用，KML 路径，免 root
7. https://github.com/LiuYiGL/RunWorldSchoolMod — 直接 Hook 运动世界校园 APP
8. https://github.com/Entity-Now/WJFakeLocation — Android 13-16，含坐标系转换
9. https://github.com/limejuiceOwO/CloudRunner — 加速度计 + GPS 联合模拟（已停维护，参考）
10. https://github.com/Ac-Wiki/Ac-Wiki/blob/1f7682141862a71ab5e7fee48b368898342c7f3c/docs/03-tools/qi-ji-yin-qiao/campus-running.md — 步道乐跑虚拟机+影梭方案 wiki
11. https://github.com/markzhang12345/MockLocation — ADB 授权 mock，Android 14 测试
12. https://github.com/Manabu-GT/android-mock-location-mcp — ADB + MCP 方式桌面端控制安卓 GPS（与架构 B 思路最接近的参考实现）
