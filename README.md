# Yet Another FreePPS

YAFreePPS 是基于 [Seyud/FreePPS](https://github.com/Seyud/FreePPS) 的独立 GPL-3.0-or-later 派生项目，基线提交为 `1557e092da83823e1608af6426937af89d2a4a75`。运行时已重写；保留原作者署名和许可证。

面向具有对应 sysfs 节点的 arm64 小米高通设备。仅对稳定识别的公版 PPS 尝试写入验证位，小米 MIPPS 保留内核原生处理。**无独立 App、无 APK 下载、无广播伪造**；使用模块 WebUI 和 Action 控制。

## 当前行为

| 场景 | 行为 |
| --- | --- |
| `adapter_svid=2717` | 当前物理会话内锁定为小米头，不写验证位，交给内核 `verify_process` |
| `adapter_svid=0000` 且 `real_type=PD_PPS` | `free=1`、等待插入保护和 SVID 稳定窗口后，重新读取条件，尝试写 `pd_verifed=1` |
| SVID 空、读取失败、其他值或非 PPS | 不写入；未来有相关事件时再判断 |
| 启动或重启时已插电 | 创建全新会话，重新等待和分类；不预写 |
| `free` 从 0 恢复到 1，仍已插电 | 重新等待；已观察到的小米 SVID 否决保持到本次拔出 |
| `Charging → Discharging` | 结束会话、丢弃分类和待执行超时；不写节点 |
| 暂停或卸载，仍处于 Charging/未知状态 | 不写 0，避免打断协商；内核在拔出时清零 |
| 显式暂停时已 Discharging | 若验证节点仍为 1，恢复为 0；本来为 0 则不重复写 |
| 电量 100%、`stop_handle_charge=1` 或已读取到 `qct=0` | 不尝试解锁；电量不可读也不解锁 |

协议分类只看 `adapter_svid`；`real_type` 仅用于确认是否 PPS，不能区分 MIPPS 和公版 PPS。`adapter_id`、`product_vdo` 和 Vbus 不参与判断。每次 attach 都独立处理，不沿用前一个充电器的结论。

公版 PPS 写入成功后不反复锁定节点；写失败可在下一次外部事件重试。节点原本已为 1 时不重复写。暂停期间仍接收相关 uevent，用于识别拔出及小米头；没有定时轮询。

## UI 与控制

- **Action**：管理器中的模块操作按钮执行启用/暂停切换。无需另装 App。
- **WebUI**：在支持 KernelSU WebUI `ksu.exec` 桥接的管理器内打开，查看状态、切换启停、设置握手等待时间。前端完全本地，无 CDN 或网络请求。
- Magisk 可使用 Action；Magisk 原生管理器不提供本项目所需的 KernelSU WebUI 桥接。兼容第三方 WebUI 宿主的行为仍需实机验证。
- WebUI 只在打开、回到前台、点击刷新或控制操作后读取状态，不后台轮询。页面中的 APDO 是能力值，不是实时功率。
- **不会发送任何快充、SOC、动画或通知广播**。公版头可能继续显示系统原生的 `qct=1` 和普通快充 UI，这是预期行为。不会伪造 `chargeSpeed`、`POWER_MAX`、`maxWattage` 或 `miuiChargerVerified`。

CLI（root）：

```sh
BIN=/data/adb/modules/YAFreePPS/bin/yafreepps
"$BIN" status
"$BIN" enable
"$BIN" disable
"$BIN" toggle
"$BIN" configure 2500 1500
```

`status` 输出 JSON；`running` 来自进程持有的文件锁，而不是可能过期的 PID。`disable`/`remove` 标志优先于 `free`，Action/WebUI 不会擅自移除管理器的停用或卸载标志。此时若要重新启用整个模块，需要在管理器中操作并按其要求重启。

`free` 和 `config` 使用临时文件 + rename 原子更新。后台监听模块目录，文件被替换后监控仍有效。无效配置会导致启动失败，或在运行中被检测到时暂停；CLI 仍可暂停、重写正确配置。

## 安装与迁移

1. **先卸载原版 FreePPS 并重启**。两者不能同时运行。安装脚本检测到原模块目录时会拒绝安装，以免旧服务继续预写。旧版独立 App 不再是本项目的控制入口，可自行卸载。
2. 将构建生成的 `YAFreePPS-v0.1.0.zip` 通过支持模块安装的 root 管理器安装并重启。
3. 在 WebUI 检查“服务运行中”，或使用 `status`。若服务未运行，查看 `daemon.log` 和 `logcat -s YAFreePPS`。
4. 默认 `free=1`。升级保留原有 `free` 和 `config`。安装和服务启动均不无条件写验证位。

当前只启用 `/sys/class/qcom-battery/pd_verifed` 路径。**MTK 不支持**：报告未验证其 SVID 时序和写入语义，因此删除原版 MTK 写入线程。未修改 sysfs 权限或 SELinux 配置；节点权限不足会记录错误。

`service.sh` 在 root late-start 阶段直接启动，不等待解锁屏幕或 `/sdcard` 可写。启动时缺失的 SVID 在后续相关事件中继续判断。若系统禁止 netlink 订阅，服务退出并报告错误，不回退为轮询或盲写。

## 兼容性与证据边界

旧版 FreePPS 的无条件预写路径与 MIPPS 互斥，会干扰小米原装 MIPPS 头；这不是用 `PD_PPS` 就能区分的“部分设备”兼容问题。用户提供的同一只 65 W 小米头实测中，预写造成 `svid=0000 / apdo=55 / qct=1`，不预写则恢复 `svid=2717 / apdo=65 / qct=4`。

本项目的实现目标是：**小米头零节点写入，公版头才尝试解锁**。代码通过会话内 SVID 否决和写入前复查落实这一目标，但本次交付尚未做手机端硬件验证；不把软件测试等同于所有机型的零副作用证明。

以下仍未验证：

- 公版 PPS 在 attach 后延迟写 1 是否真正解锁更高功率。
- 本机 `2509FPN0BC` 约 1 秒的 SVID/verify 时序能否推广到其他机型。
- 解锁后写 0 是否即时降功率。实现不会在连接状态下主动测试这一点。

默认插入保护时间 `settle_ms=2500`，SVID 连续观察窗口 `svid_stable_ms=1500`，二者取较晚到期时间；采用单次超时补读，而不是周期轮询。范围分别为 2000–30000 和 1000–30000 ms。未知 SVID 永不写入。`0000` 本身不能证明所有硬件都已结束身份发现，延长等待也不是跨机型完成握手的数学保证。若内核在没有 uevent 的情况下改变中间值，软件可能无法观察到；sysfs 多节点读取和写入也不是内核原子事务。

暂停/卸载后，“保持 0”指未插电的稳态；若暂停时仍连接且验证值已为 1，本项目不强制清零，等待内核在拔出时复位。不会为了立即显示 0 而违反连接状态下禁止写 0 的约束。

## 验收步骤（需实机执行）

1. 小米头：拔出 → 等待至少 15 s → 插入。检查 `svid=2717 / qct=4 / apdo=65`，原生 SystemUI 应恢复报告中的 `chargeSpeed=3 / maxWattage=65`；`logcat -s YAFreePPS` 应无节点写入记录。
2. 重复插拔公版头、小米头，交替测试；后一个会话不得继承前一个的类别。
3. 公版头：记录插入后的 SVID、验证位、APDO、实际电压电流和功率。观察延迟写 1 后真实功率变化；不得将 APDO 或 UI 动画当作功率证据。
4. 覆盖启动前已插电、重启后仍插电、`free=0→1`、100% 涓流。满电和涓流不执行解锁。
5. 充电中暂停/卸载：不应出现写 0。拔出后确认内核清 0，再等待至少 15 s 插入小米头，应恢复原生 MIPPS。
6. 验证 WebUI 和 Action 指向同一个状态；SystemUI 重启不应触发任何本模块广播。

## 构建与测试

需要 Python 3.11+、Rust stable（edition 2024）、Android NDK、arm64 Android 目标；不需要 Windows 固定目录或 7-Zip。

```sh
rustup target add aarch64-linux-android
rustup component add rustfmt clippy
cargo build --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
python3 -m unittest discover -s tests -v
node --test tests/webui.test.cjs
python3 build_android.py --ndk /absolute/path/to/android-ndk
```

构建脚本默认 API 28（Android 9+），执行主机测试、主机与 Android clippy、Android release 编译，然后生成 `output/yafreepps` 和模块 ZIP。`package_module.py` 检查 arm64 ELF 头和 Android 动态链接器路径，拒绝误打包主机二进制。ZIP 包含许可证、署名和 README。源代码 ZIP 不是可刷入模块。

GitHub Actions 使用固定 NDK r27d，提供以下构建和发布入口：

- 向 `main` 提交或发起 PR：测试、arm64 构建、上传模块 ZIP、精确提交的源码 ZIP 和 SHA256SUMS；不发布 Release。
- 推送 `v*` 标签：标签必须与 Cargo.toml、module.prop 版本一致；全部检查成功后自动发布 GitHub Release。
- Actions → Build and release → Run workflow：默认仅构建；勾选 `publish_release` 并填写匹配的 `release_tag` 可手动发布，也可由工作流创建标签。
- `0.x` 或包含预发布后缀的版本标记为 Pre-release，不设为 Latest。已有 Release 不会被重跑覆盖。
- 构建任务仅有仓库读取权限；发布任务单独获得 `contents: write`，使用 GitHub 自带令牌，不需要自建 PAT。PR 不触发发布。

首次发布示例（版本已为 0.1.0）：

```sh
git tag v0.1.0
git push origin v0.1.0
```

后续发布先同步 Cargo.toml、Cargo.lock 和 module.prop 的版本，并递增 versionCode，再推送对应标签。源码 ZIP 通过 `git archive HEAD` 生成，校验和覆盖模块与源码包。

测试使用伪 sysfs。Rust 包括协议轨迹测试，以及真实 inotify/poll 配合本地数据报的事件循环测试。Python 检查 CLI、暂停还原、配置验证和 JSON 转义；当宿主沙箱禁止 `NETLINK_KOBJECT_UEVENT` 时，仅真实 netlink 集成用例显式跳过，不绕过限制。Node 使用模拟 `ksu` 桥接验证页面控制和参数校验。上述测试不能证明充电功率或手机 ROM 兼容性。

## 代码入口

| 路径 | 职责 |
| --- | --- |
| `src/policy.rs` | 纯会话状态机与协议策略 |
| `src/reactor.rs` | netlink、目录 inotify、signalfd、单次分类超时与单实例锁 |
| `src/device.rs` | sysfs 快照、唯一写 1 入口、显式空闲还原、JSON 状态 |
| `src/main.rs` | daemon / 控制 CLI |
| `module/webroot/` | 无依赖 WebUI，使用 `textContent` 渲染节点内容 |
| `module/action.sh` | 模块操作按钮 |
| `module/uninstall.sh` | 先暂停再终止服务，不预写下一会话 |

WebUI 桥接依据 [KernelSU 官方模块 WebUI 文档](https://kernelsu.org/guide/module-webui.html) 和 [官方 JS 桥接实现](https://github.com/tiann/KernelSU/blob/main/js/index.js)。本地前端只构造固定 CLI 命令，时间设置只接受范围内整数，不把 sysfs 内容或任意用户文本拼入 root 命令。
