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
- `proxy_service_state`：内置代理服务开关、首选端口、实际端口、当前节点、全局代理接入模式。

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

全局代理新增来源模式：

- `manual`：继续使用现有 `global_proxy_url`。
- `proxy_pool`：使用内置代理网关的实际地址。

当选择 `proxy_pool` 时：

- `process.rs` 注入的 HTTP_PROXY/HTTPS_PROXY/ALL_PROXY 指向内置网关。
- `codex_local_access.rs` 的“跟随全局代理”也必须解析到同一内置网关。
- 网关根据当前选择节点自动决定直连、标准代理、xray 桥接或 sing-box 桥接。

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

### 阶段 4：自动桥接与内置代理网关

交付物：

- xray 桥接：vmess/vless/trojan/ss。
- sing-box 桥接：hysteria2/tuic/anytls。
- 本地 HTTP CONNECT 代理网关。
- 网络服务设置中新增“全局代理来源：手动地址 / 内置代理池”。
- 当前节点切换后，网关立即使用新出口。

验收标准：

- 选择标准 HTTP/SOCKS 节点时不启动 xray/sing-box。
- 选择高级节点时自动启动对应内核。
- 停用内置代理服务会清理子进程。
- `global_proxy_enabled + proxy_pool` 能让受管进程拿到内置网关地址。
- Codex API 服务的“跟随全局代理”也走内置网关。

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
