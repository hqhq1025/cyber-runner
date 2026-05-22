# 微信小程序定位绕过研究报告
**日期**：2026-05-22  
**场景**：北邮校园跑小程序（步道乐跑改版）/ OPPO PJW110 ColorOS V16 Android 16  
**目标**：让 `wx.getLocation()` 读到桌面端通过 `adb` 喂的假 GPS，而不是腾讯 SDK 反查的真实位置

---

## 1. `wx.getLocation()` 内部实现

### 调用链

```
JS: wx.getLocation()
  → 小程序框架（Webview/V8 引擎）
  → 原生 Bridge → com.tencent.mm:appbrand0（独立子进程）
  → com.tencent.map.geolocation.sapp.TencentLocationManager
      .requestSingleFreshLocation(request, listener, looper, bool)
  → TencentLocationListener.onLocationChanged(TencentLocation, int, String)
  → TencentLocation.getLatitude() / .getLongitude()
```

**关键事实**（来自腾讯云社区逆向分析文章 [1][2]）：

- 小程序运行在独立进程 `com.tencent.mm:appbrand0`（编号可能变化），**不在微信主进程**
- SDK 类名 `com.tencent.map.geolocation.sapp.TencentLocationManager` 未混淆，版本稳定
- 腾讯 SDK 优先级：WiFi MAC + 基站 ID → 发到腾讯 LBS 服务器（`apis.map.qq.com`）反查坐标，GPS 优先级最低
- **飞行模式 + 关 WiFi** 后 SDK 会因无法联网而返回失败，而不是回退到纯 GPS，因此纯 adb mock 无法奏效
- 必须在 SDK 的 Java 层 hook 返回值才能绕过网络反查

### 网络 Endpoint

腾讯 LBS SDK 的定位上报及反查域名为 `apis.map.qq.com`（HTTPS），旁路 MITM 理论可行但需要信任自签证书，且微信 8.0.x 有 SSL Pinning 风险。

---

## 2. 现有开源/公开方案

### 方案 A — Shadow (Qeccentric/GlobalTraveling)
**GitHub**: https://github.com/Qeccentric/GlobalTraveling  
**最后更新**: 2026-01-25，活跃维护中

最完整的系统级方案。采用"系统层 Hook + 底层文件交互"，同时伪造 GPS、基站 Cell ID、WiFi BSSID 三个数据源，Hook 高德/腾讯地图 SDK。已在 **OnePlus Ace 3V / Android 16 / ColorOS 16** 上测试通过（与目标机型高度吻合）。

### 方案 B — GhostMapX (Xposed-Modules-Repo/com.ghostmapx.app)
**GitHub**: https://github.com/Xposed-Modules-Repo/com.ghostmapx.app  
**最后更新**: 2026-03-30，最新发布 v2.1.0

系统级 Hook，只需勾选系统框架进程而非每个应用，全局生效。对 OPPO 手机有专用 scope `com.oplus.location`。比 Shadow 配置更简单，但不伪造 WiFi/基站指纹（若腾讯 SDK 仍能获取周边 WiFi 扫描列表，可能产生偏移）。

### 方案 C — AnyWhere (cxOrz/AnyWhere)
**GitHub**: https://github.com/cxOrz/AnyWhere  
**最后更新**: 2026-03-22，最新版 v2.1.8

轻量级，同时支持无 root（基于系统 Mock Location API）和有 root + LSPosed（隐藏 mock 标记）两种模式。README 专门说明微信小程序场景：在 LSPosed 中为 `com.tencent.mm` 启用模块，会自动 hook 所有子进程。**但**其 LSPosed 模块侧重于隐藏 `isFromMockProvider` 标记，不直接 hook TencentLocationManager，对腾讯 SDK 网络反查的拦截效果未明确说明。

### 方案 D — WJFakeLocation (Entity-Now/WJFakeLocation)
**GitHub**: https://github.com/Entity-Now/WJFakeLocation  
**最后更新**: 2026-03-09，最新版 v2.0.0

专门为 Android 13-16 设计的 LSPosed 模块，支持基站（GSM/LTE/NR 全制式）和 WiFi 6E 信息伪造，含 GCJ-02/WGS-84/BD-09 坐标自动转换。Roadmap 中明确列出"应用差异化 Hook 策略（微信特殊处理）"尚未完成，当前效果待验证。

