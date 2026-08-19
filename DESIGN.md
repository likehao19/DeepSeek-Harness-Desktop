# DeepSeek Harness 嵌入 Tauri 2 — 方案文档

> 结论：DSH 是本地 client-server 应用。最优方案是 **Tauri 作为桌面壳 + DSH 以 Node sidecar 原样运行 + WebView 窗口加载其本地 URL**，对 DSH 零改造。

## 1. DSH 运行架构（来自对 `@deepseek-ai/dsh` 0.1.0-rc.7 源码的核实）

- `dsh` 是 Node.js CLI（ESM），基于 Cordis 插件树。
- `dsh --profile web` 启动一个 **loopback HTTP 服务**（默认 `127.0.0.1:3080`；`--port 0` 由系统分配空闲端口；`--host` 只允许 `127.0.0.1`，`0.0.0.0` 被主动拒绝以保安全）。
- 该服务负责：
  - 托管已构建的 SPA 前端（`@deepseek-ai/dsh-web-frontend/dist`，发布包内已内置 build 产物）；
  - `/api` HTTP 桥 + WebSocket/SSE 下行（`dsh-client-connection`）；
  - 插件 bundle 与 HMR 流；
  - 在返回的 `index.html` 注入 `window.__DSH_BOOT__` 引导清单（`dsh-host-frontend-static` 的 index taps）。
- 浏览器端读取清单后启动 React UI，再回连同一服务（同源，走 `/api` 与 WS）。
- 持久化：默认 `DSH_HOME` = `$DSH_HOME` 否则 `~/.dsh`；会话/凭证/配置/个人 preset 都存于该目录。

**推论**：UI 无法作为纯静态 `file://` 运行，后端 Node 进程必须存活。故不做「静态前端 + IPC 桥」的重写方案。

## 2. 推荐架构（已确认采纳）

```
┌────────────────────────────────────────────────┐
│ Tauri 2 桌面壳 (Rust)                           │
│  · 主窗口 → 加载 http://127.0.0.1:<port>        │
│  · 管理 DSH sidecar 生命周期                    │
│        │ spawn + 解析端口 + 退出时回收          │
│        ▼                                       │
│  Node sidecar: node bin.js --profile web       │
│    --port 0 --host 127.0.0.1                   │
│  (dsh-runtime/ 内置 @deepseek-ai/dsh 依赖树)    │
│        │ loopback HTTP + WS                    │
└────────────────────────────────────────────────┘
```

### 各层职责
| 层 | 职责 |
|---|---|
| **Rust 壳 (`src-tauri/`)** | `setup()` 里 spawn sidecar；读 sidecar stdout 的 `dsh web: http://127.0.0.1:<port>` 得到端口；用 `WebviewWindowBuilder` 打开窗口指向该 URL；`RunEvent::Exit` 时 kill 子进程 |
| **DSH sidecar** | 未修改的上游 CLI，`--port 0` 动态端口避免冲突；`--host 127.0.0.1` 只暴露回环 |
| **前端** | 直接用 DSH 发布包内的 dist，不新建前端；Tauri 不注入 IPC 到该远程页面 |
| **安全模型** | 与直接跑 `dsh web` 一致：模型对本地 bash/pwsh/文件系统的访问由 DSH 自身工具处理 |

## 3. 关键决策与理由

### 3.1 运行时打包
DSH 用 Cordis Loader 动态 `import()`/`require.resolve` 解析几十个 `@deepseek-ai/dsh-*` 包 + `node-addon-require-builtin` 原生 helper，**不适合 SEA / bun --compile / pkg 单文件打包**。
- ✅ 本方案：`dsh-runtime/` 内置完整 `@deepseek-ai/dsh` 依赖树，配合**便携式 Node 22 LTS**（打包阶段作为 resource/externalBin 随应用分发；开发阶段用系统 `node`）。
- 后续可选：成熟后再评估 `bun build --compile` 减小体积。

### 3.2 端口发现
- sidecar 用 `--port 0`（系统分配），Rust 端解析 stdout 的 URL 行拿到真实端口。
- 兜底：解析失败则记录错误日志；后续可加固定端口 + 冲突扫描。

### 3.3 数据持久化
- sidecar 启动时设置 `DSH_HOME=<app_data>/dsh-home`，避免污染用户本机 `~/.dsh`，并随应用数据目录稳定落盘。
- 工作目录设为 `<app_data>/workspace`（模型的工作区）。

### 3.4 平台
- Windows：WebView2（已装）；macOS：WKWebView；Linux：WebKitGTK。
- 运行时按目标三元组内置对应 Node。

## 4. 项目结构
```
DeepSeek_Harness_Desktop/
├─ DESIGN.md                 # 本文档
├─ README.md                 # 使用说明
├─ package.json              # @tauri-apps/cli 开发依赖 + 脚本
├─ dsh-runtime/              # 内置 @deepseek-ai/dsh 依赖树（构建时组装）
│  └─ node_modules/@deepseek-ai/dsh/lib/bin.js
├─ frontend/index.html       # 占位 distDir（运行时窗口 URL 会被覆盖）
└─ src-tauri/
   ├─ Cargo.toml
   ├─ tauri.conf.json
   ├─ build.rs
   ├─ capabilities/default.json
   └─ src/
      ├─ main.rs
      ├─ lib.rs
      └─ sidecar.rs          # spawn / 端口解析 / 开窗 / 回收
```

## 5. 备选方案（未采纳）
- **Option B：重建前端走 Tauri IPC**：需大改 DSH 传输层，破坏「无上游改动」、工作量与脆弱性高。仅在完全离线、不含 Node 的分发诉求下考虑。

## 6. 参考实现
- [Lotus-c/DSH-Desktop](https://github.com/Lotus-c/DSH-Desktop)（Tauri 2）
- [ucas-liumk/deepseek-harness-desktop](https://github.com/ucas-liumk/deepseek-harness-desktop)
- 官方：[DeepSeek Harness](https://www.deepseek.com/harness/)、[deepseek-ai/deepseek-harness](https://github.com/deepseek-ai/deepseek-harness)

## 7. 交付范围（当前阶段）
最小可运行壳：sidecar 管理 + 窗口加载 DSH UI，跑通 `tauri dev`。托盘/单实例/更新/安装包为后续阶段。
