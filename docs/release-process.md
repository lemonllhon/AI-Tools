# AI Lemon Tools 发布教程

本仓库已经配置 GitHub Actions 自动发布流程：`.github/workflows/release.yml`。

结论先说清楚：

- 普通推送代码到 `main`：不会打包 Release。
- 推送版本标签，例如 `0.0.16` 或 `v0.0.16`：会自动打包并创建 GitHub Release。
- 也可以在 GitHub 网页的 `Actions -> Release -> Run workflow` 手动触发。
- 构建任务在源码仓库运行；完整 Release 会发布到 `PUBLIC_RELEASE_REPOSITORY` 指向的公开发布仓库。
- 源码仓库不会保存完整安装包 Release，公开仓库只会收到空提交、版本 tag、Release 资产和更新清单。
- 当前常用推送远端是 `lemonllhon`，最近的版本标签使用不带 `v` 的格式，例如 `0.0.16`。

## 1. 发布前准备

确认版本号已经改好：

```bash
npm version 0.0.16 --no-git-tag-version
npm run sync-version
```

`npm run sync-version` 会把 `package.json` 的版本同步到 Tauri 配置和 Cargo 配置。

如果你维护更新日志，还需要在这两个文件里加入同版本段落：

```text
CHANGELOG.md
CHANGELOG.zh-CN.md
```

Release workflow 会读取这两个文件生成中英文 Release Notes；如果缺少对应版本段落，workflow 会失败。

发布日志建议至少覆盖本次版本的：

- 新增功能
- 行为变更
- 修复内容
- 影响发布或使用的注意事项

例如 `0.0.16` 发布时，日志需要明确写入 mihomo 内核、内置代理网关、节点池筛选全选、节点池布局等内容。

## 2. 本地检查

推荐发版前先跑：

```bash
npm install
npm run typecheck
npm run build
git diff --check
```

如果本次改动涉及代理内核、运行时清单或 Tauri 打包资源，还要跑：

```bash
npm run proxy-runtime:prepare
npm run proxy-runtime:verify -- --bundle
```

说明：

- `proxy-runtime:prepare` 会下载并整理当前平台需要的代理内核到被 `.gitignore` 忽略的缓存目录。
- `proxy-runtime:verify -- --bundle` 校验本次准备出的 bundle 文件和清单是否匹配。
- 如果本地下载卡住，可以先用 `curl` 验证 GitHub release asset 是否可访问；不要把 `src-tauri/proxy-runtime/bin/`、`downloads/` 或 `proxy-runtime-bundle/` 提交进仓库。

如果本机安装了 Rust/Cargo，也可以跑：

```bash
npm run release:preflight
```

注意：`release:preflight` 包含多语言 key 完整性检查。如果仓库已有历史多语言 key 不一致，它可能在本地失败并生成 `locale-check-report.md`。这个报告不要随发布提交一起提交；除非本次就是修复翻译 key，否则可以记录失败原因后继续以 Release workflow 的实际结果为准。

## 3. 推送代码

先提交版本改动，然后推送到新仓库：

```bash
git add -A
git commit -m "chore: release v0.0.16"
git push lemonllhon main
```

只推 `main` 不会发布 Release，它只是更新代码。

如果推送 `main` 被拒绝：

```text
! [rejected] main -> main (fetch first)
```

说明远端 `main` 有本地没有的新提交，常见原因是上一次 Release workflow 自动合并了 Homebrew Cask PR。先查看差异：

```bash
git fetch lemonllhon main
git log --oneline --left-right --graph main...lemonllhon/main
git show --stat --oneline lemonllhon/main -1
```

如果远端只是类似 `chore(homebrew): update cask for v0.0.x` 的自动提交，并且不冲突，可以合并后再推：

```bash
git merge --no-ff lemonllhon/main -m "chore: merge remote main before v0.0.16 push"
git push lemonllhon main
```

不要用 `git reset --hard` 或强推覆盖远端自动提交，除非你明确确认要丢弃远端内容。

## 4. 创建并推送版本标签

标签必须和 `package.json` 里的版本一致。

例如 `package.json` 是 `0.0.16`，标签可以是 `0.0.16` 或 `v0.0.16`。当前仓库最近使用裸版本标签：

```bash
git tag 0.0.16
git push lemonllhon 0.0.16
```

推送这个 tag 后，GitHub Actions 会开始打包。

