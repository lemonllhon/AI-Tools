use crate::modules::data_dir;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager, Runtime as TauriRuntime};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const RESOURCE_RUNTIME_DIR_NAME: &str = "proxy-runtime";
const DEV_RUNTIME_BUNDLE_DIR_NAME: &str = "proxy-runtime-bundle";
const RUNTIME_MANIFEST_FILE_NAME: &str = "runtime-manifest.json";
const CACHE_ROOT_DIR_NAME: &str = "proxy-runtime";
const CACHE_DIR_NAME: &str = "cache";
const XRAY_RUNTIME: &str = "xray";
const SING_BOX_RUNTIME: &str = "sing-box";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRuntimeCacheState {
    pub target: String,
    pub resource_dir: String,
    pub cache_root: String,
    pub runtimes: Vec<ProxyRuntimeCachedBinary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRuntimeCachedBinary {
    pub runtime: String,
    pub expected_version: String,
    pub manifest_sha256: String,
    pub source_kind: ProxyRuntimeSourceKind,
    pub source_path: String,
    pub cache_path: String,
    pub cache_refreshed: bool,
    pub executable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRuntimeStatus {
    pub target: String,
    pub resource_dir: String,
    pub cache_root: String,
    pub runtimes: Vec<ProxyRuntimeStatusItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRuntimeStatusItem {
    pub runtime: String,
    pub expected_version: String,
    pub manifest_sha256: String,
    pub source_kind: Option<ProxyRuntimeSourceKind>,
    pub source_path: String,
    pub cache_path: String,
    pub available: bool,
    pub executable: bool,
    pub cache_refreshed: bool,
    pub detected_version: String,
    pub version_output: String,
    pub error: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProxyRuntimeSourceKind {
    Bundled,
    Override,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeManifest {
    schema_version: u32,
    files: Vec<RuntimeManifestEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct RuntimeManifestEntry {
    runtime: String,
    version: String,
    target: String,
    path: String,
    sha256: String,
}

pub fn current_target() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok("windows-x86_64"),
        ("macos", "x86_64") => Ok("darwin-x86_64"),
        ("macos", "aarch64") => Ok("darwin-aarch64"),
        ("linux", "x86_64") => Ok("linux-x86_64"),
        ("linux", "aarch64") => Ok("linux-aarch64"),
        (os, arch) => Err(format!("当前平台暂不支持代理内核: {}/{}", os, arch)),
    }
}

pub fn ensure_runtimes_cached<R: TauriRuntime>(
    app: &AppHandle<R>,
) -> Result<ProxyRuntimeCacheState, String> {
    let target = current_target()?;
    let resource_dir = resolve_resource_runtime_dir(app)?;
    let data_dir = data_dir::get_data_dir()?;
    ensure_runtimes_cached_from_dirs(&resource_dir, &data_dir, target)
}

pub fn get_runtime_status<R: TauriRuntime>(app: &AppHandle<R>) -> Result<ProxyRuntimeStatus, String> {
    let target = current_target()?;
    let resource_dir = resolve_resource_runtime_dir(app)?;
    let data_dir = data_dir::get_data_dir()?;
    get_runtime_status_from_dirs(&resource_dir, &data_dir, target)
}

pub fn ensure_runtime_binary<R: TauriRuntime>(
    app: &AppHandle<R>,
    runtime_name: &str,
) -> Result<PathBuf, String> {
    validate_runtime_name(runtime_name)?;
    let state = ensure_runtimes_cached(app)?;
    let cached = state
        .runtimes
        .into_iter()
        .find(|item| item.runtime == runtime_name)
        .ok_or_else(|| format!("代理内核缓存缺少 {}", runtime_name))?;
    if !cached.executable {
        return Err(format!(
            "代理内核 {} 不可执行: {}",
            runtime_name, cached.cache_path
        ));
    }
    Ok(PathBuf::from(cached.cache_path))
}

pub fn get_runtime_status_from_dirs(
    resource_dir: &Path,
    data_dir: &Path,
    target: &str,
) -> Result<ProxyRuntimeStatus, String> {
    validate_target(target)?;
    let manifest = read_manifest(resource_dir)?;
    let entries = select_target_entries(&manifest, target)?;
    let cache_root = cache_root_for_data_dir(data_dir, target);
    fs::create_dir_all(&cache_root).map_err(|err| {
        format!(
            "创建代理内核运行缓存目录失败 {}: {}",
            cache_root.display(),
            err
        )
    })?;

    let mut runtimes = Vec::with_capacity(entries.len());
    for entry in entries {
        runtimes.push(build_runtime_status_item(resource_dir, &cache_root, &entry));
    }

    Ok(ProxyRuntimeStatus {
        target: target.to_string(),
        resource_dir: display_path(resource_dir),
        cache_root: display_path(&cache_root),
        runtimes,
    })
}

pub fn ensure_runtimes_cached_from_dirs(
    resource_dir: &Path,
    data_dir: &Path,
    target: &str,
) -> Result<ProxyRuntimeCacheState, String> {
    validate_target(target)?;
    let manifest = read_manifest(resource_dir)?;
    let entries = select_target_entries(&manifest, target)?;
    let cache_root = cache_root_for_data_dir(data_dir, target);
    fs::create_dir_all(&cache_root).map_err(|err| {
        format!(
            "创建代理内核运行缓存目录失败 {}: {}",
            cache_root.display(),
            err
        )
    })?;

    let mut runtimes = Vec::with_capacity(entries.len());
    for entry in entries {
        runtimes.push(ensure_runtime_cached(resource_dir, &cache_root, &entry)?);
    }

    Ok(ProxyRuntimeCacheState {
        target: target.to_string(),
        resource_dir: display_path(resource_dir),
        cache_root: display_path(&cache_root),
        runtimes,
    })
}

pub fn cache_root_for_current_target() -> Result<PathBuf, String> {
    let target = current_target()?;
    let data_dir = data_dir::get_data_dir()?;
    Ok(cache_root_for_data_dir(&data_dir, target))
}

pub fn resolve_resource_runtime_dir<R: TauriRuntime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|err| format!("获取 Tauri 资源目录失败: {}", err))?
        .join(RESOURCE_RUNTIME_DIR_NAME);
    if resource_dir.join(RUNTIME_MANIFEST_FILE_NAME).is_file() {
        return Ok(resource_dir);
    }

    let dev_resource_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEV_RUNTIME_BUNDLE_DIR_NAME);
    if dev_resource_dir.join(RUNTIME_MANIFEST_FILE_NAME).is_file() {
        return Ok(dev_resource_dir);
    }

    Err(format!(
        "未找到代理内核资源清单: {} 或 {}",
        resource_dir.join(RUNTIME_MANIFEST_FILE_NAME).display(),
        dev_resource_dir.join(RUNTIME_MANIFEST_FILE_NAME).display()
    ))
}

fn read_manifest(resource_dir: &Path) -> Result<RuntimeManifest, String> {
    let manifest_path = resource_dir.join(RUNTIME_MANIFEST_FILE_NAME);
    let content = fs::read_to_string(&manifest_path).map_err(|err| {
        format!(
            "读取代理内核资源清单失败 {}: {}",
            manifest_path.display(),
            err
        )
    })?;
    let manifest: RuntimeManifest = serde_json::from_str(&content).map_err(|err| {
        format!(
            "解析代理内核资源清单失败 {}: {}",
            manifest_path.display(),
            err
        )
    })?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_manifest(manifest: &RuntimeManifest) -> Result<(), String> {
    if manifest.schema_version != 1 {
        return Err(format!(
            "代理内核资源清单版本不支持: {}",
            manifest.schema_version
        ));
    }

    let mut seen = HashSet::new();
    for entry in &manifest.files {
        validate_runtime_name(&entry.runtime)?;
        validate_target(&entry.target)?;
        validate_sha256(&entry.sha256)?;
        if entry.version.trim().is_empty() {
            return Err(format!("代理内核 {} 缺少版本号", entry.runtime));
        }
        if !entry.path.starts_with(&format!("bin/{}/", entry.target)) {
            return Err(format!(
                "代理内核清单路径必须位于 bin/{}/ 下: {}",
                entry.target, entry.path
            ));
        }
        validate_relative_manifest_path(&entry.path)?;

        let key = format!("{}:{}", entry.target, entry.runtime);
        if !seen.insert(key.clone()) {
            return Err(format!("代理内核资源清单存在重复项: {}", key));
        }
    }

    Ok(())
}

fn select_target_entries(
    manifest: &RuntimeManifest,
    target: &str,
) -> Result<Vec<RuntimeManifestEntry>, String> {
    let mut selected: Vec<RuntimeManifestEntry> = manifest
        .files
        .iter()
        .filter(|entry| entry.target == target)
        .cloned()
        .collect();
    selected.sort_by_key(|entry| runtime_sort_key(&entry.runtime));

    for runtime in [XRAY_RUNTIME, SING_BOX_RUNTIME] {
        if !selected.iter().any(|entry| entry.runtime == runtime) {
            return Err(format!("代理内核资源清单缺少 {} 的 {}", target, runtime));
        }
    }

    Ok(selected)
}

fn ensure_runtime_cached(
    resource_dir: &Path,
    cache_root: &Path,
    entry: &RuntimeManifestEntry,
) -> Result<ProxyRuntimeCachedBinary, String> {
    let (source_path, source_kind) = resolve_source_path(resource_dir, entry)?;
    let source_sha256 = sha256_file(&source_path)?;
    if source_sha256 != entry.sha256 {
        return Err(format!(
            "代理内核 {} 源文件 sha256 不匹配: 期望 {}, 实际 {}, 路径 {}",
            entry.runtime,
            entry.sha256,
            source_sha256,
            source_path.display()
        ));
    }

    let binary_name = manifest_path(resource_dir, &entry.path)?
        .file_name()
        .ok_or_else(|| format!("代理内核清单路径缺少文件名: {}", entry.path))?
        .to_os_string();
    let cache_path = cache_root
        .join(&entry.runtime)
        .join(&entry.sha256)
        .join(binary_name);

    let mut cache_refreshed = false;
    if cache_path.is_file() {
        let cached_sha256 = sha256_file(&cache_path)?;
        if cached_sha256 != entry.sha256 {
            fs::remove_file(&cache_path).map_err(|err| {
                format!(
                    "删除损坏的代理内核缓存失败 {}: {}",
                    cache_path.display(),
                    err
                )
            })?;
        }
    }

    if !cache_path.is_file() {
        fs::create_dir_all(cache_path.parent().ok_or_else(|| {
            format!("代理内核缓存路径缺少父目录: {}", cache_path.display())
        })?)
        .map_err(|err| format!("创建代理内核缓存目录失败: {}", err))?;
        fs::copy(&source_path, &cache_path).map_err(|err| {
            format!(
                "复制代理内核到缓存失败 {} -> {}: {}",
                source_path.display(),
                cache_path.display(),
                err
            )
        })?;
        cache_refreshed = true;
    }

    set_executable_permission(&cache_path)?;

    let cached_sha256 = sha256_file(&cache_path)?;
    if cached_sha256 != entry.sha256 {
        return Err(format!(
            "代理内核 {} 缓存 sha256 不匹配: 期望 {}, 实际 {}, 路径 {}",
            entry.runtime,
            entry.sha256,
            cached_sha256,
            cache_path.display()
        ));
    }

    Ok(ProxyRuntimeCachedBinary {
        runtime: entry.runtime.clone(),
        expected_version: entry.version.clone(),
        manifest_sha256: entry.sha256.clone(),
        source_kind,
        source_path: display_path(&source_path),
        cache_path: display_path(&cache_path),
        cache_refreshed,
        executable: is_executable(&cache_path),
    })
}

fn build_runtime_status_item(
    resource_dir: &Path,
    cache_root: &Path,
    entry: &RuntimeManifestEntry,
) -> ProxyRuntimeStatusItem {
    match ensure_runtime_cached(resource_dir, cache_root, entry) {
        Ok(cached) => {
            let cache_path = PathBuf::from(cached.cache_path.clone());
            let (detected_version, version_output, version_error) =
                detect_runtime_version(&cache_path);
            let executable = cached.executable;
            let available = executable && version_error.is_none();
            ProxyRuntimeStatusItem {
                runtime: cached.runtime,
                expected_version: cached.expected_version,
                manifest_sha256: cached.manifest_sha256,
                source_kind: Some(cached.source_kind),
                source_path: cached.source_path,
                cache_path: cached.cache_path,
                available,
                executable,
                cache_refreshed: cached.cache_refreshed,
                detected_version,
                version_output,
                error: version_error.unwrap_or_default(),
            }
        }
        Err(error) => ProxyRuntimeStatusItem {
            runtime: entry.runtime.clone(),
            expected_version: entry.version.clone(),
            manifest_sha256: entry.sha256.clone(),
            source_kind: None,
            source_path: manifest_path(resource_dir, &entry.path)
                .map(|path| display_path(&path))
                .unwrap_or_default(),
            cache_path: cache_path_for_entry(resource_dir, cache_root, entry)
                .map(|path| display_path(&path))
                .unwrap_or_default(),
            available: false,
            executable: false,
            cache_refreshed: false,
            detected_version: String::new(),
            version_output: String::new(),
            error,
        },
    }
}

fn cache_path_for_entry(
    resource_dir: &Path,
    cache_root: &Path,
    entry: &RuntimeManifestEntry,
) -> Result<PathBuf, String> {
    let binary_name = manifest_path(resource_dir, &entry.path)?
        .file_name()
        .ok_or_else(|| format!("代理内核清单路径缺少文件名: {}", entry.path))?
        .to_os_string();
    Ok(cache_root
        .join(&entry.runtime)
        .join(&entry.sha256)
        .join(binary_name))
}

fn resolve_source_path(
    resource_dir: &Path,
    entry: &RuntimeManifestEntry,
) -> Result<(PathBuf, ProxyRuntimeSourceKind), String> {
    if let Some(override_path) = runtime_override_path(&entry.runtime)? {
        if !override_path.is_file() {
            return Err(format!(
                "代理内核 {} 覆盖路径不存在或不是文件: {}",
                entry.runtime,
                override_path.display()
            ));
        }
        return Ok((override_path, ProxyRuntimeSourceKind::Override));
    }

    let source_path = manifest_path(resource_dir, &entry.path)?;
    if !source_path.is_file() {
        return Err(format!(
            "代理内核 {} 打包资源缺失: {}",
            entry.runtime,
            source_path.display()
        ));
    }
    Ok((source_path, ProxyRuntimeSourceKind::Bundled))
}

fn runtime_override_path(runtime: &str) -> Result<Option<PathBuf>, String> {
    let env_name = match runtime {
        XRAY_RUNTIME => "COCKPIT_XRAY_PATH",
        SING_BOX_RUNTIME => "COCKPIT_SING_BOX_PATH",
        _ => return Err(format!("未知代理内核类型: {}", runtime)),
    };

    let Some(raw_path) = std::env::var_os(env_name) else {
        return Ok(None);
    };
    let override_path = PathBuf::from(raw_path);
    if override_path == PathBuf::new() {
        return Ok(None);
    }
    Ok(Some(override_path))
}

fn manifest_path(root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    validate_relative_manifest_path(relative_path)?;
    let mut path = root.to_path_buf();
    for part in relative_path.split('/') {
        path.push(part);
    }
    Ok(path)
}

fn validate_relative_manifest_path(relative_path: &str) -> Result<(), String> {
    if relative_path.trim().is_empty() {
        return Err("代理内核清单路径不能为空".to_string());
    }
    let path = Path::new(relative_path);
    if path.is_absolute() {
        return Err(format!("代理内核清单路径不能是绝对路径: {}", relative_path));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => {
                return Err(format!(
                    "代理内核清单路径不能包含特殊路径片段: {}",
                    relative_path
                ));
            }
        }
    }
    Ok(())
}

fn validate_runtime_name(runtime: &str) -> Result<(), String> {
    match runtime {
        XRAY_RUNTIME | SING_BOX_RUNTIME => Ok(()),
        _ => Err(format!("未知代理内核类型: {}", runtime)),
    }
}

fn runtime_sort_key(runtime: &str) -> u8 {
    match runtime {
        XRAY_RUNTIME => 0,
        SING_BOX_RUNTIME => 1,
        _ => 2,
    }
}

fn validate_target(target: &str) -> Result<(), String> {
    match target {
        "windows-x86_64" | "darwin-x86_64" | "darwin-aarch64" | "linux-x86_64"
        | "linux-aarch64" => Ok(()),
        _ => Err(format!("未知代理内核平台: {}", target)),
    }
}

fn cache_root_for_data_dir(data_dir: &Path, target: &str) -> PathBuf {
    data_dir
        .join(CACHE_ROOT_DIR_NAME)
        .join(CACHE_DIR_NAME)
        .join(target)
}

fn detect_runtime_version(path: &Path) -> (String, String, Option<String>) {
    if !path.is_file() {
        return (
            String::new(),
            String::new(),
            Some(format!("代理内核缓存文件不存在: {}", path.display())),
        );
    }

    let mut command = Command::new(path);
    command.arg("version");
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    match command.output() {
        Ok(output) => {
            let version_output = normalize_command_output(&output.stdout, &output.stderr);
            let detected_version = first_non_empty_line(&version_output);
            if output.status.success() {
                return (detected_version, version_output, None);
            }
            let code = output
                .status
                .code()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "terminated".to_string());
            (
                detected_version,
                version_output.clone(),
                Some(format!(
                    "代理内核版本命令失败，退出码 {}: {}",
                    code,
                    truncate_for_status(&version_output)
                )),
            )
        }
        Err(err) => (
            String::new(),
            String::new(),
            Some(format!(
                "执行代理内核版本命令失败 {}: {}",
                path.display(),
                err
            )),
        ),
    }
}

