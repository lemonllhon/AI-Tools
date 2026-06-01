# Changelog

English · [简体中文](CHANGELOG.zh-CN.md)

All notable changes to Cockpit Tools will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [0.0.29] - 2026-06-01
### Added
- Added a New API provider preset for Codex API key accounts, with local New API defaults and provider display names.
- Added Codex New API quota retrieval through New API's token usage endpoint.
- Added `gpt-5.5` to the Codex local API service model list and display-name mapping.

### Changed
- Changed Codex API key account metadata so New API accounts keep their dedicated provider, plan badge, default account name, and imported account identity.
- Changed Codex provider config generation so New API writes a dedicated custom model provider while API-key runtime routing keeps using the fixed local access provider.

## [0.0.28] - 2026-05-27
### Added
- Added Codex auto account switching controls in Codex settings, including an enable switch, account scope, and a default 10% 5-hour quota threshold.

### Changed
- Changed Codex auto switching to use the 5-hour quota window as the trigger and candidate ranking signal, avoiding weekly quota from blocking seamless switching.
- Changed the Codex local API service to silently skip accounts whose 5-hour quota is at or below the auto-switch threshold and continue routing to healthier accounts.

## [0.0.27] - 2026-05-27
### Fixed
- Improved Codex OAuth token exchange failures with clearer status, response length, and provider error-code details.
- Added a Codex OAuth retry after a 403 response by promoting the next built-in proxy pool outlet, helping recover when the current outlet is rejected by the authorization service.

## [0.0.26] - 2026-05-27
### New addition
- Added a one-click Codex speed control in the dashboard API Services console, allowing all Codex accounts to be set to Standard or Fast from one place.
- Added the same all-account Codex speed control to the Codex API Service panel, and synchronized the API service default speed when applying it.

## [0.0.25] - 2026-05-24
### New addition
- Multilingual support.

## [0.0.24] - 2026-05-23
### Added
- Added managed restart hooks for the built-in proxy gateway so app updates and relaunches close proxy gateway listeners, bridge processes, and Codex API local access before restarting.
- Added startup cleanup for stale proxy bridge work directories, PID files, and leftover bridge listener ports from previous sessions.
- Added a source multi-select directly inside the proxy node list header, letting the node list filter by one or more subscription sources.

### Changed
- Changed Windows proxy bridge cleanup to terminate the full process tree with `taskkill /F /T`, reducing leftover mihomo/sing-box/xray child processes and stuck ports.
- Changed the proxy node list source filter behavior so search, group, and protocol filters reset source scope back to all sources, keeping filtered results predictable.
- Changed the proxy node list header controls so the source filter, selected-node scope, and collapse button share the same compact height.

### Fixed
- Fixed built-in proxy gateway and bridge ports sometimes staying occupied after app updates, relaunch attempts, or gateway restarts.
- Fixed preview latency tests and IP health checks in Add Resource appearing stuck on large subscriptions by adding bounded bridge-node concurrency and per-node timeout fallbacks.
- Fixed the old Display Scope subscription selector duplicating the new node-list source filter.
- Fixed source filtering being limited to a single subscription source when users need to combine nodes from multiple subscriptions.

## [0.0.23] - 2026-05-23
### Fixed
- Fixed release builds failing in `proxy_pool::gateway` by adding the explicit outbound lifetime required when returning the successful gateway fallback target.
- Fixed HTTPS upstream proxy TLS configuration for `rustls-native-certs` 0.8 by reading the new certificate result shape and treating partial native certificate load errors as warnings.

## [0.0.22] - 2026-05-23
### Added
- Added HTTPS upstream proxy support in the built-in proxy gateway, including TLS connection setup to the upstream proxy, system root certificate validation, CONNECT tunneling, absolute-form HTTP forwarding, and upstream proxy authentication reuse.
- Added realtime backend progress events for batch proxy latency tests and batch IP health checks, with `started`, per-node `node_done`, and `finished` phases keyed by task ID.
- Added import-preview latency tests and IP health checks for selected preview nodes before importing paste resources or URL subscriptions.
- Added proxy gateway failover events so the Network Services UI can update the current node-pool outlet when a backup node becomes active.

