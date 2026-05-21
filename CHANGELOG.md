# Changelog

English · [简体中文](CHANGELOG.zh-CN.md)

All notable changes to Cockpit Tools will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).


## [0.0.4] - 2026-05-22
###New addition
-* * Added/v1/responses WebSocket handshake, authentication, responsive.create forwarding, SSE to WS event forwarding
-Optimize layout logic structure, dashboard service console display Codex WS address and status
-Add multi account functionality
-Add WebSocket entry, status, and copy buttons to the service panel functionality
-Optimize the button status in the dashboard service console


## [0.0.3] - 2026-05-22
### Added
- **Optimize the Codex API service for rapid startup**
- **Optimize the logical structure of layout**
- ** Add multi-account functionality**
- **Enhance service panel features Refresh quotas Subscribe Model provider functions Wake up tasks**
- **Enhance session management with multiple concurrent instances**
## [0.0.2] - 2026-05-21

### Added
- **Dashboard adds a unified API service console**
- **New themes added: Cream, Starry Sky, Ocean**
- **Added ChatGPT Session Converter Mode**
---
## [0.0.1] - 2026-05-21

### Added
- **Codex Local API Service Now Supports Upstream Proxy Mode**: The API service settings can switch between following the application's global proxy and directly connecting to the official upstream, with the selected mode persistently applied to gateway requests.
- **Codex OAuth now includes a built-in 2FA quick code entry interface**: The account addition pop-up can display saved MFA keys, refresh countdown timers, and one-click copy verification codes; when reauthorizing, the target account's email will be shown and copyable.
- **Codex Local API Service Now Supports Custom Account Scheduling**: The API service collection allows the selection of a "Custom" strategy, enabling the setting of priorities and weights for each account, batch adjustment of selected accounts, and writing standardized scheduling rules into the gateway selection logic.
- **Codex Token Import Now Supports ChatGPT/Codex Session JSON**: You can import session JSON by pasting it directly or wrapping it in the `session`/`session_json` field, and reuse the existing Codex OAuth credential import process.
### Change
- **Codex Local API Service Upstream Connection Failure Now Provides More Actionable Network/Proxy Diagnostics**: The gateway logs 502 failure statuses and presents network, proxy, or `chatgpt.com` accessibility issues as clearer error messages.
### New Addition
- **Update Check**: Implemented automatic update checks via the GitHub Releases API.
  When a new version is detected, a beautifully designed frosted glass-style notification card pops up in the upper right corner.
  - Add a manual "Check for Updates" button on the **Settings → About** page, with real-time status feedback.
  Click the notification or download button to directly jump to the GitHub Release page for downloading.

---

### Technical
- Built with Tauri 2.0 + React + TypeScript.
- SQLite database for local data persistence.
- Secure credential storage using system keychain.
- Cross-platform support (macOS primary, Windows/Linux planned).
