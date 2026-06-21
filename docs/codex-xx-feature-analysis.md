# Codex `xx` 分支功能借鉴与 main 落地分析

日期：2026-06-22

本文对比当前 `main` 分支与本地 `xx` 分支。`xx` 跟踪的是 `origin/main`，远程地址为 `github.com/jlcodes99/cockpit-tools`。目标是分析 `xx` 中 Codex 相关、当前 `main` 没有或可以增强的能力，并给出适合落地到 `main` 的优先级和实施路径。

结论先说：不建议把 `xx` 整体合并进 `main`。两个分支在品牌、发布流程、Claude 平台、代理运行时、图标资源、系统模块等大量非 Codex 区域都有差异。Codex 能力应按功能点拆分，小步迁移。

## 当前 main 基线

当前 `main` 已具备比较完整的 Codex 能力：

- Codex OAuth / API Key 账号管理。
- Codex API 服务账号池、供应商池、融合/账号池/供应商来源模式、WebSocket 模式、调度策略、统计、绑定 OAuth、LAN/本机访问、速度控制、更新配置。
- Codex 账号总览、账号分组、配额刷新、自动切号、本地 API 服务成员管理。
- 新增的“切换 API”操作，可在从列表成员账号切换后，一键改回由 API 服务提供账号。
- Codex 实例、会话、唤醒相关页面与后端能力。

`xx` 中 Codex API 服务和会话管理更像后续演进版本，值得借鉴的点主要集中在模型供应商协议能力、API 服务独立页面、请求日志、命名 API Key、模型路由、Sidecar 网关、配置写入安全性、会话修复等方向。

## 建议优先吸收的能力

### 1. 模型供应商协议能力与 API 接入转换：Responses / Chat Completions

这是第一优先级。当前 `main` 的 New API / 模型供应商管理缺少显式“协议能力”处理，容易把不同供应商都按同一种 Codex Responses 协议处理。`xx` 在这块做了系统增强，值得借鉴的是协议字段、协议选择和协议透传机制，而不是在 `main` 里对某个具体供应商做特殊固定。

这里要特别强调：协议能力不能只落成字段和 UI。要想按本文档做出来的 `main` 增强真正可用，必须同时落地 API 接入转换层。也就是：

- 面向 Codex / 客户端时，仍能接受 Codex 常用的 Responses 风格请求。
- 面向只支持 Chat Completions 的上游供应商时，要能把请求转换为 Chat Completions 兼容形态。
- 当本地 API 服务对外暴露 Chat Completions 接口时，也要能把客户端 Chat Completions 请求映射到内部 Responses 路由，再把 Responses 结果转换回 Chat Completions 响应。

`xx` 相关文件：

- `src/types/codex.ts`
- `src/services/codexModelProviderService.ts`
- `src/components/codex/CodexModelProviderManager.tsx`
- `src/pages/CodexAccountsPage.tsx`
- `src-tauri/src/models/codex.rs`
- `src-tauri/src/modules/codex_account.rs`
- `src-tauri/src/modules/codex_local_access.rs`
- `src-tauri/src/commands/codex.rs`

`xx` 的核心设计：

- 前端类型新增 `CodexProviderWireApi = "responses" | "chat_completions"`。
- 模型供应商保存 `wireApi`，Codex API Key 账号保存 `api_wire_api`。
- 模型供应商表单提供“协议”选择：
  - `responses`：Responses 原生协议。
  - `chat_completions`：OpenAI Chat Completions 兼容协议。
- 后端只接受合法协议值：`responses` / `chat_completions`。
- 后端会根据显式选择保存协议能力；`main` 首阶段只对 URL 中明确包含 `/chat/completions` 的端点推断为 `chat_completions`，其他默认 `responses`。
- 对 `chat_completions` 协议的 API Key 账号，`xx` 通过 provider gateway 转换请求，使 Codex 侧仍能使用服务。
- `main` 落地时不应固定识别 `https://api.apikey.fun/v1`；它的初始协议值应与 New API / 普通模型供应商保持同一套默认逻辑，并允许用户显式修改。

`xx` 中可借鉴的 API 转换点：

