# 内置代理池与网络服务实现方案

> 本文档是后续实现当前项目内置代理能力的执行准则。后续每个功能点必须按本文档拆分实施、验证、提交验收结果；只有用户明确回复“继续”或同等明确指令后，才能进入下一功能点开发。

## 目标

在 `cockpit-tools` 的“网络服务”中提供自带代理能力，使用户无需再依赖 Clash、v2rayN、浏览器代理插件等外部软件，就能完成代理节点、节点池、订阅、测速、IP 健康检查、订阅刷新和全局代理接入。

目标平台必须覆盖当前项目支持的桌面平台：

- Windows x64
- macOS Intel
- macOS Apple Silicon
- Linux x64
- Linux arm64

## 本次调研结论

### Trace-Browser 可借鉴部分

Trace-Browser 的代理能力集中在这些模块：

- `backend/internal/proxy/`：节点解析、xray/sing-box 桥接、测速、IP 健康 HTTP client。
- `backend/internal/browser/proxy_dao.go`：代理节点 SQLite 持久化、分组、测速结果、IP 健康结果、订阅来源字段。
- `backend/app_proxy_import.go`：Clash URL 订阅拉取、Base64/分享链接订阅兼容、DNS 与分组建议。
- `backend/app.go` 代理池 API：列表、分组、保存、验证、单个/批量测速、单个/批量 IP 健康、导入预览测速。
- `backend/proxy_switch_bridge.go`：HTTP CONNECT 中转、自动切换出口、xray/sing-box 自动桥接。
- `backend/authenticated_proxy_bridge.go`：带账号密码的 HTTP/SOCKS 代理转为浏览器可用本地 HTTP 中转。
- `publish/runtime-manifest.json` 与 `publish/runtime-sources.json`：xray/sing-box 的多平台运行时清单和校验思路。
- `frontend/src/modules/browser/pages/ProxyPoolPage.tsx`：代理池、订阅管理、添加资源、测试全部、检查 IP、刷新订阅等交互形态。

本地检查显示 Trace-Browser 仓库已有 `bin/darwin-amd64` 和 `bin/darwin-arm64` 的 xray/sing-box 文件及 manifest 记录。但当前 `cockpit-tools` 没有这些代理运行时资源，也没有代理池模块。

### cockpit-tools 当前现状

当前项目是 Tauri 2 + Rust + React/Vite：

- 网络设置在 `src/pages/SettingsPage.tsx`，已有 WebSocket 服务、网页查询服务、全局代理开关。
- 全局代理配置保存在 `src-tauri/src/modules/config.rs` 的 `UserConfig` 中。
- `src-tauri/src/modules/process.rs` 已把全局代理注入到受管进程的环境变量。
- `src-tauri/src/modules/codex_local_access.rs` 已支持 API 服务上游模式：直连或跟随全局代理。
- 当前数据目录由 `src-tauri/src/modules/data_dir.rs` 管理，默认位于用户目录下 `.antigravity_cockpit`。
- 项目已有 `rusqlite`、`reqwest`、`tokio`、`serde_json` 等依赖，可支撑代理池数据库和异步网络测试。

因此实现路线不是直接复制 Wails/Go 代码，而是把 Trace-Browser 的能力重新落到 Rust/Tauri 架构中。

## 设计原则

1. 不一次性大改。每个阶段只交付一个可验收能力。
2. 不跳过验收。阶段验收未通过，不进入下一阶段。
3. 优先复用当前项目已有网络配置、数据目录、日志、命令注册、前端样式与多语言机制。
4. xray/sing-box 只作为本地子进程运行，不修改系统代理，不启用 TUN/VPN/透明代理。
5. 所有代理凭据必须脱敏显示和脱敏记录日志。
6. 订阅拉取、IP 健康检查属于外部网络请求，必须在 UI 上给用户明确动作入口，不能偷偷后台调用第三方服务。
7. 运行时二进制必须有来源、版本、sha256 校验和平台映射，不接受无清单的裸文件。

## 总体架构

### 后端模块

新增 Rust 模块建议：

- `src-tauri/src/modules/proxy_pool/mod.rs`
- `src-tauri/src/modules/proxy_pool/models.rs`
- `src-tauri/src/modules/proxy_pool/store.rs`
- `src-tauri/src/modules/proxy_pool/parser.rs`
- `src-tauri/src/modules/proxy_pool/subscription.rs`
- `src-tauri/src/modules/proxy_pool/runtime.rs`
- `src-tauri/src/modules/proxy_pool/gateway.rs`
- `src-tauri/src/modules/proxy_pool/health.rs`
- `src-tauri/src/commands/proxy_pool.rs`

