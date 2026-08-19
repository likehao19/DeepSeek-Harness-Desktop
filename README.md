# DeepSeek Harness Desktop

Tauri 2 桌面壳，把 [DeepSeek Harness](https://www.deepseek.com/harness/)（`dsh`）作为 Node sidecar 原样运行，并用系统 WebView 窗口加载其本地 HTTP UI。**对 DSH 零改造**。详见 [DESIGN.md](DESIGN.md)。

## 架构
```
Tauri 壳 (Rust) ──spawn──▶ Node sidecar: node bin.js --profile web --port 0
    │                              （dsh-runtime/ 内置 @deepseek-ai/dsh）
    └── WebView 窗口 ──http://127.0.0.1:<port>──▶ DSH HTTP + WS 服务
```

## 前置条件
- Node.js 22+（开发阶段用系统 node 跑 sidecar）
- Rust 工具链（`cargo`）
- Windows：WebView2（Win10/11 已预装）

## 开发
```bash
# 1. 组装 dsh-runtime（安装 @deepseek-ai/dsh 依赖树）
npm run dsh:install

# 2. 安装 Tauri CLI
npm install

# 3. 启动
npm run dev        # 等价于 tauri dev
```

首次启动会自动在应用数据目录下建 `dsh-home`（会话/凭证）与 `workspace`（模型工作区）。

## 打包（Windows exe 安装包）
```bash
# 1. 组装 dsh-runtime（含内置便携 Node 运行时）
npm run dsh:install
copy <path-to>\node.exe dsh-runtime\node.exe     # 内置 Node 22 LTS

# 2. 生成官方图标（需 Node 22 + 依赖 sharp）
cd scripts/icon-tools && npm install --ignore-scripts
node gen-official-icon.mjs

# 3. 构建 NSIS 安装包
cd ../../src-tauri
tauri build --bundles nsis
# 产物：dist\DeepSeek Harness_0.1.0_x64-setup.exe
```

## 无 Node 环境能否运行？
**能。** 安装包内置了便携 Node 22 运行时（`dsh-runtime/node.exe`）与完整的 DSH 依赖树（`dsh-runtime/`）作为应用资源。运行时 Rust 壳从资源目录解析内置 `node.exe`，不依赖系统 PATH 里的 Node。本机已实测：把 Node 从 PATH 移除后运行安装后的 exe，DSH 服务照常启动并返回完整 UI。

前提只有 Windows 自带的 **WebView2 运行时**（Win10/11 预装；安装包默认在缺失时引导下载）。

> 代价：安装包较大（约 50MB，内置约 300MB 未压缩的 Node + DSH 依赖，经 NSIS LZMA 压缩后）。
> 已实测两种瘦身均**不可行/不建议**：
> - `bun build --compile`：DSH 通过 Cordis Loader 在运行时动态 `import()`/`require.resolve` 几十个包，bun 只能静态打包 21 个模块，产出的 exe 启动即报路径错误。
> - 精简 `node_modules`：用运行时加载追踪找出“从未加载”的 297 个包（约 154MB）后移除以瘦身，结果启动即崩（原生 `koffi` 模块虽未被 `import` 却会在启动时被 `require.resolve` 到）。DSH 的动态解析机制使得“按加载情况裁剪”不安全，且激进裁剪还会禁用功能（其他模型厂商 SDK、MCP 客户端、图片处理）。故保持完整依赖树，以换取功能可靠。

## 功能特性
- **无控制台窗口**：sidecar（`node.exe`）以 `CREATE_NO_WINDOW` 创建，启动时不弹 cmd 黑窗；主程序为 Windows GUI 子系统。
- **系统托盘**：托盘图标 + 菜单（显示/聚焦、在浏览器打开、退出）；左键单击托盘也可唤起窗口。
- **单实例**：重复启动只会聚焦已有窗口，不会开第二个进程。
- **自动更新**：启动时后台检查更新并静默安装（见下方「自动更新」）。
- **数据同步**：可与用户已安装的 Harness 共享会话/凭证/配置（见下方「同步」）。

## 同步（与已装 Harness 共享数据）
DSH 的全部用户数据都放在 `DSH_HOME`（会话、API 凭证、模型配置、个人 preset）。桌面应用按以下优先级选择 `DSH_HOME`：
1. `DSH_DESKTOP_DSH_HOME` 环境变量（显式指定，可指向任意路径）；
2. 若用户本机已存在 `~/.dsh`（说明已装 Harness），自动复用 → **与已装 Harness 完全共享数据**；
3. 否则用应用自带数据目录 `<data>/dsh-home`。

> 说明：`DSH_DESKTOP_DSH_HOME` 设为 `~/.dsh` 即可与官方 Harness 同步。工作区可用 `DSH_DESKTOP_WORKSPACE` 指定。

## 自动更新
自动更新**只更新本桌面应用外壳本身**（含内置的 DSH 运行时版本），**不是**“同步拉取最新 Harness”，也**不是**数据同步。
- 实现：`tauri-plugin-updater`，启动时后台 `check()`，发现新版本即下载安装并重启。
- 需要：一个更新服务器（在 `tauri.conf.json` 的 `plugins.updater.endpoints` 配一个返回 Tauri update manifest 的 URL）+ Ed25519 签名（公钥已写入配置；私钥在本仓库 `signing-key.secret`，**已 gitignore**，构建/发版时用 `TAURI_SIGNING_PRIVATE_KEY` 环境变量签名）。
- 当前 `endpoints` 是占位地址，需替换为你自己的更新服务器后才能工作。

## 验证记录
已在本机跑通：`cargo build` 编译通过 → 启动 `dsh-desktop.exe` → 自动 spawn `dsh --profile web --port 0`（DSH 服务端口动态分配）→ WebView2 窗口打开并加载 DSH UI（服务返回 `index.html`，含注入的 `window.__DSH_BOOT__`，标题 “DeepSeek Harness”）。并验证：无 Node 环境独立运行、单实例生效、`DSH_DESKTOP_DSH_HOME` 同步目录生效。

## 环境变量（可选）
- `DSH_DESKTOP_NODE`：Node 运行时路径（打包时指向内置 node）。
- `DSH_DESKTOP_RUNTIME`：`dsh-runtime` 目录路径。
- `DSH_DESKTOP_DATA_DIR`：数据根目录（内含 `webview2` 等），默认用系统应用数据目录。
- `DSH_DESKTOP_DSH_HOME`：DSH 用户数据目录（同步用，见上）。
- `DSH_DESKTOP_WORKSPACE`：模型工作目录。
- `TAURI_SIGNING_PRIVATE_KEY`：自动更新发版签名私钥。

## 本机网络说明（仅此工作台）
本机的原生 TLS（schannel，cargo/curl/.NET 使用）损坏（`SEC_E_NO_CREDENTIALS`），Node 的 OpenSSL 可用。因此：
- `scripts/registry-proxy.mjs` 是一个把 cargo 的 HTTPS crates 源转成 loopback 明文 HTTP 的本地代理（Node/OpenSSL 转发到 rsproxy.cn）。cargo 之前需先运行它：`node scripts/registry-proxy.mjs`。
- `src-tauri/.cargo/config.toml` 指向该本地代理。在网络健康、schannel 正常的机器上可删除该代理与配置，改回 crates.io 直连。
- GitHub 下载（NSIS 工具链）不稳定，可用 `gh-proxy.com` / `ghfast.top` 镜像。

## 结构
```
├─ src-tauri/       Rust 壳（sidecar 管理、窗口）
├─ dsh-runtime/     内置 @deepseek-ai/dsh 依赖树
├─ frontend/        占位 distDir（运行时窗口 URL 会被覆盖）
└─ DESIGN.md        方案文档
```