- 请求识别：
  - `is_responses_request(...)` 识别 `/v1/responses`。
  - `is_chat_completions_request(...)` 识别 `/v1/chat/completions` 或以 `/chat/completions` 结尾的路径。
- Chat Completions -> Responses：
  - `build_responses_body_from_chat_completions(...)` 将 `messages`、`tools`、`tool_choice`、`response_format`、`service_tier` 等转换成 Responses 请求体。
  - `system` role 映射为 Responses 侧更合适的 `developer`。
  - `tool` 消息映射为 `function_call_output`。
  - assistant 的 `tool_calls` 映射为 Responses `function_call`。
  - 过长工具名会缩短，并保留原名映射，用于响应阶段还原。
- Responses -> Chat Completions：
  - `build_chat_completion_payload(...)` 将 Responses 非流式结果转成 `chat.completion` 响应。
  - 输出文本转为 assistant `message.content`。
  - reasoning 摘要转为 `reasoning_content`。
  - Responses `function_call` 转回 Chat Completions `tool_calls`。
  - usage 转成 `prompt_tokens`、`completion_tokens`、`cached_tokens`、`reasoning_tokens`。
- Responses SSE -> Chat Completions SSE：
  - 流式转换器把 `response.output_text.delta` 转为 Chat Completions delta content。
  - 把 `response.reasoning_summary_text.delta` 转为 delta reasoning_content。
  - 把 `response.output_item.added`、`response.function_call_arguments.delta/done` 转为 delta tool_calls。
  - 结束时补 `data: [DONE]`。

`main` 当前缺口：

- `main` 中 Codex provider 写 `config.toml` 时基本固定 `wire_api = "responses"`。
- New API 供应商没有持久化 `wireApi` / `api_wire_api`。
- 供应商测试、导入 API Key、快速切换供应商时无法表达“这个供应商到底支持 Responses 原生还是 Chat Completions 兼容”。
- 对模型供应商没有协议能力字段，只能依赖默认路径。

为什么必须优先处理：

- 这是所有后续模型供应商增强的基础。如果协议能力不正确，请求日志、模型路由、供应商测试、模型别名、价格统计都可能建立在错误调用链上。
- New API 供应商往往不是完全一致的协议形态，有的支持 Responses，有的只支持 Chat Completions，有的是 Codex/Responses 包装服务。
- 用户导入 API Key 后，如果协议判断错，会出现模型列表可见但请求失败、流式输出异常、工具调用格式不兼容等问题。

建议落地方式：

1. 先补类型与存储：
   - 前端 `CodexModelProvider` 增加 `wireApi?: "responses" | "chat_completions"`。
   - 后端 `CodexAccount` 增加 `api_wire_api?: "responses" | "chat_completions"`。
   - 数据读取时兼容旧数据，缺失时按 Base URL 自动推断。
2. 补供应商能力推断：
   - 抽出 `resolveProviderWireApi(baseUrl, explicitWireApi)`。
   - 不对 `https://api.apikey.fun/v1` 做固定识别；它与 New API 的初始协议默认值保持一致。
   - 不做供应商域名级强制识别；只有明确 `/chat/completions` 路径才自动推断为 Chat Completions。
   - 用户仍可手动覆盖协议能力。
3. 补 UI：
   - 模型供应商新增/编辑表单增加“协议”选择。
   - 供应商卡片展示当前协议。
   - Chat Completions 协议旁显示提示：需要通过本地 provider gateway 适配 Codex。
4. 补账号导入链路：
   - 从模型供应商创建 Codex API Key 账号时，把 `wireApi` 写入 `api_wire_api`。
   - 快速切换供应商时同步更新账号的 `api_wire_api`。
   - APIKEY.FUN / New API 预填或导入时使用同一套初始协议默认值，不做供应商级强制锁定。
5. 补后端路由：
   - `responses` 账号继续走现有直连/本地 API 服务逻辑。
   - `chat_completions` 账号必须走 provider gateway / adapter，不应假装是 Responses 原生账号。
   - provider gateway 需要根据账号的 `api_wire_api` 选择上游协议。
   - 如果客户端请求 `/v1/responses`，但目标上游是 `chat_completions`，要在 gateway 内完成 Responses -> Chat Completions 的上游请求适配，并把上游结果转回 Responses 语义，保证 Codex 客户端不感知差异。
   - 如果客户端请求 `/v1/chat/completions`，但内部服务主路径是 Responses，应支持 Chat Completions -> Responses -> Chat Completions 的完整闭环。
   - 第一阶段不要只做“识别后报错”。识别后报错只能用于临时保护，不算完成协议增强。
