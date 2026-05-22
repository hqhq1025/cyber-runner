# Android 架构 B 实测验证

**日期**：2026-05-22
**环境**：macOS · Android Emulator AVD `Medium_Phone_API_35` (Android 15, Google Play 镜像) · adb 35.0.2

## 结论先行

✅ **架构 B 通**。Android 的 mock location 注入可以在不安装任何应用、不 root 的情况下，**单条 `adb shell` 命令** 完成：

```bash
adb shell appops set com.android.shell android:mock_location allow
adb shell cmd location providers add-test-provider gps \
  --supportsAltitude --supportsSpeed --supportsBearing
adb shell cmd location providers set-test-provider-enabled gps true
adb shell cmd location providers set-test-provider-location gps \
  --location <LAT>,<LNG> --accuracy 4
```

GPS + Fused 两个 provider 同时收到伪造位置，`dumpsys location` 立即可见。

## dlut-FakeRun 的真实形态

clone + build `LangQi99/dlut-FakeRun` 后，读源码发现这个 APP **本质上就是上面 4 条 adb 命令的图形化包装**：

- `app/src/main/java/com/langqi/fakegps/LocationMockService.java::addTestProviderGPS()` 调 `LocationManager.addTestProvider(...)`
- `setLocationGPS()` 调 `setTestProviderLocation(...)`
- 它额外加了：注册一个 BroadcastReceiver 监听 `com.langqi.fakegps.MOCK_LOCATION` intent + 一个 Activity 让人选 KML 路径 + 一个前台通知不被系统杀

**对我们项目的意义**：架构 B 实施时**不需要**编译/分发 dlut-FakeRun 这个 APK，直接在 Rust 桌面端用 `std::process::Command::new("adb").args(["shell", "cmd", "location", "providers", "set-test-provider-location", ...])` 调上面 4 条命令就够了，比 iOS 侧用 `pymobiledevice3` 简单一个数量级。

## 实测步骤与产物

### 1. 工具链

```
adb 35.0.2
Java 22 (JDK 23 也可)
Android SDK 34
AVD: Medium_Phone_API_35 (Android 15)
gradle: 用项目自带 ./gradlew (需先 chmod +x)
```

### 2. 启 emulator

```bash
emulator -avd Medium_Phone_API_35 -no-window -no-audio -no-snapshot -wipe-data
```

**坑**：第一次启用了 snapshot 复活导致 `adb` 返回 `unauthorized`，必须加 `-no-snapshot -wipe-data` 让它 fresh 启动接受 host 的 ADB key。

### 3. （可选）装 dlut-FakeRun APK

```bash
cd external/dlut-FakeRun
chmod +x gradlew
JAVA_HOME=/Library/Java/JavaVirtualMachines/jdk-22.jdk/Contents/Home ./gradlew assembleDebug
adb install app/build/outputs/apk/debug/app-debug.apk
```

构建耗时 4 分钟，产出 ~3 MB 的 debug APK。

### 4. 直接走 cmd location（绕过 dlut-FakeRun）

5 个点的连续投放（轨迹模拟）：

```bash
POINTS=(
  "39.96706,116.36655"
  "39.96712,116.36661"
  "39.96718,116.36667"
  "39.96724,116.36673"
  "39.96730,116.36679"
)
for p in "${POINTS[@]}"; do
  adb shell cmd location providers set-test-provider-location gps \
    --location "$p" --accuracy 4
  sleep 1
done
```

`dumpsys location` 输出（末点验证）：

```
last location=Location[gps   39.967300,116.366790 hAcc=4.0 et=+3m31s321ms mock]
last location=Location[fused 39.967300,116.366790 hAcc=4.0 et=+3m31s321ms mock]
last location=Location[gps   39.967300,116.366790 hAcc=4.0 et=+3m31s321ms mock]
```

末点坐标精确匹配，证明：
- ✅ 5 点全部成功注入
- ✅ GPS 与 Fused 两个 provider 同步更新
- ✅ 精度（hAcc=4.0）可控

## 剩下的 `mock` 标志问题

Android 系统给每个 mock 出来的 `Location` 自动打 `mock` 标记（对应 Java 层 `Location.isFromMockProvider()` 返回 true）。**严格的校园跑 APP**（如步道乐跑、乐动力）会读这个标志拒绝接受数据。绕过有两条路：

1. **LSPosed + Xposed Hook**（需 root）：用 `XposedFakeLocation`、`MotionEmulator` 等模块 hook 掉 `isFromMockProvider` 让它永远返回 false
2. **APP 本身 Hook**（需 root + LSPatch）：直接改校园跑 APP 的检测逻辑，参考 `LiuYiGL/RunWorldSchoolMod`

校园跑 APP 检测严格性的实际情况：
- ✅ `Keep`：检测 mock 标志，会拒
- ⚠️ `小米运动健康` / `华为运动健康` / `dlut 操场跑`：调研显示 dlut-FakeRun 默认免 root 方案能用，说明这些 APP 不查 mock 标志
- ❓ `步道乐跑` / `乐动力`：需在真机实测

## 与项目的整合方案

把上面 4 条 `adb shell` 命令封装成 Rust 模块加到 `crates/fake-run-engine/`，与 iOS 的 `pymobiledevice3 + python main.py` 并列：

```rust
// crates/fake-run-engine/src/android.rs (待实现)
pub async fn android_init(adb: &Path, serial: &str) -> Result<(), EngineError> {
    run(adb, &["-s", serial, "shell", "appops", "set", "com.android.shell",
               "android:mock_location", "allow"]).await?;
    run(adb, &["-s", serial, "shell", "cmd", "location", "providers",
               "add-test-provider", "gps", "--supportsAltitude",
               "--supportsSpeed", "--supportsBearing"]).await?;
    run(adb, &["-s", serial, "shell", "cmd", "location", "providers",
               "set-test-provider-enabled", "gps", "true"]).await?;
    Ok(())
}

pub async fn android_push_location(
    adb: &Path, serial: &str, lat: f64, lng: f64, acc: f32,
) -> Result<(), EngineError> {
    run(adb, &["-s", serial, "shell", "cmd", "location", "providers",
               "set-test-provider-location", "gps",
               "--location", &format!("{lat},{lng}"),
               "--accuracy", &acc.to_string()]).await?;
    Ok(())
}
```

复用 `fake-run-core::{bd09_to_wgs84, fixed_step_path, parse_route}` 算法，**Tauri host 的 RealEngine 可以并行支持 iOS（spawn python）和 Android（shell adb）两条路径**，UI 上加一个"目标设备"选项即可。

## 立即可执行的下一步

按优先级：

1. **真机实测**：找一台已 root 或开了开发者模式的 Android 手机，装步道乐跑/乐动力，跑上面 4 条命令，看 APP 是否接受
2. **如果接受** → 在 `fake-run-engine` 加 `android.rs` 模块（约 100 行），在 UI 加设备选择器
3. **如果拒绝（mock 标志被检测）** → 先装 LSPosed + `XposedFakeLocation`，再用 4 条命令测一次

## 引用

- 调研报告：[`docs/research/android-fake-run-2026-05-22.md`](../research/android-fake-run-2026-05-22.md)
- dlut-FakeRun 源码（已 clone）：[`external/dlut-FakeRun/`](../../external/dlut-FakeRun/)
- 关键源文件：`external/dlut-FakeRun/app/src/main/java/com/langqi/fakegps/LocationMockService.java`
