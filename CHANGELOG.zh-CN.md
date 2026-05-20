# 更新日志

简体中文 · [English](CHANGELOG.md)

本文件记录 Cockpit Tools 的所有重要变更。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)。

---
## [0.0.2] - 2026-05-21

### 新增 
- **仪表盘新增统一 API 服务控制台
- **新增主题：奶油色、星空色、海洋色
- **新增 ChatGPT Session Converter 模式


---
## [0.0.1] - 2026-05-21

### 新增
- **Codex 本地 API 服务现可选择上游代理模式**：API 服务设置可在跟随应用全局代理与直连官方上游之间切换，并将所选模式持久化用于网关请求。
- **Codex OAuth 授权现内置 2FA 快速取码入口**：添加账号弹框可展示已保存 MFA 密钥、刷新倒计时与一键复制验证码；重新授权时会显示并可复制目标账号邮箱。
- **Codex 本地 API 服务现支持自定义账号调度**：API 服务集合可选择“自定义”策略，为每个账号设置优先级与权重，批量调整已选账号，并把规范化后的调度规则写入网关选号逻辑。
- **Codex Token 导入现支持 ChatGPT/Codex session JSON**：可导入直接粘贴或包裹在 `session`/`session_json` 字段中的 session JSON，并复用现有 Codex OAuth 凭据导入流程。

### 变更
- **Codex 本地 API 服务上游连接失败现提供更可操作的网络/代理诊断**：网关会记录 502 失败状态，并把网络、代理或 `chatgpt.com` 可访问性问题提示成更清晰的错误信息。
### 新增
- **更新检查**: 实现通过 GitHub Releases API 自动检查更新。
  - 发现新版本时，右上角弹出精美的毛玻璃风格通知卡片。
  - 在 **设置 → 关于** 页面新增手动"检查更新"按钮，支持实时状态反馈。
  - 点击通知或下载按钮可直接跳转到 GitHub Release 页面下载。
- **国际化**: 为全部 17 种支持语言添加了更新通知相关翻译。

---

### 技术栈
- 基于 Tauri 2.0 + React + TypeScript 构建。
- 使用 SQLite 数据库进行本地数据持久化。
- 使用系统钥匙串安全存储凭证。
- 跨平台支持（当前以 macOS 为主，Windows/Linux 计划中）。
