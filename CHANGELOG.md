# Changelog

English · [简体中文](CHANGELOG.zh-CN.md)

All notable changes to Cockpit Tools will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [0.0.8] - 2026-05-22
### Fixed
- Fixed automatic updater signature verification by aligning the app updater public key with the release signing key.
- Fixed update failure messaging so non-retryable signature errors are no longer shown as network retry failures.
- Fixed the manual release download fallback URL for numeric release tags such as `0.0.8`.

### Changed
- Added release-time validation that rejects `latest.json` generation when updater signatures are created with a different key than the configured app public key.

## [0.0.7] - 2026-05-22
### Added
- Added built-in proxy runtime status commands for xray and sing-box, including cache preparation, version detection, and cache directory opening from the desktop app.
- Added a Network Services settings status panel that shows the current proxy runtime target, resource directory, cache directory, runtime availability, detected version, and clear error messages.

### Changed
- Extended proxy runtime verification so the app can confirm cached runtime executables with `version` commands before later proxy node and bridge features are enabled.

## [0.0.6] - 2026-05-22
### Added
- Added the first built-in proxy runtime foundation for xray and sing-box, with pinned multi-platform manifests for Windows, macOS Intel, macOS Apple Silicon, Linux x64, and Linux arm64.
- Added proxy runtime prepare and verify scripts that download, extract, checksum, and package only the runtime binaries required by the current build target.
- Added the Rust-side runtime resolver and cache layer, including manifest validation, sha256 verification, environment override paths, and data-directory runtime cache preparation.

### Changed
- Updated build and release workflows so packaged releases can include the correct proxy runtime targets, including macOS universal builds.
- Updated the release workflow to accept both `0.0.6` and `v0.0.6` style tags.

## [0.0.5] - 2026-05-22
### New Addition
- **Added codex auto-join feature**

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