注意：当前 Release workflow 同时监听 `v*` 和 `[0-9]*` 标签。为了减少混乱，同一个版本只推一种格式，不要同时推 `0.0.16` 和 `v0.0.16`。

如果 tag 已经推送成功，但 `main` 因远端领先被拒绝，先不要移动 tag。按上一节把远端 `main` 合并后推上去即可。Release workflow 会按 tag 指向的发布提交构建。

## 5. 查看打包进度

打开仓库：

```text
https://github.com/lemonllhon/AI-Tools/actions
```

进入 `Release` workflow，等待所有平台构建完成。

如果本机安装了 GitHub CLI，也可以：

```bash
gh run list --repo lemonllhon/AI-Tools --limit 8
```

如果没有 `gh`，可以用 GitHub API 查看最新 run：

```powershell
$headers = @{ 'User-Agent' = 'release-check' }
$runs = Invoke-RestMethod -Headers $headers -Uri 'https://api.github.com/repos/lemonllhon/AI-Tools/actions/runs?per_page=10'
$runs.workflow_runs | Select-Object -First 5 name,display_title,head_branch,head_sha,status,conclusion,html_url
```

查看某个 Release run 的 job 状态：

```powershell
$headers = @{ 'User-Agent' = 'release-check' }
$run = Invoke-RestMethod -Headers $headers -Uri 'https://api.github.com/repos/lemonllhon/AI-Tools/actions/runs/<RUN_ID>'
$jobs = Invoke-RestMethod -Headers $headers -Uri $run.jobs_url
$jobs.jobs | Select-Object name,status,conclusion,started_at,completed_at
```

一次正常的 Release 应该看到这些 job 成功：

- `Prepare public release`
- Windows x86_64 release
- Linux x86_64 release
- Linux arm64 release
- macOS Intel release
- macOS Apple Silicon release
- macOS universal release
- `Rebuild merged latest.json`
- `Upload SHA256SUMS`
- `Publish release as latest`
- `Update Homebrew Cask`

Release 下载页面在公开发布仓库：

```text
https://github.com/<PUBLIC_RELEASE_REPOSITORY>/releases
```

当前 workflow 会构建：

- macOS Apple Silicon
- macOS Intel
- macOS Universal
- Windows x64 安装包
- Windows x64 portable 免安装包
- Linux x64
- Linux ARM64

完成后会在公开发布仓库创建 GitHub Release，并上传安装包、Windows portable 免安装 zip、`latest.json` 和 `SHA256SUMS.txt`。

`latest.json` 里的下载地址也会指向公开发布仓库，应用内自动更新不会依赖源码仓库的 Release。

### 5.1 Windows portable 免安装版

Release workflow 会在 Windows 构建成功后额外上传一个免安装压缩包：

```text
AI Lemon Tools_版本号_windows_x64_portable.zip
```

例如：

```text
AI Lemon Tools_0.0.16_windows_x64_portable.zip
```

用户下载后解压，直接运行里面的 `AI Lemon Tools.exe` 即可，不需要安装。

注意：

- portable 包不会写入 Windows 安装列表。
- 如果系统缺少 WebView2 Runtime，需要先安装 Microsoft Edge WebView2 Runtime。
- portable 包会进入 `SHA256SUMS.txt` 校验文件。
- portable 包不写入 `latest.json` 自动更新清单；自动更新仍使用 Windows NSIS/MSI 安装包链路。

## 6. 必须配置的 Secrets 和 Variables

当前发布流程需要两组配置：公开发布仓库配置、Tauri 自动更新签名配置。

### 6.1 公开发布仓库

在源码仓库配置：

```text
PUBLIC_RELEASE_REPOSITORY
PUBLIC_RELEASE_TOKEN
```

推荐配置方式：

```text
PUBLIC_RELEASE_REPOSITORY = Repository variable
PUBLIC_RELEASE_TOKEN = Repository secret
```

`PUBLIC_RELEASE_REPOSITORY` 的格式是：

```text
owner/repo
```

例如：

```text
lemon-casino/ai-lemon-tools-release
```

`PUBLIC_RELEASE_TOKEN` 是一个 GitHub token，需要对公开发布仓库有 `Contents: Read and write` 权限。workflow 会用它在公开仓库里创建空提交、强制更新同名 tag、创建/更新 Release、上传安装包、上传 `latest.json` 和 `SHA256SUMS.txt`。

