use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const DEFAULT_DATA_DIR_NAME: &str = "tools";
const LEGACY_HOME_DATA_DIR_NAME: &str = ".antigravity_cockpit";
const LEGACY_APP_DATA_DIR_NAME: &str = "com.antigravity.cockpit-tools";
const APP_CONFIG_DIR_NAME: &str = "ai-lemon-tools";
const DATA_DIR_OVERRIDE_FILE: &str = "data-dir.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct DataDirOverride {
    #[serde(default)]
    path: Option<String>,
}

fn app_base_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(appimage) = std::env::var("APPIMAGE") {
            let appimage_path = PathBuf::from(appimage.trim());
            if !appimage_path.as_os_str().is_empty() {
                if let Some(parent) = appimage_path.parent() {
                    return Ok(parent.to_path_buf());
                }
            }
        }
    }

    let exe_path = std::env::current_exe().map_err(|e| format!("无法获取软件路径: {}", e))?;
    exe_path
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("无法解析软件所在目录: {}", exe_path.display()))
}

pub fn default_data_dir() -> Result<PathBuf, String> {
    Ok(app_base_dir()?.join(DEFAULT_DATA_DIR_NAME))
}

fn legacy_home_data_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("无法获取用户主目录")?;
    Ok(home.join(LEGACY_HOME_DATA_DIR_NAME))
}

fn normalize_path_text(path: &Path) -> String {
    let normalized = path
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();

    #[cfg(target_os = "windows")]
    {
        return normalized.to_ascii_lowercase();
    }

    #[cfg(not(target_os = "windows"))]
    {
        normalized
    }
}

fn is_legacy_home_data_dir_path(path: &Path) -> bool {
    let Ok(legacy_home_dir) = legacy_home_data_dir() else {
        return false;
    };

    path_eq(path, &legacy_home_dir)
        || normalize_path_text(path) == normalize_path_text(&legacy_home_dir)
}

fn override_config_dir() -> Result<PathBuf, String> {
    if let Some(config_dir) = dirs::config_dir() {
        return Ok(config_dir.join(APP_CONFIG_DIR_NAME));
    }

    let home = dirs::home_dir().ok_or("无法获取配置目录")?;
    Ok(home.join(".config").join(APP_CONFIG_DIR_NAME))
}

fn override_config_path() -> Result<PathBuf, String> {
    Ok(override_config_dir()?.join(DATA_DIR_OVERRIDE_FILE))
}

fn read_override_config() -> Result<DataDirOverride, String> {
    let path = override_config_path()?;
    if !path.exists() {
        return Ok(DataDirOverride::default());
    }

    let content =
        fs::read_to_string(&path).map_err(|e| format!("读取数据目录配置失败: {}", e))?;
    if content.trim().is_empty() {
        return Ok(DataDirOverride::default());
    }

    serde_json::from_str(&content).map_err(|e| format!("解析数据目录配置失败: {}", e))
}

fn write_override_config(config: &DataDirOverride) -> Result<(), String> {
    let dir = override_config_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| format!("创建数据目录配置目录失败: {}", e))?;
    }

    let content =
        serde_json::to_string_pretty(config).map_err(|e| format!("序列化数据目录配置失败: {}", e))?;
    fs::write(override_config_path()?, content)
        .map_err(|e| format!("写入数据目录配置失败: {}", e))
}

fn configured_data_dir() -> Result<Option<PathBuf>, String> {
    let config = read_override_config()?;
    let Some(path) = config.path else {
        return Ok(None);
    };

    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let configured = PathBuf::from(trimmed);
    if is_legacy_home_data_dir_path(&configured) {
        return Ok(None);
    }

    Ok(Some(configured))
}

pub fn get_data_dir() -> Result<PathBuf, String> {
    let data_dir = configured_data_dir()?.unwrap_or(default_data_dir()?);
    if !data_dir.exists() {
        fs::create_dir_all(&data_dir).map_err(|e| format!("创建数据目录失败: {}", e))?;
    }
    merge_legacy_data_dirs_into(&data_dir);
    Ok(data_dir)
}

fn canonicalize_existing(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn is_dir_empty(path: &Path) -> Result<bool, String> {
    let mut entries = fs::read_dir(path).map_err(|e| format!("读取数据目录失败: {}", e))?;
    Ok(entries.next().is_none())
}

fn copy_dir_contents(source: &Path, target: &Path) -> Result<(), String> {
    for entry in fs::read_dir(source).map_err(|e| format!("读取当前数据目录失败: {}", e))? {
        let entry = entry.map_err(|e| format!("读取当前数据目录条目失败: {}", e))?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());

        if source_path.is_dir() {
            fs::create_dir_all(&target_path)
                .map_err(|e| format!("创建数据目录子目录失败: {}", e))?;
            copy_dir_contents(&source_path, &target_path)?;
        } else if source_path.is_file() {
            fs::copy(&source_path, &target_path)
                .map_err(|e| format!("复制数据文件失败: {}", e))?;
        }
    }

    Ok(())
}

