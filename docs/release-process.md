# AI Lemon Tools 发布教程

本仓库已经配置 GitHub Actions 自动发布流程：`.github/workflows/release.yml`。

结论先说清楚：

- 普通推送代码到 `main`：不会打包 Release。
- 推送版本标签，例如 `v0.24.0`：会自动打包并创建 GitHub Release。
- 也可以在 GitHub 网页的 `Actions -> Release -> Run workflow` 手动触发。

## 1. 发布前准备

确认版本号已经改好：

```bash
npm version 0.24.1 --no-git-tag-version
npm run sync-version
```

`npm run sync-version` 会把 `package.json` 的版本同步到 Tauri 配置。

如果你维护更新日志，还需要在这两个文件里加入同版本段落：

```text
CHANGELOG.md
CHANGELOG.zh-CN.md
```

Release workflow 会读取这两个文件生成中英文 Release Notes；如果缺少对应版本段落，workflow 会失败。

## 2. 本地检查

推荐发版前先跑：

```bash
npm install
npm run typecheck
npm run build
```

如果本机安装了 Rust/Cargo，也可以跑：

```bash
npm run release:preflight
```

## 3. 推送代码

先提交版本改动，然后推送到新仓库：

```bash
git add -A
git commit -m "chore: release v0.24.1"
git push lemon main
```

只推 `main` 不会发布 Release，它只是更新代码。

## 4. 创建并推送版本标签

标签必须和 `package.json` 里的版本一致。

例如 `package.json` 是 `0.24.1`，标签就必须是 `v0.24.1`：

```bash
git tag v0.24.1
git push lemon v0.24.1
```

推送这个 tag 后，GitHub Actions 会开始打包。

## 5. 查看打包进度

打开仓库：

```text
https://github.com/lemon-casino/Ai-Lemon-Tools/actions
```

进入 `Release` workflow，等待所有平台构建完成。

当前 workflow 会构建：

- macOS Apple Silicon
- macOS Intel
- macOS Universal
- Windows x64
- Linux x64
- Linux ARM64

完成后会创建 GitHub Release，并上传安装包、`latest.json` 和 `SHA256SUMS.txt`。

## 6. 必须配置的 Secrets

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

## 7. 失败后怎么重发

如果 workflow 失败，先修代码并重新提交：

```bash
git add -A
git commit -m "fix: release build"
git push lemon main
```

如果同一个 tag 已经推过，需要删除本地和远程旧 tag 后重推：

```bash
git tag -d v0.24.1
git push lemon :refs/tags/v0.24.1
git tag v0.24.1
git push lemon v0.24.1
```

只在你确认要覆盖这次发布时这么做。

## 8. 最短发布命令

日常发布可以记这几行：

```bash
npm version 0.24.1 --no-git-tag-version
npm run sync-version
git add -A
git commit -m "chore: release v0.24.1"
git push lemon main
git tag v0.24.1
git push lemon v0.24.1
```