公开发布仓库可以是空仓库。第一次发布时，workflow 会自动在公开仓库创建 `main` 空提交和对应版本 tag。

如果想把 `PUBLIC_RELEASE_REPOSITORY` 也放到 Secret，workflow 也兼容；但它不是敏感值，放 Repository variable 更直观。

### 6.2 Tauri 自动更新签名

因为 Tauri 配置了自动更新产物：

```json
"createUpdaterArtifacts": true
```

GitHub 仓库需要配置这两个 Secrets：

```text
TAURI_SIGNING_PRIVATE_KEY
TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

配置位置：

```text
GitHub 仓库 -> Settings -> Secrets and variables -> Actions -> New repository secret
```

`GITHUB_TOKEN` 不需要手动配置，GitHub Actions 会自动提供。

这两个 Secrets 的含义：

```text
TAURI_SIGNING_PRIVATE_KEY = 生成出来的 .key 文件完整内容
TAURI_SIGNING_PRIVATE_KEY_PASSWORD = 生成 .key 时输入的 Password
```

生成签名密钥示例：

```bash
npm run tauri signer generate -- -w ~/.tauri/ai-lemon-tools.key
```

生成时终端会输出一个 public key。这个 public key 要写进：

```text
src-tauri/tauri.conf.json
```

对应位置：

```json
"pubkey": "这里放 public key"
```

私钥和密码必须保存好。私钥丢失后，已经安装旧公钥版本的用户会无法继续通过同一更新链路验证后续更新。

发布前必须确认 `src-tauri/tauri.conf.json` 中的 `plugins.updater.pubkey` 与 GitHub Actions
中的 `TAURI_SIGNING_PRIVATE_KEY` 是同一对密钥。发布 workflow 在生成合并后的
`latest.json` 时会校验所有 `.sig` 文件的 key id 是否与配置公钥一致；不一致会直接失败，
避免发布出客户端下载到一半才因签名公钥不匹配而失败的更新。

如果需要轮换自动更新密钥，推荐做一次桥接版本：桥接版本的应用内置新公钥，但发布产物仍用旧私钥签名，
这样旧版本客户端可以验证并升级到桥接版本；桥接版本之后的发布再改用新私钥签名。若旧私钥已经丢失，
旧版本客户端只能手动安装一次内置新公钥的版本，之后自动更新才能恢复。

## 7. 注意事项与常见失败处理

### 7.1 Tag 和版本不一致

如果 Actions 报错：

```text
Tag (0.0.16) does not match package.json version (0.0.15 or v0.0.15).
```

通常原因是只执行了：

```bash
npm version 0.0.16 --no-git-tag-version
npm run sync-version
```

但没有把版本文件提交，然后就打了 tag。

记住：`npm version --no-git-tag-version` 只修改本地文件，不会自动 commit。tag 指向的是你创建 tag 那一刻的 commit。如果版本改动没有 commit，Actions 仍然会读到旧 commit 里的旧版本。

正确顺序：

```bash
npm version 0.0.16 --no-git-tag-version
npm run sync-version
git add package.json package-lock.json src-tauri/tauri.conf.json src-tauri/Cargo.toml
git commit -m "chore: release v0.0.16"
git push lemonllhon main
git tag 0.0.16
git push lemonllhon 0.0.16
```

如果错误 tag 已经推到远程，修好 commit 后更新 tag：

```bash
git tag -f 0.0.16
git push lemonllhon 0.0.16 --force
```

### 7.2 缺少 changelog 段落

如果 Actions 报错：

```text
Missing changelog section for version 0.0.16 in CHANGELOG.zh-CN.md
```

说明 workflow 没有在更新日志里找到对应版本。两个文件都必须有同版本段落：

```text
CHANGELOG.md
CHANGELOG.zh-CN.md
```

格式必须类似：

```markdown
## [0.0.16] - 2026-05-23
```

或者：

```markdown
## [v0.0.16] - 2026-05-23
```

只改一个文件不够，中英文两个文件都要加。补完后提交、推送，并把 tag 更新到新 commit：

```bash
git add CHANGELOG.md CHANGELOG.zh-CN.md
git commit -m "docs: add changelog for v0.0.16"
git push lemonllhon main
git tag -f 0.0.16
git push lemonllhon 0.0.16 --force
```

### 7.3 GitHub 权限

源码仓库只需要运行 Actions、读取源码；如果启用了 Homebrew Cask 自动 PR，源码仓库还需要允许 workflow 创建 PR。仓库建议确认：

```text
Settings -> Actions -> General -> Workflow permissions -> Read and write permissions
```

如果继续使用默认 `GITHUB_TOKEN` 创建 PR，还需要勾选：

```text
Settings -> Actions -> General -> Workflow permissions -> Allow GitHub Actions to create and approve pull requests
```

当前 workflow 已把 Homebrew Cask PR 的创建和自动合并切到 `PUBLIC_RELEASE_TOKEN`，因此 `PUBLIC_RELEASE_TOKEN` 除了公开发布仓库权限外，也需要能写入源码仓库的 Cask 分支并创建 PR。Fine-grained token 至少给源码仓库 `Contents: Read and write`、`Pull requests: Read and write`；如果使用 classic token，通常需要 `repo` 权限。

公开发布仓库的写入权限也由 `PUBLIC_RELEASE_TOKEN` 提供。如果权限不足，通常会在创建公开 tag、创建 Release、上传资产或创建 Cask PR 时报 `Resource not accessible by integration`、`Not Found`、`403` 或 `GitHub Actions is not permitted to create or approve pull requests`。

### 7.4 失败的 run 怎么处理

旧的失败 run 不需要删除。修复后重新推 `main`，再更新同一个 `v*` tag，GitHub 会触发新的 Release workflow。

如果公开发布仓库的 GitHub Release 页面已经产生了失败残留的 draft release，可以在 GitHub 网页手动删除 draft 后重新跑，或者让后续 workflow 覆盖同名资产。

### 7.5 Windows portable 找不到 release 目录

如果 portable 步骤报错类似：

```text
Release directory not found: ...\src-tauri\target\release
```

原因通常是 GitHub Windows runner 在 Rust workspace 下把 Cargo 产物放到了仓库根目录的 `target/release`，而不是 `src-tauri/target/release`。当前 workflow 已改为自动搜索：

```text
target
src-tauri/target
```

如果后续仍然报 `No Windows executable found in Cargo target directories`，说明 Windows 构建阶段没有生成主程序 exe，需要先查看同一个 run 里 Windows 的 `Build the app` 日志。

### 7.6 公开发布仓库为空导致 main 分支不存在

如果 `Prepare public release` 报错类似：

```text
fatal: couldn't find remote ref refs/heads/main
```

说明公开发布仓库还是空仓库，没有任何分支。当前 workflow 已改为手动初始化公开发布仓库：如果公开仓库没有默认分支，会自动创建 `main` 的第一条空提交，再创建版本 tag 和 Release。

### 7.7 Homebrew Cask 下载 universal DMG 404

如果 `Update Homebrew Cask` 报错类似：

```text
curl: (22) The requested URL returned error: 404
Failed to download release asset after retries.
```

通常是 Tauri 上传的 DMG 文件名和 workflow 里硬编码的文件名不一致。例如实际 asset 可能是：

```text
AI.Lemon.Tools_0.0.3_universal.dmg
```

而不是：

```text
AI Lemon Tools_0.0.3_universal.dmg
```

当前 workflow 已改为从公开 Release assets 里按 `*_universal.dmg` 自动查找并下载，同时把 Cask 的 `url` 和 `verified` 更新为实际公开发布仓库地址。

## 8. 失败后怎么重发

如果 workflow 失败，先修代码并重新提交：

```bash
git add -A
git commit -m "fix: release build"
git push lemonllhon main
```

如果同一个 tag 已经推过，需要删除本地和远程旧 tag 后重推：

```bash
git tag -d 0.0.16
git push lemonllhon :refs/tags/0.0.16
git tag 0.0.16
git push lemonllhon 0.0.16
```

只在你确认要覆盖这次发布时这么做。

## 9. 最短发布命令

日常发布可以记这几行：

```bash
npm version 0.0.16 --no-git-tag-version
npm run sync-version
npm run build
npm run proxy-runtime:prepare
npm run proxy-runtime:verify -- --bundle
git add -A
git commit -m "chore: release v0.0.16"
git push lemonllhon main
git tag 0.0.16
git push lemonllhon 0.0.16
```

如果 `git push lemonllhon main` 提示 `fetch first`，插入这几行后再推：

```bash
git fetch lemonllhon main
git log --oneline --left-right --graph main...lemonllhon/main
git merge --no-ff lemonllhon/main -m "chore: merge remote main before v0.0.16 push"
git push lemonllhon main
```