6. 补转换边界：
   - 请求体必须覆盖 `messages`、多模态 content parts、`tools`、`tool_choice`、`response_format`、`stream`、`service_tier`。
   - 响应体必须覆盖普通文本、reasoning、tool calls、usage、finish_reason。
   - 流式响应必须覆盖文本 delta、reasoning delta、tool call delta、完成事件和 `[DONE]`。
   - 错误响应要保留上游 status/code/message，并转换为当前客户端可理解的格式。
7. 补测试：
   - APIKEY.FUN 与 New API 的初始协议默认值一致。
   - 普通 `/v1` 端点默认 `responses`，明确 `/chat/completions` 端点默认 `chat_completions`。
   - 显式选择优先于自动推断。
   - 快速切换供应商后账号 `api_wire_api` 正确更新。
   - Chat Completions 请求能转换为 Responses 请求体。
   - Responses 普通响应能转换为 Chat Completions 响应。
   - Responses SSE 能转换为 Chat Completions SSE。
   - function/tool call 名称缩短后能在响应里还原。
   - `service_tier`、usage、reasoning token 不丢失。

风险：

- 如果第一阶段只加 UI/存储但不加 provider gateway，Chat Completions 供应商可能仍无法被 Codex 正常使用。
- 如果只做协议字段、不做请求/响应转换，会出现“供应商看起来已支持，但实际请求仍失败”的半成品状态。
- 自动推断名单需要保守，不能把支持 Responses 的服务误判为 Chat Completions。
- 旧账号迁移要谨慎，尤其是已经能正常工作的 API Key 账号，不应被错误改协议。

推荐第一阶段验收标准：

- `main` 中模型供应商和 Codex API Key 账号能保存协议字段。
- APIKEY.FUN 导入/预填/快速切换后的协议初始值与 New API 保持一致，且用户可以显式修改。
- New API 供应商卡片能显示协议能力。
- 缺失协议的旧数据可自动推断，但不破坏现有可用账号。
- Chat Completions 供应商不仅能被识别，还能通过 adapter 完成一次非流式文本请求。
- Chat Completions 供应商能通过 adapter 完成一次流式文本请求。
- function/tool call 请求在转换后仍能完成一轮调用。
- Responses 与 Chat Completions 两种客户端入口都能返回对应格式的响应。

优先级：最高，第一优先顺序处理。

### 2. 独立 Codex API 服务页面

`xx` 相关文件：

- `src/pages/CodexApiServicePage.tsx`
- `src/pages/CodexApiServicePage.css`
- `src/components/codex/CodexServicePanelModal.tsx`

能力概览：

- API 服务独立页面，不再把所有服务控制都塞在 Codex 账号总览或弹窗里。
- 分页签管理概览、密钥、账号、模型、日志。
- 支持聊天测试、流式测试、请求日志筛选、模型价格与模型路由管理。

落地价值：

- 当前 `main` 的 API 服务功能已经较多，继续放在总览页会越来越拥挤。
- 独立页面更适合长期维护、排查问题和配置高级能力。

建议落地方式：

1. 先落页面骨架和只读状态，不急着迁移所有写操作。
2. 第一阶段只展示当前 `main` 已有的服务状态、地址、密钥、统计、测试入口。
3. 等后端具备请求日志、命名 Key、模型路由后，再逐步打开对应页签。

风险：

- `xx` 页面依赖大量 `main` 当前没有的类型和命令，不能直接复制。
- 需要避免和现有 `CodexLocalAccessModal` 形成两套重复配置入口。

优先级：高，分阶段。

### 3. 命名 API Key 与按 Key 管理策略

`xx` 新增命令：

- `codex_local_access_create_api_key`
- `codex_local_access_update_api_key`
- `codex_local_access_rotate_named_api_key`
- `codex_local_access_delete_api_key`

相关文件：