### Changed
- Changed proxy runtime resolution to prefer the packaged application `proxy-runtime` directory before falling back to cached runtime binaries, while keeping Windows, macOS, and Linux paths supported.
- Changed the built-in proxy runtime settings panel to be collapsible and added direct access to the packaged runtime resource directory.
- Changed batch latency and IP health checks to persist each completed node result immediately and update the UI progressively instead of waiting for the full batch to finish.
- Changed node-pool gateway routing so a successful backup node is persisted as `current_node_id` when the previous current node fails, with cooldown protection against concurrent request churn.
- Updated the proxy network service implementation plan with staged completion status for HTTPS upstream proxying, realtime progress, preview diagnostics, and failover persistence.

### Fixed
- Fixed HTTPS proxy nodes being unusable as built-in gateway upstreams because the gateway only supported plaintext HTTP proxy forwarding.
- Fixed batch proxy diagnostics feeling frozen during long checks by streaming individual node progress to the frontend.
- Fixed imported preview nodes requiring full import before they could be temporarily latency-tested or IP-health checked.
- Fixed node-pool failover only working for the current request without promoting the working backup node for later gateway requests.
- Fixed open settings state not reflecting automatic gateway failover until the node list was manually reloaded.

## [0.0.21] - 2026-05-23
### Changed
- Changed Codex API local access port cleanup to wait for gateway port release and treat already-exited owner PIDs as non-fatal after rechecking the listener state.
- Changed port cleanup results and UI notices to show when the API service switches from the previous port to a new available port.

### Fixed
- Fixed Windows port cleanup failing when `taskkill` reported that a PID found by `netstat` no longer existed; the cleanup now rechecks whether the process and port are still alive before deciding failure.
- Fixed macOS port cleanup treating `lsof`-discovered PIDs that exit during cleanup as hard failures, while still waiting for the port to release and changing ports when needed.
- Fixed Linux port owner detection relying only on `lsof`; Linux now falls back to `ss -ltnp` and `netstat -ltnp` so minimal environments can still clear or recover from occupied API service ports.

## [0.0.20] - 2026-05-23
### Changed
- Changed the proxy node import preview action label from "Preview import" to "Preview", keeping "Import selected" as the only action that actually writes nodes.
- Changed proxy resource import previews to start with no nodes selected, so users explicitly choose which nodes to import.
- Changed proxy resource, URL subscription, subscription edit, and manual node forms to use left-aligned labels and inputs for a more consistent settings layout.

### Fixed
- Fixed the proxy node list showing duplicate row checkboxes by removing the separate batch-delete checkbox and reusing the leading node selection checkbox for selected-node batch actions.
- Fixed imported proxy nodes being implicitly selected during the preview flow before the user had chosen them.
- Fixed the IP health detail dialog opening relative to the page instead of the current viewport by rendering the dialog at the document body level.
- Fixed add-resource paste content, group, name prefix, URL subscription, and manual node fields appearing visually misaligned.

## [0.0.19] - 2026-05-23
### Changed
- Changed proxy latency tests to try multiple `generate_204` endpoints in order, reducing false failures when a single probe host is blocked or returns a transient gateway error.
- Changed mihomo bridge startup to run from an isolated per-node work directory and disable profile cache persistence, reducing cache-file contention during repeated or batch checks.
- Changed WebSocket bridge option mapping so share-link `host` is treated as the WS `Host` header instead of being reused as the TLS `servername`.

### Fixed
- Fixed batch latency checks failing too easily when `cp.cloudflare.com/generate_204` could not complete through an otherwise usable node.
- Fixed selected VLESS/WS/TLS nodes producing mihomo `unexpected status: 502 Bad Gateway` more often because WS Host and TLS server name were not separated cleanly.
- Fixed built-in proxy gateway restarts reporting the gateway port as occupied while a previous listener task was still shutting down.
- Fixed Codex API local access port updates starting the old port before switching, which could leave the service reporting `127.0.0.1:<port> already in use`.

## [0.0.18] - 2026-05-23
### Changed
- Changed temporary mihomo bridge readiness so latency tests and IP health checks wait briefly after the local mixed/SOCKS endpoint is reachable, giving mihomo compatible providers time to finish initialization.
- Changed the latency probe endpoint from Google `generate_204` to Cloudflare `generate_204` to reduce regional reachability noise.
- Improved latency and IP health failure messages by including the reqwest source cause chain alongside the bridge log snippet.