当前功能先落在 `src-tauri`。只有当 CLI 也需要代理池能力时，再把纯业务模块迁入 `crates/cockpit-core`，避免现在扩大改动面。

### 前端模块

新增建议：

- `src/types/proxyPool.ts`
- `src/services/proxyPoolService.ts`
- `src/components/proxy-pool/*`
- `src/pages/ProxyPoolPage.tsx` 或 `src/pages/settings/ProxyPoolSection.tsx`

入口放在“设置 > 网络服务”中：先展示内置代理服务状态、当前出口、管理按钮；节点池管理界面使用单独页面或模态工作区，避免把网络设置页塞成巨型表单。

### 数据存储

使用独立 SQLite 数据库：

- 路径：`<data_dir>/proxy-pool/proxy_pool.db`
- 开启 WAL。
- 维护 `schema_migrations`。
- 不写入 `config.json` 的大数组，避免频繁测速和健康检查导致主配置文件膨胀。

核心表：

- `proxy_nodes`：节点 ID、名称、协议、原始配置、标准配置、分组、DNS、订阅来源、排序、启用状态、测速结果、IP 健康结果。
- `proxy_sources`：订阅 ID、URL、名称前缀、分组、DNS、自动刷新开关、刷新间隔、最后刷新时间、上次错误。
- `proxy_service_state`：内置代理服务开关、首选端口、实际端口、出口模式、当前活动节点、节点池已选节点列表、全局代理接入模式。

内置节点：

- `__direct__`：直连。
- `__local__`：本地代理 `http://127.0.0.1:7890`，作为兼容入口保留，但默认不强制启用。

### 协议支持范围

第一版支持：

- `http://`
- `https://`
- `socks5://`
- `vmess://`
- `vless://`
- `trojan://`
- `ss://`
- Clash YAML 中的 http/socks5/vmess/vless/trojan/ss。
- sing-box 路线：`hysteria2://`、`hysteria://`、`tuic`、`anytls://`、Clash YAML 中对应类型。
- Base64 订阅和逐行分享链接。

第一版明确不支持：

- SSR。
- TUN 模式。
- 系统级代理开关。
- 局域网共享代理。
- 自动下载未知来源内核。

## 运行时内核落地方案

这里的“内核”指代理运行时二进制：`xray` 与 `sing-box`。它们不是 Rust 代码的一部分，而是由当前项目校验、落盘、启动、监控和清理的本地子进程。

### 平台映射

统一使用 Rust/Tauri 目标名作为项目内部 target，避免 Go 项目的 `darwin-amd64` / `linux-arm64` 命名在 Rust 侧来回转换。

| 平台 | Rust/Tauri target | xray 文件名 | sing-box 文件名 |
| --- | --- | --- | --- |
| Windows x64 | `windows-x86_64` | `xray.exe` | `sing-box.exe` |
| macOS Intel | `darwin-x86_64` | `xray` | `sing-box` |
| macOS Apple Silicon | `darwin-aarch64` | `xray` | `sing-box` |
| Linux x64 | `linux-x86_64` | `xray` | `sing-box` |
| Linux arm64 | `linux-aarch64` | `xray` | `sing-box` |

macOS universal 包必须同时包含 `darwin-x86_64` 与 `darwin-aarch64`，运行时用 `std::env::consts::ARCH` 选择当前机器对应内核。

### 仓库目录

建议新增：

```text
src-tauri/proxy-runtime/
  runtime-sources.json
  runtime-manifest.json
  bin/
    windows-x86_64/
      xray.exe
      sing-box.exe
    darwin-x86_64/
      xray
      sing-box
    darwin-aarch64/
      xray
      sing-box
    linux-x86_64/
      xray
      sing-box
    linux-aarch64/
      xray
      sing-box
```

`runtime-sources.json` 记录上游归档来源与归档 sha256；`runtime-manifest.json` 记录项目实际打包的二进制 sha256。后续刷新内核只能通过脚本更新这两个清单，不能手工替换裸文件。

### 清单格式

`runtime-manifest.json` 建议结构：