- `src/services/codexLocalAccessService.ts`
- `src/types/codexLocalAccess.ts`
- `src-tauri/src/models/codex_local_access.rs`
- `src-tauri/src/modules/codex_local_access.rs`

能力概览：

- API 服务不再只有一个全局密钥。
- 可以创建多个有名称的客户端密钥。
- 可单独轮换、删除、配置策略。
- 日志中可记录 key id / label，方便追踪调用方。

落地价值：

- LAN/API 服务共享时，不同设备、脚本、客户端可以使用不同 Key。
- 某个客户端泄露或不用了，只需要删除一个 Key，不必重置整个服务。
- 为后续 per-key 限流、账号范围、权限控制打基础。

建议落地方式：

1. 保留当前 `apiKey` 作为默认 Key，新增 `apiKeys` 字段，做向后兼容迁移。
2. 先支持创建、重命名、轮换、删除。
3. 再支持每个 Key 的账号范围和策略。
4. UI 放到独立 API 服务页，不建议继续塞进账号总览工具栏。

风险：

- 旧客户端必须继续可用。
- 日志和 UI 不能泄露完整密钥。

优先级：高。

### 4. 请求日志与诊断能力

`xx` 新增命令：

- `codex_local_access_query_request_logs`

相关能力：

- 按请求类型区分：文本、图片生成、图片编辑、其他。
- 按成功/失败、网关模式、账号、模型、API Key 筛选。
- 记录 token、延迟、错误分类、请求 ID。
- Sidecar 事件也会回写到请求日志和账号健康状态。

落地价值：

- 当前 `main` 有统计，但缺少可查询的请求明细。
- 当某个客户端报错、某个账号失败、某个模型不可用时，请求日志能显著降低排查成本。

建议落地方式：

1. 加一个有保留期限的本地日志库或文件存储。
2. 默认只记录元数据、token、延迟、错误分类，不记录 prompt、图片、完整响应、API Key、OAuth token。
3. 先做日志查询后端，再在独立 API 服务页加“日志”页签。

风险：

- 隐私风险最高，必须明确不存敏感正文。
- 必须设置保留条数或保留天数，避免日志无限增长。

优先级：高。

### 5. Codex config.toml 格式化、清理与原子写入

`xx` 新增文件：

- `src-tauri/src/modules/codex_config_format.rs`

能力概览：

- 写入前规范化 `config.toml` 的空行和结构。
- 清理 TOML 文档中不合理的格式。
- 提供 Codex config 的原子写入工具。
- 写入/修复时可做审计日志。

落地价值：

- 当前 `main` 多处会写 Codex 配置：账号切换、API 服务激活、Provider 写入、快速配置、实例配置。
- 抽成统一工具后，能减少配置损坏风险，也让后续 diff 更稳定。

建议落地方式：

1. 先单独迁移为后端工具模块。
2. 补 TOML 输入/输出测试。
3. 再逐个替换现有写 config 的调用点。
4. 第一阶段必须保持现有语义不变，只改善写入安全性和格式稳定性。

风险：

- TOML 注释、顺序、空行如果处理不好，会影响用户手写配置体验。

优先级：高，且相对安全。

### 6. 会话文件 mtime 保护

`xx` 新增文件：

- `src-tauri/src/modules/codex_session_file_time.rs`

能力概览：

- 读取会话文件修改时间。
- 写入修复后恢复修改时间。
- 按毫秒精度比较修改时间。

落地价值：

- 会话可见性修复、跨实例同步、线程同步如果改写文件，可能会意外改变文件 mtime。
- mtime 变化可能导致会话排序变化，影响用户“最近会话”体验。

建议落地方式：

1. 先迁移这个工具模块。
2. 用到会话修复和同步写入路径。
3. 添加平台差异测试，尤其是 Windows/macOS/Linux 的时间精度。

风险：

- 低。主要风险是不同文件系统时间精度不同。

优先级：高。

## 建议中期增强的能力

### 7. 模型路由、模型别名、账号模型排除、价格配置

`xx` 相关模型/命令：

- `CodexLocalAccessAccountModelRule`
- `CodexLocalAccessModelAlias`
- `CodexLocalAccessModelPricing`
- `codex_local_access_update_account_model_rules`
- `codex_local_access_update_model_rules`
- `codex_local_access_update_model_pricings`

