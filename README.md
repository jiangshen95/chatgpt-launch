# ChatGPT Launcher

一个跨平台（macOS / Windows / Linux）的 **ChatGPT 桌面版启动器**：为每个"配置"绑定
代理 / 时区 / 语言，一键以指定环境启动 ChatGPT 桌面版，并提供低风险的连接验证与
IP·时区·语言一致性检查。

> 定位类似 CC Switch / AdsPower 的"配置卡片 + 一键启动"，但面向 ChatGPT 官方桌面版。

## 功能（v1）

- **配置管理**：新建 / 编辑 / 复制 / 删除配置文件（卡片式列表）。
- **代理**：支持 `socks5` / `http` / `https`，可带用户名密码。
- **时区 / 语言**：可手动指定，或设为"自动"——启动时通过代理请求**中立第三方端点**
  （ipinfo.io / ip.sb / ipapi.is，**绝不触碰 OpenAI**）探测出口位置，自动得出时区与语言。
- **连接测试**：显示出口 IP / 国家 / 城市 / 时区，并给出"IP 与设备时区/语言是否一致"的
  风控一致性提示，可一键应用检测结果。
- **诊断模式（默认关闭）**：以 `--remote-debugging-port` 启动（仅绑定 `127.0.0.1`），
  并可观测已启动进程（含子进程）的 socket 连接，判断它是否真的连到了本地代理端口。
- **CDP 探针（诊断模式下）**：直连本地 DevTools 端口，读取 ChatGPT 进程内真实看到的
  **时区 / 语言 / 本地时间**，并通过其自身网络栈请求中立端点得到**实测出口 IP**，
  用于验证注入的代理/时区是否真正生效。
- **遥测开关**：UI 已预留 Sentry / Statsig / Sparkle 三处占位（仅保存配置，**v1 不生效**）。

## 架构

```
chatgpt-launcher/
├── core/                  # 纯逻辑库（无 GUI 依赖，可独立编译/测试）
│   └── src/
│       ├── model.rs       # 数据模型（Profile / ProxyConfig / ExitInfo ...）
│       ├── store.rs       # JSON 持久化（CRUD）
│       ├── launcher.rs    # 启动：定位 app + 注入环境变量/参数 + spawn
│       ├── geo.rs         # 出口位置探测 + 国家→语言映射
│       ├── consistency.rs # IP/时区/语言 一致性检查（风控提示）
│       ├── platform.rs    # 三平台 app 定位 + 进程树 socket 观测
│       ├── probe.rs       # CDP 探针：读取应用内时区/语言 + 实测出口
│       └── error.rs
└── src-tauri/             # Tauri v2 壳层：tauri::command 封装 + 前端
    └── src/lib.rs
```

- 后端：**Rust**（`core` 无 GUI 依赖，便于测试；`src-tauri` 只做命令转发与配置目录解析）。
- 前端：**React 19 + TypeScript + Vite**，通过 Tauri IPC 调用 Rust 命令。

## 构建

### 依赖

- Rust（stable）
- Node.js ≥ 20、npm
- 平台系统依赖（仅 Linux 需要）：`libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libssl-dev`
  —— 见 <https://tauri.app/start/prerequisites/>

### 命令

```bash
npm install

# 开发（前端热更新 + Tauri 窗口）
npm run tauri dev

# 打包安装器
npm run tauri build

# 仅测试核心逻辑（无需 GUI 系统依赖）
cargo test -p chatgpt-launcher-core
```

> 本仓库的 `package.json` 声明了 `allowScripts: { "esbuild": true }`（npm ≥ 12 的
> 脚本白名单），因为 Vite 依赖的 esbuild 需要执行 postinstall。

## 工作原理

启动一个配置时，`core::launcher` 会：

1. 定位 ChatGPT 桌面版可执行文件（自动探测，或使用配置里手动指定的 `appPath`）；
2. 若时区/语言设为"自动"，通过代理请求中立端点得到出口时区/语言；
3. 构造启动命令：