```json
{
  "schemaVersion": 1,
  "files": [
    {
      "runtime": "xray",
      "version": "26.2.6",
      "target": "darwin-aarch64",
      "path": "bin/darwin-aarch64/xray",
      "sha256": "<extracted-binary-sha256>"
    }
  ]
}
```

`runtime-sources.json` 建议结构：

```json
{
  "schemaVersion": 1,
  "sources": [
    {
      "runtime": "xray",
      "version": "26.2.6",
      "target": "darwin-aarch64",
      "archiveType": "zip",
      "url": "https://github.com/XTLS/Xray-core/releases/download/v26.2.6/Xray-macos-arm64-v8a.zip",
      "archiveSha256": "<archive-sha256>",
      "archiveBinaryPath": "xray",
      "destPath": "bin/darwin-aarch64/xray"
    }
  ]
}
```

第一版可沿用 Trace-Browser 已固定的 xray `26.2.6` 与 sing-box `1.12.17` 清单思路；如需升级版本，必须单独开“内核版本升级”任务，并重新验收所有平台清单。

### 打包方式

当前 `src-tauri/tauri.conf.json` 已使用 `bundle.resources` 打包资源。内核也走资源路径，不走系统 PATH。

建议新增脚本：

- `scripts/prepare-proxy-runtime.cjs`
- `scripts/verify-proxy-runtime.cjs`

`prepare-proxy-runtime.cjs` 负责在 `npm run tauri` 和 CI 构建前准备资源目录：

```text
src-tauri/proxy-runtime-bundle/
  runtime-manifest.json
  bin/<target>/xray
  bin/<target>/sing-box
```

Tauri 资源配置只打包 `src-tauri/proxy-runtime-bundle`。这样每个平台安装包只携带自己需要的内核；macOS universal 携带两个 macOS target。

目标选择规则：

- 本地 dev：默认使用当前 host target。
- CI build matrix：通过 `PROXY_RUNTIME_TARGETS` 明确传入。
- macOS universal：`PROXY_RUNTIME_TARGETS=darwin-x86_64,darwin-aarch64`。

构建矩阵后续需要补充：

| matrix label | `PROXY_RUNTIME_TARGETS` |
| --- | --- |
| `windows-latest` | `windows-x86_64` |
| `macos-aarch64` | `darwin-aarch64` |
| `macos-x86_64` | `darwin-x86_64` |
| `macos-universal` | `darwin-x86_64,darwin-aarch64` |
| `ubuntu-22.04` | `linux-x86_64` |
| `ubuntu-24.04-arm` | `linux-aarch64` |

### 运行时解析流程

新增 `src-tauri/src/modules/proxy_pool/runtime.rs`，提供统一解析器：

1. 识别当前 target。
2. 读取内置 `runtime-manifest.json`。
3. 查找当前 target 下的 `xray` 与 `sing-box` 条目。
4. 优先检查用户显式覆盖路径：
   - `COCKPIT_XRAY_PATH`
   - `COCKPIT_SING_BOX_PATH`
5. 若无覆盖路径，从 Tauri resource dir 找到打包内核。
6. 校验 sha256。
7. 复制到用户数据目录的运行时缓存：

```text
<data_dir>/proxy-runtime/cache/
  <target>/
    xray/<sha256>/xray[.exe]
    sing-box/<sha256>/sing-box[.exe]
```

8. Unix 系统执行 `chmod 755`。
9. 再次校验缓存文件 sha256。
10. 执行 `<binary> version` 获取版本并返回状态。

所有后续桥接都从缓存路径启动，不直接从 app 安装目录或 AppImage 挂载目录启动。这样可以避开只读目录、路径空格、Linux AppImage 挂载权限和 macOS bundle 资源权限差异。

### 平台细节

Windows：

- 文件名必须带 `.exe`。
- 启动子进程时隐藏窗口。
- 清理时优先结束子进程；如果后续发现 xray/sing-box 会派生子进程，再补进程树清理。
- 不写入 Program Files，只运行数据目录缓存。

macOS：

- Intel 与 Apple Silicon 分开缓存。
- universal 包运行时按当前 CPU 架构选择内核。
- 打包发布时必须确保资源内可执行文件参与签名/公证流程；若签名策略不覆盖资源内可执行文件，阶段 7 必须补签名脚本。
- 开发态如果遇到 quarantine/xattr 问题，只提示用户修复本地文件，不在应用里静默绕过系统安全策略。

Linux：