fn normalize_command_output(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout_text = String::from_utf8_lossy(stdout).trim().to_string();
    let stderr_text = String::from_utf8_lossy(stderr).trim().to_string();
    match (stdout_text.is_empty(), stderr_text.is_empty()) {
        (true, true) => String::new(),
        (false, true) => stdout_text,
        (true, false) => stderr_text,
        (false, false) => format!("{}\n{}", stdout_text, stderr_text),
    }
}

fn first_non_empty_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .to_string()
}

fn truncate_for_status(text: &str) -> String {
    const LIMIT: usize = 500;
    let trimmed = text.trim();
    if trimmed.chars().count() <= LIMIT {
        return trimmed.to_string();
    }
    let mut truncated: String = trimmed.chars().take(LIMIT).collect();
    truncated.push_str("...");
    truncated
}

fn validate_sha256(value: &str) -> Result<(), String> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(());
    }
    Err(format!("代理内核 sha256 格式无效: {}", value))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        fs::File::open(path).map_err(|err| format!("读取文件失败 {}: {}", path.display(), err))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| format!("读取文件失败 {}: {}", path.display(), err))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn set_executable_permission(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)
            .map_err(|err| format!("读取代理内核缓存权限失败 {}: {}", path.display(), err))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .map_err(|err| format!("设置代理内核缓存执行权限失败 {}: {}", path.display(), err))?;
    }

    #[cfg(not(unix))]
    {
        let _ = path;
    }

    Ok(())
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn caches_bundled_runtime_files() {
        let fixture = TestFixture::new("caches_bundled_runtime_files");
        let xray_sha = fixture.write_resource_runtime("xray", "xray.exe", b"xray-binary");
        let sing_box_sha =
            fixture.write_resource_runtime("sing-box", "sing-box.exe", b"sing-box-binary");
        fixture.write_manifest(&[
            ManifestFixtureEntry::new("xray", "xray.exe", &xray_sha),
            ManifestFixtureEntry::new("sing-box", "sing-box.exe", &sing_box_sha),
        ]);

        let state = ensure_runtimes_cached_from_dirs(
            &fixture.resource_dir,
            &fixture.data_dir,
            "windows-x86_64",
        )
        .expect("runtime cache should be prepared");

        assert_eq!(state.target, "windows-x86_64");
        assert_eq!(state.runtimes.len(), 2);
        for runtime in state.runtimes {
            assert!(PathBuf::from(runtime.cache_path).is_file());
            assert_eq!(runtime.source_kind, ProxyRuntimeSourceKind::Bundled);
            assert!(runtime.cache_refreshed);
            assert!(runtime.executable);
        }
    }

    #[test]
    fn rejects_bundled_runtime_sha_mismatch() {
        let fixture = TestFixture::new("rejects_bundled_runtime_sha_mismatch");
        fixture.write_resource_runtime("xray", "xray.exe", b"xray-binary");
        let sing_box_sha =
            fixture.write_resource_runtime("sing-box", "sing-box.exe", b"sing-box-binary");
        fixture.write_manifest(&[
            ManifestFixtureEntry::new("xray", "xray.exe", &"0".repeat(64)),
            ManifestFixtureEntry::new("sing-box", "sing-box.exe", &sing_box_sha),
        ]);

        let error = ensure_runtimes_cached_from_dirs(
            &fixture.resource_dir,
            &fixture.data_dir,
            "windows-x86_64",
        )
        .expect_err("sha mismatch must fail");

        assert!(error.contains("sha256 不匹配"));
    }

    #[test]
    fn refreshes_corrupted_cache_file() {
        let fixture = TestFixture::new("refreshes_corrupted_cache_file");
        let xray_sha = fixture.write_resource_runtime("xray", "xray.exe", b"xray-binary");
        let sing_box_sha =
            fixture.write_resource_runtime("sing-box", "sing-box.exe", b"sing-box-binary");
        fixture.write_manifest(&[
            ManifestFixtureEntry::new("xray", "xray.exe", &xray_sha),
            ManifestFixtureEntry::new("sing-box", "sing-box.exe", &sing_box_sha),
        ]);

        let xray_cache_path = fixture
            .data_dir
            .join("proxy-runtime")
            .join("cache")
            .join("windows-x86_64")
            .join("xray")
            .join(&xray_sha)
            .join("xray.exe");
        fs::create_dir_all(xray_cache_path.parent().unwrap()).unwrap();
        fs::write(&xray_cache_path, b"corrupted").unwrap();

        let state = ensure_runtimes_cached_from_dirs(
            &fixture.resource_dir,
            &fixture.data_dir,
            "windows-x86_64",
        )
        .expect("runtime cache should be repaired");

        let xray = state
            .runtimes
            .iter()
            .find(|runtime| runtime.runtime == "xray")
            .expect("xray should be present");
        assert!(xray.cache_refreshed);
        assert_eq!(fs::read(xray_cache_path).unwrap(), b"xray-binary".to_vec());
    }

    struct TestFixture {
        root: PathBuf,
        resource_dir: PathBuf,
        data_dir: PathBuf,
    }

    struct ManifestFixtureEntry {
        runtime: String,
        file_name: String,
        sha256: String,
    }

    impl ManifestFixtureEntry {
        fn new(runtime: &str, file_name: &str, sha256: &str) -> Self {
            Self {
                runtime: runtime.to_string(),
                file_name: file_name.to_string(),
                sha256: sha256.to_string(),
            }
        }
    }

    impl TestFixture {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir()
                .join(format!("cockpit-proxy-runtime-{}-{}", name, unique));
            let resource_dir = root.join("resource");
            let data_dir = root.join("data");
            fs::create_dir_all(&resource_dir).unwrap();
            fs::create_dir_all(&data_dir).unwrap();
            Self {
                root,
                resource_dir,
                data_dir,
            }
        }

        fn write_resource_runtime(&self, runtime: &str, file_name: &str, content: &[u8]) -> String {
            let path = self
                .resource_dir
                .join("bin")
                .join("windows-x86_64")
                .join(file_name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content).unwrap();
            assert!(runtime == "xray" || runtime == "sing-box");
            sha256_file(&path).unwrap()
        }

        fn write_manifest(&self, entries: &[ManifestFixtureEntry]) {
            let files: Vec<serde_json::Value> = entries
                .iter()
                .map(|entry| {
                    serde_json::json!({
                        "runtime": entry.runtime,
                        "version": "test-version",
                        "target": "windows-x86_64",
                        "path": format!("bin/windows-x86_64/{}", entry.file_name),
                        "sha256": entry.sha256,
                    })
                })
                .collect();
            let manifest = serde_json::json!({
                "schemaVersion": 1,
                "files": files,
            });
            fs::write(
                self.resource_dir.join(RUNTIME_MANIFEST_FILE_NAME),
                serde_json::to_string_pretty(&manifest).unwrap(),
            )
            .unwrap();
        }
    }

    impl Drop for TestFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