fn path_eq(left: &Path, right: &Path) -> bool {
    canonicalize_existing(left) == canonicalize_existing(right)
}

fn path_nested(left: &Path, right: &Path) -> bool {
    let left = canonicalize_existing(left);
    let right = canonicalize_existing(right);
    left.starts_with(&right) || right.starts_with(&left)
}

fn legacy_data_dir_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(legacy_home_dir) = legacy_home_data_dir() {
        candidates.push(legacy_home_dir);
    }

    if let Some(data_local_dir) = dirs::data_local_dir() {
        candidates.push(data_local_dir.join(LEGACY_APP_DATA_DIR_NAME));
    }

    candidates
}

fn files_have_same_content(left: &Path, right: &Path) -> bool {
    let Ok(left_meta) = fs::metadata(left) else {
        return false;
    };
    let Ok(right_meta) = fs::metadata(right) else {
        return false;
    };
    if left_meta.len() != right_meta.len() {
        return false;
    }

    let Ok(mut left_file) = fs::File::open(left) else {
        return false;
    };
    let Ok(mut right_file) = fs::File::open(right) else {
        return false;
    };

    let mut left_buf = Vec::new();
    let mut right_buf = Vec::new();
    if left_file.read_to_end(&mut left_buf).is_err()
        || right_file.read_to_end(&mut right_buf).is_err()
    {
        return false;
    }
    left_buf == right_buf
}

fn remove_dir_if_empty(path: &Path) {
    if fs::read_dir(path)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
    {
        let _ = fs::remove_dir(path);
    }
}

fn merge_dir_contents_preserving_target(source: &Path, target: &Path) -> Result<(), String> {
    if !source.exists() || !source.is_dir() {
        return Ok(());
    }
    if path_eq(source, target) || path_nested(source, target) {
        return Ok(());
    }

    fs::create_dir_all(target).map_err(|e| format!("创建目标数据目录失败: {}", e))?;

    let entries = match fs::read_dir(source) {
        Ok(entries) => entries,
        Err(error) => {
            return Err(format!("读取旧数据目录失败: {}", error));
        }
    };

    for entry in entries {
        let entry = entry.map_err(|e| format!("读取旧数据目录条目失败: {}", e))?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());

        if source_path.is_dir() {
            merge_dir_contents_preserving_target(&source_path, &target_path)?;
            remove_dir_if_empty(&source_path);
            continue;
        }

        if !source_path.is_file() {
            continue;
        }

        if target_path.exists() {
            if target_path.is_file() && files_have_same_content(&source_path, &target_path) {
                let _ = fs::remove_file(&source_path);
            }
            continue;
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目标数据子目录失败: {}", e))?;
        }
        fs::copy(&source_path, &target_path).map_err(|e| format!("迁移数据文件失败: {}", e))?;
        let _ = fs::remove_file(&source_path);
    }

    remove_dir_if_empty(source);
    Ok(())
}

fn merge_legacy_data_dirs_into(target: &Path) {
    for source in legacy_data_dir_candidates() {
        if !source.exists() || path_eq(&source, target) || path_nested(&source, target) {
            continue;
        }

        if let Err(error) = merge_dir_contents_preserving_target(&source, target) {
            eprintln!(
                "[DataDirMigration] 旧数据目录迁移跳过: source={}, target={}, error={}",
                source.display(),
                target.display(),
                error
            );
        }
    }
}

pub fn set_data_dir_path(path: PathBuf) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("数据目录不能为空".to_string());
    }
    if !path.is_absolute() {
        return Err("请选择绝对路径作为数据目录".to_string());
    }

    if !path.exists() {
        fs::create_dir_all(&path).map_err(|e| format!("创建数据目录失败: {}", e))?;
    }
    if !path.is_dir() {
        return Err("请选择文件夹作为数据目录".to_string());
    }

    let current = get_data_dir()?;
    let current_canonical = canonicalize_existing(&current);
    let next_canonical = canonicalize_existing(&path);
    if current_canonical != next_canonical {
        if next_canonical.starts_with(&current_canonical)
            || current_canonical.starts_with(&next_canonical)
        {
            return Err("新数据目录不能与当前数据目录互相嵌套".to_string());
        }

        if is_dir_empty(&path)? {
            copy_dir_contents(&current, &path)?;
        }
    }

    write_override_config(&DataDirOverride {
        path: Some(path.to_string_lossy().to_string()),
    })?;
    Ok(path)
}

pub fn reset_data_dir_path() -> Result<PathBuf, String> {
    let path = override_config_path()?;
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("重置数据目录失败: {}", e))?;
    }
    get_data_dir()
}