- 缓存文件必须 `chmod 755`。
- AppImage/deb/rpm 都只从数据目录缓存启动。
- 如目标系统缺少运行所需 libc 或内核能力，显示“内核不可用”并保留应用可用，不阻塞启动主程序。

### 后端命令

阶段 1 必须先实现这些命令，不接入节点池：

- `proxy_runtime_get_status`
- `proxy_runtime_verify`
- `proxy_runtime_open_cache_dir`

返回结构建议：

```ts
interface ProxyRuntimeStatus {
  target: string
  runtimes: Array<{
    runtime: 'xray' | 'sing-box'
    expectedVersion: string
    manifestSha256: string
    sourcePath: string
    cachePath: string
    available: boolean
    executable: boolean
    detectedVersion: string
    error: string
  }>
}
```

### 内核阶段验收门禁

阶段 1 拆成三个功能点，必须逐个验收：

1. 清单与资源准备脚本。
   - 能生成 `proxy-runtime-bundle`。
   - 能拒绝缺失文件和 sha256 不匹配。
2. Rust runtime 解析与缓存。
   - 能找到当前平台内核。
   - 能复制到 `<data_dir>/proxy-runtime/cache`。
   - Unix 能设置执行权限。
3. 版本检测命令与 UI 状态。
   - 能返回 xray/sing-box 版本。
   - 缺失/不可执行/版本命令失败时 UI 有清晰错误。

完成这三个功能点前，不进入代理节点解析、代理池数据库或桥接开发。

## 桥接与内置代理服务

运行时层分两类：

1. 节点桥接：xray/sing-box 为高级节点启动本地 SOCKS 入站。
2. 应用代理网关：当前项目启动一个本地 HTTP CONNECT 代理，例如 `http://127.0.0.1:<port>`，作为全局代理可注入的稳定入口。

全局代理语义调整：

- 网络服务中的“全局代理”不再让用户手填任意代理地址，而是作为“启用内置代理网关”的开关。
- `global_proxy_url` 继续作为兼容字段保存，但由系统自动写入内置代理网关地址，例如 `http://127.0.0.1:7897`。
- 用户导入节点后，不会自动把全部节点作为出口；出口模式由 `proxy_service_state.outlet_mode` 管理，只允许 `direct`、`local`、`node_pool` 三选一。
- `direct` 模式只启用内置直连节点；`local` 模式只启用内置本地代理节点；`node_pool` 模式停用直连和本地代理，只启用 `selected_node_ids_json` 中用户多选的普通节点。
- `node_pool` 模式允许多个普通节点作为候选出口，`current_node_id` 表示当前活动节点，后续自动故障切换只能在已选节点池内切换。
- 内置“本地代理 127.0.0.1:<port>”节点保留，默认端口 `7890`，用户可修改端口后选择该节点，以接入 Clash、其他代理软件或自定义本地代理。
- Codex API 服务的“跟随全局代理”必须读取同一个 `global_proxy_url`，因此也跟随内置代理网关。

启用内置代理网关时：

- `process.rs` 注入的 HTTP_PROXY/HTTPS_PROXY/ALL_PROXY 指向内置网关。
- `codex_local_access.rs` 的“跟随全局代理”也必须解析到同一内置网关。
- 网关根据当前选择节点自动决定直连、标准代理、xray 桥接或 sing-box 桥接。

网关转发功能落地前，必须先完成这层配置与 UI 语义调整，保证之后实现 HTTP CONNECT 网关时不会再调整用户理解模型。

## 分阶段开发计划

### 阶段 0：方案验收

交付物：

- 本文档。

验收标准：

- 用户确认阶段划分、支持范围、验收规则可接受。
- 用户明确回复可以继续后，才能进入阶段 1。

### 阶段 1：运行时内核与路径解析

交付物：

- xray/sing-box 多平台清单。
- 当前平台 runtime 解析器。
- 运行时 sha256 校验。
- 后端命令：获取运行时状态、校验运行时、返回版本信息。

验收标准：

- 当前开发机可执行 `xray version` 和 `sing-box version`。
- 缺失、不可执行、校验失败时有清晰错误。
- 不启动任何代理节点。

### 阶段 2：代理池数据库与基础 CRUD

交付物：

- `proxy_pool.db`、迁移、DAO。
- 后端命令：列表、分组、保存、删除、批量删除、启用/禁用。
- 前端基础代理池界面。
- 支持手动添加 `http`、`https`、`socks5`。

验收标准：