### 方案 E — HookWXMAppLocation（AshenOne 文章 + 代码）
**文章**: https://cloud.tencent.com/developer/article/2216961  
**归档镜像**: https://github.com/FightingForWhat/reverse-documents/blob/main/simpread-...AshenOne.md  
**性质**: 教程 + 最小可行代码，非完整可安装模块

这是技术原理最清晰的参考资料，直接 hook `TencentLocationManager.requestSingleFreshLocation` 和对应的 `getLatitude()`/`getLongitude()`，不依赖 mock location 标准 API，理论上绕过所有腾讯 SDK 网络反查。V2EX 2025 年帖子 [4] 显示有人用 `requestLocationUpdates` 的同构写法在新版微信上测试成功。

### 方案 F — 微信定位Pro（XYik0222）
**来源**: lsposed.cn 模块仓库  
**描述**: 专门针对微信小程序的 LSPosed 模块，"支持微信小程序"，入口为"长按微信+号"  
无独立 GitHub，仅通过 lsposed.cn 分发，代码不开源，可信度偏低。

---

## 3. 方案对比矩阵

| 方案 | 需要 Root | 需重启微信 | 被检测/封号风险 | 配置复杂度 | 当前微信 8.0.x 有效性 |
|------|-----------|------------|-----------------|------------|----------------------|
| Shadow (A) | 是 | 是（首次） | 低（伪造WiFi/基站，难侦测） | ★★★ | 高（ColorOS 16 已验证） |
| GhostMapX (B) | 是 | 是 | 低 | ★★ | 高（2026-03 仍更新） |
| AnyWhere (C) | 可选 | 是 | 中（未伪造基站） | ★★ | 中（未明确说明腾讯SDK） |
| WJFakeLocation (D) | 是 | 是 | 低 | ★★★ | 中（微信特殊处理未完成） |
| HookWXMApp (E) | 是 | 是 | 中（需自行编译） | ★★★★★ | 高（原理层面通杀） |
| 微信定位Pro (F) | 是 | 是 | 未知（不开源） | ★★ | 未知 |

---

## 4. 校园跑小程序的额外检测与模拟需求

根据 2025-05 最新的步道乐飞/步道乐跑分析文章 [7] 和 CSDN 社区实践记录 [8]：

### 已知检测机制

1. **里程 + 打卡点 GPS**：主要依赖定位，是本报告解决的核心问题
2. **步数/步频**（较新版本）：读取传感器数据，FakeLocation 付费版有"步频模拟"功能；也可通过禁用"传感器-访问设备动作与方向"权限规避（部分版本有效）
3. **后台检测**（新版）：检测是否有可疑后台进程。应对方法：用 LSPosed 模块（如 Dia）隐藏 FakeLocation 包名，或用双流量卡方案限制校园跑联网
4. **开发者模式 Mock Location 标记**：通过 `Location.isFromMockProvider()` 检测，需 root + LSPosed 才能隐藏（AnyWhere/Shadow 均有此功能）
5. **版本号强制更新**：旧版本方案可能随强制更新失效

### 已知专项方案

- **NanCunChild/ldbp**（已停止更新，2024-04 归档）：曾是专门的步道乐跑自动化工具，已停止维护，作者建议转向抓包方向 [9]
- **影梭 (GoGoGo)**：影梭开发者明确声明**不支持**校园跑类 App 的使用，且对微信系 App 无效 [10]
- **实践结论**：北邮校园跑（步道乐跑改版）为微信小程序，不需要单独的步道乐跑 App，因此需要的是绕过**微信小程序的 `wx.getLocation()`**，而非步道乐跑 Native App 的定位。Shadow 方案（同时伪造 WiFi 指纹 + TencentLocationManager hook）是目前最贴合场景的方案。

---

## 5. 不需要 Root 的灰色方案

| 思路 | 实际效果 |
|------|---------|
| 飞行模式 + 关 WiFi | **无效**。腾讯 SDK 在无网络时直接返回失败，不回退到 GPS |
| 微信"位置权限"设置 | **无效**。微信没有"强制使用系统 GPS"的开关，权限只控制是否允许定位，不控制 SDK 内部实现 |
| 系统 Mock Location（开发者选项）| **部分有效**，百度地图等会被骗，但腾讯 SDK 优先级高于系统 GPS，通常不受影响 |
| VirtualXposed | **基本失效**。微信 8.0.x 检测 VirtualApp 容器，直接拒绝运行或功能异常 [V2EX 讨论] |
| AnyWhere 无 root 模式 | **对微信小程序无效**。仅修改 `LocationManager` mock provider，腾讯 SDK 绕开了系统 API |