```text
env:  HTTPS_PROXY / HTTP_PROXY / ALL_PROXY / NO_PROXY
      TZ=<时区>  LANG=<语言>  LC_ALL=<语言>
args: --proxy-server=<代理URL>  --lang=<语言>
      （仅 Windows） --timezone-for-testing=<时区>
      （诊断模式追加） --remote-debugging-port=9224 --remote-debugging-address=127.0.0.1
                       --remote-allow-origins=*
```

4. `spawn` 并返回 PID / 实际参数等结果。

> Windows 下 Chromium 忽略 `TZ` 环境变量，故额外注入 `--timezone-for-testing`；
> `--remote-allow-origins=*` 用于放行程序化 CDP 客户端连接本地调试端口（见下文"验证"）。

## 验证代理与时区是否真正生效

「测试」只验证**代理本身**能否连通、出口在哪；要确认 **ChatGPT 进程实际**用了哪个代理、
看到了哪个时区，请用诊断模式：

1. 勾选顶部「诊断模式（默认关闭）」，启动配置。
2. 点击「观测连接」：按**进程树**（主进程 + 子进程）汇总 socket 连接，可看到每条连接的
   进程名 / PID / 远端，并提示是否命中本地代理端口。
3. 点击「CDP 探针」：读取 ChatGPT 渲染进程真实上报的
   - 时区（`Intl.DateTimeFormat().resolvedOptions().timeZone`）、
   - 语言（`navigator.language`）、本地时间（`new Date()`）、
   - 实测出口 IP/位置/时区（新建临时 target 走 ChatGPT 自身网络栈访问 `ipinfo.io/json`）。

   若实测出口 IP 与「测试」的出口一致，说明 `--proxy-server` 注入生效；若显示本机 IP，
   说明 Store 版未吃进代理参数。时区同理，与系统时区对比即可判断 `--timezone-for-testing`
   是否生效。

> 诊断模式仅绑定 `127.0.0.1`，但 `--remote-allow-origins=*` 会放宽本地调试端口的 Origin
> 校验，**仅用于诊断，日常请关闭**。亦可配合代理客户端（Clash/v2rayN）的连接日志交叉验证。

## 平台说明与已知限制

| 平台 | 说明 |
|------|------|
| macOS | 官方 app 是 Electron；为透传自定义环境变量，直接 exec `ChatGPT.app/Contents/MacOS/ChatGPT`（`open -a` 不会透传 env）。 |
| Windows | 官方 app 为 **Microsoft Store 签名的 MSIX** 应用（无独立 MSI/EXE），默认装于受保护的 `C:\Program Files\WindowsApps\`；本工具通过 `Get-AppxPackage` + `Get-AppxPackageManifest` 自动发现（兼容包名 `OpenAI.Codex`、`OpenAI.ChatGPT-Desktop`，及 exe 位于 `app\` 子目录的情况）。若直接启动被 ACL 拒绝（权限不足），请**以管理员身份运行本工具**；Store 版能否接受注入的 `--proxy-server`/环境变量需实机验证。另：Windows 上 Chromium 对 `TZ` 环境变量支持不可靠（[Chromium Issue 40200249](https://issues.chromium.org/issues/40200249)），本工具已改用 `--timezone-for-testing` 注入时区。 |
| Linux | 官方版处于公测，安装形态多样（AppImage/deb/snap），路径探测失败时请在配置里手动指定。 |

## 安全与风控说明

- **验证分层**：连接测试与 CDP 探针只打中立回显/地理端点（ipinfo.io 等），**从不请求 OpenAI**；
  socket 观测只看本机连接；诊断模式默认关闭且仅绑定 `127.0.0.1`（但会注入
  `--remote-allow-origins=*` 以允许本机程序化 CDP 客户端连接，仅诊断时使用）。
- **一致性优先**：IP、时区、语言三者长期一致是规避风控的关键，本工具内置一致性检查正是为此。
- 用代理访问 ChatGPT 可能触及 OpenAI 服务条款，请自行评估并遵守当地法规。

## Roadmap

- [ ] 遥测拦截（Sentry / Statsig / Sparkle），通过 hosts 或本地过滤代理实现
- [ ] 代理出口健康度 / 独享性提示
- [ ] 多账号与"一账号一 IP"的固定绑定提示
- [ ] 自动更新（tauri-plugin-updater）