- 重启应用后节点仍存在。
- 内置直连节点不可误删。
- UI 可按名称、协议、分组搜索。
- 代理凭据在列表和日志中脱敏。

### 阶段 3：订阅与添加资源

交付物：

- Clash YAML 解析。
- Base64 订阅/分享链接解析。
- URL 订阅拉取。
- 订阅来源管理：编辑 URL、分组、名称前缀、DNS、删除来源。
- 刷新单个订阅、刷新全部订阅。

验收标准：

- URL 订阅导入后生成稳定 `source_id`。
- 刷新订阅时只替换该订阅来源下的节点。
- 已删除订阅节点不会残留。
- 解析失败不破坏现有节点。
- 支持导入预览和选择性导入。

### 阶段 3.5：全局代理与内置网关配置语义

交付物：

- 网络服务“全局代理”改为“启用内置代理网关”。
- 后端代理池服务状态：启用状态、网关端口、出口模式、节点池已选节点、当前活动节点、本地代理节点端口。
- 本地 HTTP/CONNECT 代理网关监听 `127.0.0.1:<gateway_port>`。
- 网关转发支持当前已可直接落地的出口：直连、HTTP 上游代理、SOCKS5 上游代理。
- 保存网络服务配置时自动同步 `global_proxy_url = http://127.0.0.1:<gateway_port>`。
- 代理节点池提供 `直连 / 本地代理 / 节点池` 三选一出口模式；节点池模式下普通节点可多选，导入节点不自动成为出口。
- 内置“本地代理 127.0.0.1”节点默认端口 `7890`，并允许用户修改。
- vmess/vless/trojan/ss/hysteria2/tuic/anytls 等高级节点先返回清晰错误，进入阶段 4 后通过 xray/sing-box 桥接为本地 SOCKS 出口。

验收标准：

- 开启全局代理后，受管进程和 Codex API 的“跟随全局代理”都拿到内置网关地址。
- 修改网关端口后，`global_proxy_url` 自动同步。
- 修改本地代理端口后，内置本地代理节点名称、端口和配置同步更新。
- 切换 `direct` 或 `local` 时必须清空节点池选择，并停用另两个出口类型。
- 切换 `node_pool` 时必须停用直连和本地代理，只启用用户多选的普通节点；`current_node_id` 必须属于已选节点池。
- 节点池内可手动切换当前活动节点；后续自动故障切换只能在 `selected_node_ids_json` 中寻找备用节点。
- 外部代理软件只需监听本机端口，并在代理池选择“本地代理”节点即可接入。
- 使用直连、HTTP 或 SOCKS5 出口时，`curl -x http://127.0.0.1:<gateway_port> https://example.com` 可通过网关完成 CONNECT 转发。

### 阶段 4：自动桥接与内置代理网关

交付物：

- xray 桥接：vmess/vless/trojan/ss 自动生成本地 SOCKS 入站并作为网关上游。
- sing-box 桥接：hysteria/hysteria2/tuic/anytls 自动生成本地 SOCKS 入站并作为网关上游。
- 本地 HTTP CONNECT 代理网关统一转发到直连、标准 HTTP/SOCKS5 或内核桥接 SOCKS 出口。
- 当前活动节点切换后，网关下一次请求立即使用新出口；自动故障切换只允许在节点池已选节点内轮换。
- 桥接配置写入 `<data_dir>/proxy-pool/bridge`，桥接内核使用运行时缓存中的 xray/sing-box。

验收标准：

- 选择标准 HTTP/SOCKS 节点时不启动 xray/sing-box。
- 选择高级节点时自动启动对应内核。
- 停用内置代理服务会清理子进程。
- `global_proxy_enabled` 能让受管进程拿到内置网关地址。
- Codex API 服务的“跟随全局代理”也走内置网关。
- 对高级节点执行 `curl -x http://127.0.0.1:<gateway_port> https://example.com` 时，网关能通过桥接出口完成 CONNECT 转发。

### 阶段 5：测速与 IP 健康检查

交付物：

- 单个节点测速。
- 批量“测试全部”，并发可控。
- 导入预览阶段临时测速。
- 单个/批量 IP 健康检查。
- 前端实时进度事件。

验收标准：

- 测速结果持久化到节点。
- 批量任务可显示进行中、成功、失败、超时。
- IP 健康检查结果显示出口 IP、地区、ASN/组织、风险字段和原始 JSON。
- 第三方 IP 健康请求必须由用户点击触发。