### Fixed
- Fixed a race where selected subscription nodes could work through the built-in gateway, but immediate latency tests or IP health checks failed because mihomo had opened the port before its proxy provider was ready.
- Fixed advanced node diagnostics hiding lower-level request causes such as connection aborts, resets, or tunnel setup failures.

## [0.0.17] - 2026-05-23
### Added
- Added detailed proxy bridge diagnostics for gateway forwarding, latency tests, and IP health checks, including mihomo log snippets when node dialing fails.
- Added bridge startup and readiness logs with runtime, node, local SOCKS endpoint, binary path, and generated config path for easier release-build troubleshooting.
- Added regression coverage for mihomo bridge config generation so selected advanced nodes route through the expected proxy group.

### Changed
- Changed mihomo bridge config generation to use a standard `proxies` plus `proxy-groups` layout, with `MATCH` routing through the generated group instead of relying on a bare proxy name.
- Changed advanced-node TLS handling to read more subscription-compatible SNI fields, including `sni`, `servername`, `serverName`, `peer`, query `host`, and Clash YAML options.
- Changed advanced-node insecure certificate handling to honor share-link query flags such as `allowInsecure`, `insecure`, and `skip-cert-verify`.

### Fixed
- Fixed selected VLESS/WS/TLS subscription nodes failing through the built-in proxy gateway because generated mihomo routing config did not define an explicit proxy group.
- Fixed latency tests and IP health checks hiding the real mihomo failure reason behind generic request errors.
- Fixed gateway failures showing only opaque CONNECT/tunnel errors instead of surfacing actionable node errors such as WebSocket `502 Bad Gateway`.

## [0.0.16] - 2026-05-23
### Added
- Added bundled mihomo runtime support for Windows, macOS Intel, macOS Apple Silicon, Linux x86_64, and Linux arm64 proxy bridge packaging.
- Added mihomo-based bridge generation as the primary path for advanced proxy protocols including `vmess`, `vless`, `trojan`, `ss`, `hysteria`, `hysteria2`, `tuic`, and `anytls`.
- Added current-filter select-all support in the proxy node list so users can quickly select or unselect all visible subscription/search/group/protocol matches.

### Changed
- Changed the built-in proxy gateway bridge path to prefer mihomo for selected node-pool outlets, keeping xray and sing-box as compatibility fallbacks for future use.
- Changed latency tests, IP health checks, and real gateway forwarding so advanced subscription nodes can be bridged through mihomo instead of stopping at unsupported protocol errors.
- Changed the proxy node pool header so the selected-node view button sits next to the node count, before the collapse/expand control.
- Changed proxy node pool layout alignment to be left-aligned across source metadata, list headers, node status, and compact node rows.

### Fixed
- Fixed selected advanced nodes failing to serve Codex API traffic through the built-in gateway because the bridge path was not using the subscription-compatible mihomo core.
- Fixed proxy node pool controls appearing visually right-aligned in the Network Services settings area.
- Fixed large filtered node sets requiring manual one-by-one selection by allowing the visible filtered node set to be selected in one action.

## [0.0.15] - 2026-05-22
### Added
- Added temporary xray/sing-box bridge startup for latency tests and IP health checks, allowing advanced nodes such as `vmess`, `vless`, `trojan`, `ss`, `hysteria`, `hysteria2`, `tuic`, and `anytls` to be checked through the bundled proxy cores.
- Added full IP health result persistence and frontend typing so node rows can open detailed health information instead of showing only a short summary.
- Added an eye-button IP health detail dialog with IP, location, fraud score, residential/broadcast flags, ASN organization, source, update time, error text, and raw response data.
- Added a node list display scope control that can switch between all nodes and individual subscriptions when multiple subscription sources exist.
- Added an "Selected" node list view that shows selected node-pool outlets across all subscriptions.

### Changed
- Changed advanced protocol latency and IP health checks to create a temporary bridge automatically, keeping the bridge alive for the duration of the request and cleaning it up afterward.
- Changed batch checks so regular nodes can still run concurrently while bridge-required advanced nodes are processed safely through temporary bridge sessions.
- Changed node list filtering so search, group, and protocol filters reset the list scope back to all nodes, while switching a subscription clears search/group/protocol filters.
- Changed the proxy node pool panel to remove duplicated gateway address, gateway port, and external local proxy port controls; those settings now stay only in the main built-in proxy gateway area.
- Changed the node list height to be calculated from the first 10 rendered node rows instead of using a fixed hard-coded height.
- Changed node list wheel behavior so scrolling past the top or bottom continues scrolling the surrounding settings page.