能力概览：

- 某些账号可排除特定模型。
- 支持模型别名和 fork 行为。
- 支持模型价格配置，用于统计或成本展示。

落地价值：

- 不同账号/供应商可能不支持同一批模型。
- 账号级模型排除可以减少错误重试。
- 模型别名可以让客户端使用稳定模型名，内部再映射到真实上游模型。

建议落地方式：

1. 先做“账号排除模型”，这是最直接改善路由可靠性的部分。
2. 再做模型别名。
3. 最后做价格配置，先只用于展示/统计，不参与自动调度。

风险：

- 模型别名会改变客户端请求实际到达的模型，必须保证规则可解释、可预测。
- 需要覆盖路由、回退、不可用模型等测试。

优先级：中高。

### 8. 超时预设与高级重试参数

`xx` 相关能力：

- `CodexLocalAccessTimeouts`
- `codex_local_access_update_timeouts`
- `codex_local_access_update_timeout_presets`
- Sidecar/legacy/WebSocket/上游发送/单账号状态等多类超时和重试参数。

落地价值：

- 不同网络、供应商、模型对超时的需求不一样。
- 图片生成、长流式响应适合更长等待；交互式文本请求适合更短失败反馈。

建议落地方式：

1. 先做两个内置预设：“短等待”和“长等待”。
2. 高级数值配置放到高级面板。
3. 请求日志中记录本次使用的预设，方便排查。

风险：

- 参数太多会让用户困惑。
- 默认值必须稳，不应让普通用户需要调参才能正常使用。

优先级：中。

### 9. 图片生成能力感知

`xx` 相关能力：

- `CodexLocalAccessImageGenerationMode`
- `CodexLocalAccessImageGenerationStatus`
- 图片生成/图片编辑请求类型。
- image_generation tool 注入/移除。
- 账号图片能力成功/失败统计。

落地价值：

- 部分 OAuth/API Key 账号可能不支持图片生成。
- 服务可以避免把图片请求发给已知不支持的账号。

建议落地方式：

1. 先只做请求类型识别和统计。
2. 再基于成功/失败记录账号能力。
3. 最后再考虑 tool 注入和用户可配置模式。

风险：

- tool 注入和协议强相关，容易受 Codex/OpenAI 上游变化影响。
- 错误判断能力会导致可用账号被误跳过。

优先级：中。

### 10. 会话可见性修复 UI

`xx` 相关文件/命令：

- `src/components/codex/CodexSessionVisibilityRepairModal.tsx`
- `codex_repair_session_visibility_across_instances`
- `codex_list_session_visibility_repair_instances`
- `codex_list_session_visibility_repair_providers`

落地价值：

- 当前 `main` 已经有切号后 provider 变化的自动修复逻辑。
- 但对于历史会话、跨实例会话，手动诊断和修复入口仍然有价值。

建议落地方式：

1. 先做只读诊断：列出实例、provider、可修复会话数量。
2. 再做带预览和确认的修复。
3. 必须结合 mtime 保护，避免修复后会话排序乱掉。

风险：

- 会话元数据变更会直接影响用户可见历史，必须可预览、可解释。

优先级：中高。

## 需要单独设计后再考虑的能力

### 11. Sidecar 网关模式

`xx` 相关能力：

- `CodexLocalAccessGatewayMode::{Legacy, Sidecar}`
- Sidecar 配置、manifest、认证文件生成。
- Sidecar 子进程启动、复用、事件回传。
- Sidecar 请求日志与健康状态回写。

落地价值：

- 将复杂的流式代理、请求调度、长连接处理从 Tauri 主进程隔离出去。
- 有机会提升 API 服务在高流量或长流式请求下的稳定性。

为什么不能直接搬：

- 这不是一个小功能，而是运行时架构变更。
- 涉及二进制打包、跨平台路径、启动/停止、崩溃恢复、日志、代理环境、发布产物。
- `xx` 同时还改动了很多代理运行时相关内容，直接抽取风险很高。

建议：

- 先不要进入 `main`。
- 单独开实验分支做 POC。
- 先验证 Windows/macOS/Linux 打包和重启恢复。