### 阶段 6：订阅自动刷新与出口策略

交付物：

- 订阅自动刷新调度器。
- 刷新间隔配置。
- 手动切换当前出口。
- 可选自动轮换出口：按分组、按健康节点优先、按间隔。

验收标准：

- 自动刷新不会并发刷新同一订阅。
- 刷新失败保留旧节点并记录错误。
- 自动轮换不会选择禁用节点。
- 关闭应用时所有调度器和子进程退出。

### 阶段 7：打包与跨平台验证

交付物：

- Windows/macOS/Linux 的 runtime 打包配置。
- 发布脚本校验 runtime 清单。
- 文档更新：使用说明、故障排查。

验收标准：

- 每个平台构建包只包含对应平台 runtime。
- runtime 缺失时应用可启动，但代理服务显示不可用。
- 至少完成当前平台的端到端验证；其他平台保留清单级和路径级测试，等待对应机器实测。

## 剩余功能逐步开发计划

本节用于约束 2026-05-23 之后继续开发的剩余代理能力。以下任务必须按顺序推进；每个功能点完成后必须先交付验收结果，只有用户明确回复“继续”或同等明确指令，才能进入下一个功能点。

当前已确认的剩余功能：

- HTTPS 上游代理在内置网关中的真正转发支持。
- 订阅自动刷新调度器。
- 自动故障切换与当前出口持久化。
- 导入预览阶段的临时延迟测速与 IP 健康检测。
- 批量测速 / IP 健康后端实时进度事件。

当前执行状态（2026-05-23）：

- 剩余阶段 1 已落地：HTTPS 上游代理可在内置网关中执行 TLS 连接、CONNECT 和普通 HTTP 转发。
- 剩余阶段 2 已落地：批量延迟测速和批量 IP 健康检查支持后端实时进度事件。
- 剩余阶段 3 已落地：导入预览节点支持用户点击触发的临时延迟测速和 IP 健康检测，检测结果仅保留在预览区。
- 剩余阶段 4 已落地：节点池请求级故障切换成功后会在安全条件内持久化当前出口，并向前端同步当前出口展示。
- 下一步必须等待用户验收确认后，再进入剩余阶段 5：订阅自动刷新调度器。

### 剩余阶段 1：HTTPS 上游代理支持

目标：

- `https://host:port` 类型的上游代理必须可作为内置代理网关出口使用。
- 该能力只表示“连接到 HTTPS 代理服务器本身时使用 TLS”，不是把目标站点强行走 HTTPS。
- HTTP 请求和 CONNECT 请求都必须能复用 HTTPS 上游代理。

实现范围：

- 后端网关：`src-tauri/src/modules/proxy_pool/gateway.rs`。
- 必要时补充 TLS helper：优先在 `gateway.rs` 内部实现，复杂度上升时再拆到 `src-tauri/src/modules/proxy_pool/tls.rs`。
- 复用现有 `tokio-rustls` / `rustls` 依赖；如果需要系统根证书，再新增明确依赖并说明原因。
- 继续保留 HTTP 上游代理、SOCKS5、直连、mihomo 桥接的现有行为。

技术方案：

- 新增 `connect_https_proxy(outbound)`，先 TCP 连接上游代理，再用 TLS 包裹连接。
- TLS SNI 使用上游代理 `host`；IP 地址作为 host 时必须有清晰错误或安全兜底。
- CONNECT 转发时，在 TLS 流内发送标准 `CONNECT target:port HTTP/1.1` 请求。
- 普通 HTTP 转发时，在 TLS 流内发送 absolute-form HTTP 请求。
- 代理认证继续复用现有 `Proxy-Authorization` 构造逻辑。
- 错误信息要能区分：TCP 连接失败、TLS 握手失败、代理 CONNECT 返回非 2xx。

验收标准：

- 选择 HTTPS 代理节点后，`curl -x http://127.0.0.1:<gateway_port> https://example.com` 能通过内置网关转发。
- HTTPS 上游代理账号密码可用，日志不泄露完整凭据。
- HTTP / SOCKS5 / 高级节点桥接不发生回归。
- HTTPS 上游代理不可用时，节点池备用节点仍可继续尝试。
- Windows/macOS/Linux 代码路径一致，不引入仅 Windows 可用的实现。

完成后暂停点：

- 输出改动文件、验证命令、手动验收方法。
- 等待用户确认后再进入剩余阶段 2。