### Fixed
- Fixed legacy advanced-node messages such as "vless node needs built-in bridge first" by routing those checks through the actual temporary bridge path.
- Fixed advanced-node IP health display so stale bridge-pending summaries are normalized away when listing nodes.
- Fixed xray bridge config generation so Clash YAML `options.type` is not incorrectly treated as the xray transport network.
- Fixed large subscription node pools requiring the mouse to leave the node list before the rest of the settings page could scroll.

## [0.0.14] - 2026-05-22
### Added
- Added a real built-in proxy gateway service that listens on `127.0.0.1` and provides HTTP and CONNECT forwarding for the app.
- Added built-in gateway outbound routing for Direct, Local Proxy, and Node Pool modes.
- Added HTTP upstream forwarding so the built-in gateway can reuse an external local proxy such as `127.0.0.1:7890`.
- Added SOCKS5 upstream forwarding, including optional username/password authentication.
- Added gateway startup restore on app launch so the saved built-in proxy state is applied automatically.
- Added xray bridge startup for advanced node protocols including `vmess`, `vless`, `trojan`, and `ss`.
- Added sing-box bridge startup for advanced node protocols including `hysteria`, `hysteria2`, `tuic`, and `anytls`.
- Added per-node bridge config generation under the proxy pool data directory, with local SOCKS bridge endpoints exposed back to the gateway.
- Added runtime binary resolution for cached xray/sing-box executables so bridge startup can use prepared platform runtimes.
- Added proxy pool outlet mode persistence with `outlet_mode`, `current_node_id`, and `selected_node_ids_json`.
- Added multi-select node pool outlets so users can choose more than one proxy node while keeping one current active node.
- Added latency testing for a single node and for all enabled/selected proxy nodes.
- Added IP health checks for a single node and for all enabled/selected proxy nodes, with persisted health summaries.
- Added frontend service and type support for gateway state, outlet modes, selected node IDs, latency results, and IP health results.
- Added Network Services controls for testing latency, checking IP health, refreshing node state, and managing selected node pool outlets.

### Changed
- Changed the Network Services global proxy switch to mean enabling the built-in proxy gateway, using the configured gateway port.
- Changed Codex API "follow global proxy" behavior to use the same built-in gateway address when the gateway is enabled.
- Changed proxy selection to a mutually exclusive Direct / Local Proxy / Node Pool model.
- Changed node pool selection so imported nodes are not automatically used as active outlets.
- Changed node pool mode so only user-selected normal nodes are enabled for outbound use.
- Changed Direct mode so only the built-in Direct outlet remains enabled.
- Changed Local Proxy mode so only the built-in Local Proxy outlet remains enabled.
- Changed local proxy configuration to allow editing the default local proxy port instead of hard-coding `7890`.
- Changed proxy pool list layout to support collapsing and scroll limiting for large node pools.
- Changed proxy node row density so Direct, Local Proxy, and up to 10 selected/visible nodes fit in a compact management area.
- Updated Settings and Network Services summaries to show gateway URL, outlet mode, current node, and selected node count.
- Updated proxy pool localization strings for the new gateway, outlet, health, selection, and layout controls.
- Updated the proxy network service implementation plan with the implemented gateway, bridge, selection, and health-check behavior.

### Fixed
- Fixed outlet synchronization so choosing Direct, Local Proxy, or Node Pool disables the other outlet types.
- Fixed node pool selection behavior so selecting normal nodes disables Direct and Local Proxy outlets.
- Fixed Direct and Local Proxy selection behavior so normal proxy nodes are cleared from active outlet selection.
- Fixed gateway state synchronization when saving Network Services settings or updating proxy pool service state.
- Fixed gateway fallback behavior so unavailable or unsupported outlet targets fall back to Direct instead of leaving stale state active.
- Fixed large node pools expanding the Network Services page height excessively.

## [0.0.13] - 2026-05-22
### Fixed
- Fixed macOS universal release builds by forcing proxy subscription source query rows to be collected before their SQLite statements are dropped.