优先级：中，需设计评审。

### 12. 官方 Codex App 模型注入

`xx` 新增文件：

- `src-tauri/src/modules/codex_model_injector.rs`

能力概览：

- 根据 Codex home 派生 remote debugging port。
- 给官方 Codex App 注入模型 catalog。
- 依赖浏览器/CDP target 发现。

落地价值：

- 可能让自定义供应商/模型出现在官方 Codex UI 中。

风险：

- remote debugging 是强控制能力，安全边界敏感。
- 官方 App 内部结构变化会导致注入失效。

建议：

- 仅作为实验能力。
- 必须显式 opt-in。
- 文档中说明安全风险。

优先级：低到中。

### 13. 官方 App Server 元数据重建

`xx` 新增文件：

- `src-tauri/src/modules/codex_official_app_server.rs`

能力概览：

- 调用 `codex-app-server rebuild-thread-metadata` 重建线程元数据。

落地价值：

- 可作为会话同步或可见性修复后的补救工具。

风险：

- 依赖外部官方二进制和路径假设。
- 失败诊断需要清晰展示 stdout/stderr。

优先级：低到中。

## 不建议直接纳入的 xx 内容

以下内容虽然在 `xx` 中存在，但不应作为这次 Codex 借鉴范围直接进入 `main`：

- Claude 平台相关模块和 UI。
- 大范围代理运行时移除/替换。
- 图标、品牌、发布流程、Homebrew Cask 变更。
- 非 Codex 的账号、导入、OAuth、系统模块大重构。
- Sidecar 打包链路，除非已经完成独立设计和验证。

## 推荐落地路线

### 第一阶段：低风险基础设施

- 优先补齐模型供应商协议能力与 API 接入转换：`wireApi` / `api_wire_api`、协议推断、APIKEY.FUN 与 New API 初始值一致、供应商 UI 展示、Responses 与 Chat Completions 请求/响应/SSE 转换。
- 迁移 `codex_config_format.rs`，统一 Codex config 写入和格式化。
- 迁移 `codex_session_file_time.rs`，保护会话文件 mtime。
- 引入请求类型枚举和基础模型字段，但暂不改变路由。

### 第二阶段：API 服务可观测性

- 增加请求日志存储与查询。
- 新增 Codex API 服务独立页面的只读版本。
- 增加日志页签和测试入口。

### 第三阶段：API Key 管理

- 增加命名 API Key。
- 保持当前全局 `apiKey` 兼容。
- 支持按 Key 轮换、删除、标签展示。

### 第四阶段：模型与路由增强

- 增加账号级模型排除。
- 增加模型别名。
- 增加模型价格元数据与统计展示。

### 第五阶段：高级运行时实验

- 单独评估 Sidecar 网关。
- 单独评估图片生成 tool 注入。
- 单独评估官方 Codex App 模型注入。

## 每个功能点落地前的验证清单

- `npm run typecheck`
- `npm run build`
- 受影响 Rust crate 的 `cargo check`
- 配置读写、路由、认证、日志保留策略的后端测试。
- 手动检查：
  - 当前单一 API Key 仍可使用。
  - “切换 API”仍能把 Codex 改回 API 服务 provider。
  - 账号池、供应商池、融合模式行为不回退。
  - 请求日志不保存 prompt、图片、完整响应、API Key、OAuth token、refresh token。
  - 会话修复不会改变用户可见的会话排序。

## 总结

`xx` 对 `main` 最值得借鉴的 Codex 能力是：

1. 模型供应商协议能力与 API 接入转换，尤其是 New API / APIKEY.FUN 使用一致初始协议逻辑，并支持 `responses` 与 `chat_completions` 显式区分、请求转换、响应转换和流式转换。
2. Codex config 安全写入与格式化。
3. 会话文件 mtime 保护。
4. 独立 API 服务页面。
5. 请求日志与诊断。
6. 命名 API Key。
7. 模型路由与账号模型排除。

Sidecar 网关、图片生成 tool 注入、官方 App 模型注入、官方 App Server 元数据重建都值得关注，但它们属于较高风险能力，应放在后续实验分支或独立设计中推进。