### 剩余阶段 2：后端实时进度事件基础与批量测速 / IP 健康进度

目标：

- 批量“测试全部”和“检查 IP”不再只等最终结果；后端每完成一个节点就向前端发送进度事件。
- 先完成已存在批量任务的实时进度，再扩展到导入预览阶段。

实现范围：

- 后端命令：`src-tauri/src/commands/proxy_pool.rs`。
- 后端检测逻辑：`src-tauri/src/modules/proxy_pool/store.rs`、`src-tauri/src/modules/proxy_pool/health.rs`。
- 前端服务与类型：`src/services/proxyPoolService.ts`、`src/types/proxyPool.ts`。
- 前端 UI：`src/pages/settings/ProxyPoolSection.tsx`、`src/pages/settings/Settings.css`。

事件设计：

- 事件名使用稳定前缀，例如 `proxy_pool://check_progress`。
- 事件 payload 至少包含：
  - `taskId`
  - `kind`: `latency` 或 `ip_health`
  - `phase`: `started` / `node_done` / `finished`
  - `nodeId`
  - `ok`
  - `latencyMs`
  - `ipHealthSummary`
  - `error`
  - `completed`
  - `total`
- 前端发起批量任务时生成或接收 `taskId`，只消费当前任务事件，避免历史事件串台。

技术方案：

- 批量检测仍保持并发上限，但每个节点完成后立即 emit 事件。
- 高级节点检测仍串行或低并发，避免同时启动多个临时内核导致端口和资源压力。
- 最终命令响应仍保留完整快照，用于兜底刷新 UI。
- 前端收到事件后局部更新对应节点状态、进度计数和错误摘要。

验收标准：

- 批量延迟测试中，列表节点能逐个显示测试中、成功、失败。
- 批量 IP 健康中，眼睛详情数据能在单个节点完成后更新。
- 关闭或切换页面后不会重复注册事件监听。
- 批量任务失败时仍返回最终错误摘要，不影响已完成节点结果持久化。

完成后暂停点：

- 输出事件 payload 示例、改动文件、验证结果。
- 等待用户确认后再进入剩余阶段 3。

### 剩余阶段 3：导入预览阶段临时测速与 IP 健康检测

目标：

- 添加资源或 URL 订阅预览后，用户可以对预览节点执行临时延迟测速与 IP 健康检测。
- 检测结果只附着在预览结果上，不写入 `proxy_nodes`，只有真正导入后才进入持久化节点。

实现范围：

- 预览模型：`src-tauri/src/modules/proxy_pool/models.rs`、`src/types/proxyPool.ts`。
- 预览检测命令：`src-tauri/src/commands/proxy_pool.rs`、`src/services/proxyPoolService.ts`。
- 检测复用：`src-tauri/src/modules/proxy_pool/health.rs`。
- 前端预览区：`src/pages/settings/ProxyPoolSection.tsx`。

技术方案：

- 新增预览检测请求，使用 `previewId` 和预览节点标准配置构造临时 `ProxyCheckTarget`。
- 用户可选择：
  - 只测速。
  - 只查 IP 健康。
  - 对当前勾选预览节点批量检测。
- 临时检测必须沿用 mihomo 临时桥接能力，检测结束后清理临时子进程和配置文件。
- IPPure 等第三方 IP 健康请求必须由用户点击触发，不能在预览完成后自动调用。

验收标准：

- 预览列表能显示临时延迟和 IP 健康摘要。
- 导入后默认不自动选择为出口节点。
- 导入后如需要保留预览检测结果，必须由导入请求显式携带；第一版可不持久化预览检测结果。
- 预览检测失败不影响导入勾选和导入动作。

完成后暂停点：

- 输出手动验收流程：粘贴订阅、预览、检测、导入所选。
- 等待用户确认后再进入剩余阶段 4。

### 剩余阶段 4：自动故障切换与当前出口持久化

目标：

- 节点池模式下，如果当前活动节点不可用，网关只能在用户已选择的节点中尝试备用节点。
- 某个备用节点转发成功后，应可按策略更新 `current_node_id`，让后续请求优先走成功节点。

实现范围：

- 网关候选逻辑：`src-tauri/src/modules/proxy_pool/gateway.rs`。
- 节点池状态：`src-tauri/src/modules/proxy_pool/store.rs`。
- 前端当前出口展示：`src/pages/settings/ProxyPoolSection.tsx`、`src/pages/SettingsPage.tsx`。