**结论**：对微信小程序 `wx.getLocation()` 来说，**无 root 方案不存在可靠的绕过路径**，必须 root + LSPosed hook TencentLocationManager。

---

## 6. 推荐路线

### 首选路线 — Shadow (Qeccentric/GlobalTraveling)

**理由**：已在 OnePlus Ace 3V / Android 16 / ColorOS 16 上验证（与 OPPO PJW110 架构相近），同时伪造 GPS + WiFi BSSID + 基站 Cell ID，从根本上切断腾讯 SDK 的三条反查路径，维护活跃。

**立即可执行的第一步**：

```bash
# 克隆仓库，查看最新 Release
git clone https://github.com/Qeccentric/GlobalTraveling.git
# 或直接前往 Releases 页下载 Shadow.apk
# https://github.com/Qeccentric/GlobalTraveling/releases
```

前置条件：先安装 Magisk（或 KernelSU）+ LSPosed Zygisk 版，重启后安装 Shadow.apk，在 LSPosed 中勾选微信 (`com.tencent.mm`) 作为作用域，重启手机。

### 备选路线 — GhostMapX (Xposed-Modules-Repo/com.ghostmapx.app)

**理由**：配置最简单（只需勾选系统框架进程），有专用 OPPO scope，更新至 2026-03，不需要逐应用配置。若 Shadow 因 ColorOS 16 有额外限制失败，可尝试此方案。

**立即可执行的第一步**：

```bash
# 下载 GhostMapX APK（v2.1.0）
# https://github.com/Xposed-Modules-Repo/com.ghostmapx.app/releases
# LSPosed 作用域：android, com.android.phone, com.oplus.location, com.android.location.fused
```

---

## 法律与学术诚信风险

用虚拟定位完成学校规定的体育打卡，属于学术诚信层面的欺骗行为，违反大多数高校学生手册的规定，被查处可能面临成绩无效或纪律处分。微信平台方面，伪造位置违反《微信软件许可及服务协议》，存在账号被封禁的风险（实际执行力度较低但不为零）。本报告仅供技术研究参考。

---

## 参考来源

1. 腾讯云社区 - 使用Xposed进行微信小程序API的hook: https://cloud.tencent.com/developer/article/2216961
2. FightingForWhat/reverse-documents 归档: https://github.com/FightingForWhat/reverse-documents/blob/main/simpread-使用%20Xposed%20进行微信小程序%20API%20的%20hook%20_%20AshenOne.md
3. V2EX - 2025年微信小程序虚拟定位模块讨论: https://www.v2ex.com/t/1104800
4. lsposed.cn - HookWXMAppLocation: https://lsposed.cn/1204
5. GitHub - Qeccentric/GlobalTraveling (Shadow): https://github.com/Qeccentric/GlobalTraveling
6. GitHub - Xposed-Modules-Repo/com.ghostmapx.app (GhostMapX): https://github.com/Xposed-Modules-Repo/com.ghostmapx.app
7. GitHub - cxOrz/AnyWhere: https://github.com/cxOrz/AnyWhere
8. GitHub - Entity-Now/WJFakeLocation: https://github.com/Entity-Now/WJFakeLocation
9. 步道乐飞防检测指北 (2025-05): https://wynphor.github.io/2025/05/08/步道乐飞防检测指北/
10. GitHub - NanCunChild/ldbp (已归档): https://github.com/NanCunChild/ldbp
11. GitHub - ZCShou/GoGoGo (影梭): https://github.com/ZCShou/GoGoGo
12. lsposed.cn - Shadow 模块页: https://lsposed.cn/1294
13. lsposed.cn - GhostMapX 模块页: https://lsposed.cn/1282
14. 步道乐跑破解校园跑教程: https://yumi1.top/school-campus-running/index.html
15. 腾讯云社区 - 安卓虚拟定位原理: https://cloud.tencent.cn/developer/article/2216962