## [0.0.12] - 2026-05-22
### Added
- Added URL subscription import for the built-in proxy node pool, including `http`/`https` fetch validation, a 2 MB content limit, preview, and selective import.
- Added stable hashed subscription `source_id` tracking so repeated imports and refreshes are scoped to the same subscription source.
- Added a subscription source list in Network Services with source URL, group, node count, last refresh time, and last error visibility.
- Added refresh actions for a single subscription source and all subscription sources.

### Changed
- Extended proxy pool frontend types, services, and Tauri commands to return subscription sources and refresh results alongside nodes.
- Updated proxy protocol filtering so imported subscription protocols can appear in the filter list.

### Fixed
- Preserved existing subscription nodes when a refresh fetch or parse fails, recording `last_error` instead of deleting the old source nodes.
- Replaced subscription nodes atomically only after a successful fetch and parse, scoped by the matching `source_id`.

## [0.0.11] - 2026-05-22
### Added
- Added the first built-in proxy node pool database with SQLite persistence, schema migrations, built-in direct/local nodes, basic CRUD commands, enable/disable support, search/filter metadata, and credential-masked node output.
- Added the Network Services proxy node pool panel for manually adding `http`, `https`, and `socks5` nodes, searching by name/address/group, filtering by protocol/group, and deleting single or selected nodes.
- Added resource import preview and selective import for Clash YAML `proxies`, Base64 subscription text, and share links including `http`, `https`, `socks5`, `vmess`, `vless`, `trojan`, `ss`, `hysteria`, `hysteria2`, `tuic`, and `anytls`.
- Added structured YAML parsing for proxy resource imports.

### Changed
- Improved the built-in proxy runtime settings layout so runtime status, cache paths, and action buttons use a full-width responsive panel.
- Kept imported proxy credentials inside the backend/database import flow and returned only masked preview data to the frontend.

### Fixed
- Fixed proxy pool database initialization for `rusqlite 0.32` by using the correct `pragma_update` database name type, restoring Linux, macOS, and Windows release builds.
- Kept the workspace `Cargo.lock` update from `0.0.10` so CI release builds can resolve the new YAML parser dependency in locked mode.

## [0.0.10] - 2026-05-22
### Added
- Added the first built-in proxy node pool database with SQLite persistence, schema migrations, built-in direct/local nodes, basic CRUD commands, enable/disable support, search/filter metadata, and credential-masked node output.
- Added the Network Services proxy node pool panel for manually adding `http`, `https`, and `socks5` nodes, searching by name/address/group, filtering by protocol/group, and deleting single or selected nodes.
- Added resource import preview and selective import for Clash YAML `proxies`, Base64 subscription text, and share links including `http`, `https`, `socks5`, `vmess`, `vless`, `trojan`, `ss`, `hysteria`, `hysteria2`, `tuic`, and `anytls`.
- Added structured YAML parsing for proxy resource imports.

### Changed
- Improved the built-in proxy runtime settings layout so runtime status, cache paths, and action buttons use a full-width responsive panel.
- Kept imported proxy credentials inside the backend/database import flow and returned only masked preview data to the frontend.

### Fixed
- Updated the workspace `Cargo.lock` for the new proxy resource YAML parser so CI release builds can run with locked Cargo dependencies.

## [0.0.9] - 2026-05-22
### Added
- Added the first built-in proxy node pool database with SQLite persistence, schema migrations, built-in direct/local nodes, basic CRUD commands, enable/disable support, search/filter metadata, and credential-masked node output.
- Added the Network Services proxy node pool panel for manually adding `http`, `https`, and `socks5` nodes, searching by name/address/group, filtering by protocol/group, and deleting single or selected nodes.
- Added resource import preview and selective import for Clash YAML `proxies`, Base64 subscription text, and share links including `http`, `https`, `socks5`, `vmess`, `vless`, `trojan`, `ss`, `hysteria`, `hysteria2`, `tuic`, and `anytls`.
- Added structured YAML parsing for proxy resource imports.

### Changed
- Improved the built-in proxy runtime settings layout so runtime status, cache paths, and action buttons use a full-width responsive panel.
- Kept imported proxy credentials inside the backend/database import flow and returned only masked preview data to the frontend.

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