策略：

- 默认启用“请求级故障切换”：当前节点失败后尝试已选备用节点。
- 新增“成功备用节点持久化为当前出口”逻辑，但必须满足：
  - 只在 `outlet_mode = node_pool` 时执行。
  - 只写入 `selected_node_ids_json` 内已有节点。
  - 不把直连或本地代理自动切进节点池。
  - 不在所有节点失败时清空用户选择。
- 加入短冷却时间，避免两个并发请求反复互相覆盖当前出口。
- 后续可扩展“按延迟优先”或“按 IP 健康优先”，但第一版只做失败后成功备用节点置顶。

验收标准：

- 选中 A、B 两个节点，A 不可用、B 可用时，请求能自动用 B 成功。
- B 成功后当前出口显示变为 B。
- 用户手动切回 A 后，下一次请求仍先尝试 A。
- 所有节点不可用时，错误信息列出候选节点失败原因。
- 不选择任何普通节点时不会误启用节点池。

完成后暂停点：

- 输出故障切换日志样例和数据库状态变化。
- 等待用户确认后再进入剩余阶段 5。

### 剩余阶段 5：订阅自动刷新调度器

目标：

- 支持用户为订阅来源开启自动刷新，并配置刷新间隔。
- 自动刷新只更新订阅节点，不自动执行 IP 健康检查，不偷偷调用第三方 IP 服务。

实现范围：

- 订阅来源模型与更新接口：`src-tauri/src/modules/proxy_pool/models.rs`、`src-tauri/src/modules/proxy_pool/store.rs`。
- 调度器：新增或扩展 `src-tauri/src/modules/proxy_pool/subscription_scheduler.rs`。
- 应用启动恢复：`src-tauri/src/lib.rs`。
- 前端订阅来源编辑：`src/pages/settings/ProxyPoolSection.tsx`。

技术方案：

- 复用 `proxy_sources.auto_refresh_enabled` 和 `refresh_interval_minutes` 字段。
- 应用启动后启动单例调度器，按来源计算下一次刷新时间。
- 每个 source 必须有独立锁，避免同一订阅并发刷新。
- 刷新失败保留旧节点，写入 `last_error`，下次到点继续尝试。
- 删除订阅来源后调度器要停止该来源任务。
- 用户修改订阅 URL、分组、名称前缀或刷新间隔后，调度器立即重载。

验收标准：

- 开启自动刷新并设置短间隔后，到点能刷新订阅来源。
- 刷新失败时旧节点不丢失，来源卡片显示最后错误。
- 应用重启后自动刷新配置仍生效。
- 删除订阅来源后不会继续请求已删除 URL。
- 关闭应用时调度器任务退出，不残留后台子进程。

完成后暂停点：

- 输出调度器状态、验收结果和下一步建议。
- 等待用户确认后再考虑更高级的出口轮换策略。

## 必测场景

每个阶段至少运行：

- `npm run typecheck`
- `cargo fmt --check`
- 相关 Rust 单元测试或集成测试

涉及前端页面时还需要：

- 本地启动应用或 Vite/Tauri 开发服务。
- 检查桌面宽度和窄屏布局。
- 确认按钮文字不溢出，表格空状态、加载态、失败态完整。

涉及 runtime 或网关时还需要：

- 子进程启动失败路径。
- 端口被占用路径。
- 应用退出清理路径。
- 节点凭据脱敏日志。

## 风险与处理

- xray/sing-box 版本兼容风险：通过 manifest 固定版本和 sha256，不在运行时自动更新。
- macOS 可执行权限与签名风险：打包阶段必须校验 `chmod +x`、资源路径、Gatekeeper 影响。
- 代理凭据泄露风险：所有日志和 UI 默认脱敏，详情查看也不展示完整密码。
- 大订阅性能风险：订阅拉取限制大小，解析和批量测试使用并发上限。
- 外部 IP 健康服务可用性风险：失败只记录错误，不影响节点原始配置。
- 当前全局代理语义变更风险：保留手动模式，内置代理池作为新增来源，不破坏已有配置。

## 后续执行纪律

从阶段 1 开始，每次只开发一个阶段内的一个功能点。完成后必须输出：

- 改了哪些文件。
- 做了哪些验证。
- 当前功能点的验收结果。
- 下一步建议。

除非用户明确说“继续”，否则不得自动进入下一功能点。
