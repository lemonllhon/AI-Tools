use super::models::{
    ProxyImportApplyRequest, ProxyImportApplyResponse, ProxyImportPreviewRequest,
    ProxyImportPreviewResponse, ProxyNodeSaveRequest, ProxyPoolListResponse, ProxyPoolNode,
    DIRECT_NODE_ID, LOCAL_NODE_ID,
};
use super::parser;
use crate::modules::data_dir;
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::json;
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

const DB_DIR_NAME: &str = "proxy-pool";
const DB_FILE_NAME: &str = "proxy_pool.db";
const BUILTIN_GROUP: &str = "内置";
const DEFAULT_GROUP: &str = "默认";
const SUPPORTED_MANUAL_PROTOCOLS: &[&str] = &["http", "https", "socks5"];

pub fn list_nodes() -> Result<ProxyPoolListResponse, String> {
    let db_path = proxy_pool_db_path()?;
    let conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;

    let mut stmt = conn
        .prepare(
            "SELECT
                id, name, protocol, host, port, username, password, group_name,
                source_id, source_name, sort_order, enabled, builtin, latency_ms,
                latency_status, ip_health_summary, created_at, updated_at
             FROM proxy_nodes
             ORDER BY builtin DESC, sort_order ASC, created_at ASC",
        )
        .map_err(|err| format!("读取代理节点失败: {}", err))?;

    let nodes = stmt
        .query_map([], map_node_row)
        .map_err(|err| format!("读取代理节点失败: {}", err))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取代理节点失败: {}", err))?;

    let groups = collect_groups(&nodes);

    Ok(ProxyPoolListResponse {
        db_path: display_path(&db_path),
        nodes,
        groups,
    })
}

pub fn save_node(request: ProxyNodeSaveRequest) -> Result<ProxyPoolNode, String> {
    let db_path = proxy_pool_db_path()?;
    let conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;

    let normalized = NormalizedNodeInput::from_request(request)?;
    let now = now_iso();
    let existing = normalized
        .id
        .as_deref()
        .map(|id| load_existing_node_meta(&conn, id))
        .transpose()?
        .flatten();

    if existing.as_ref().is_some_and(|meta| meta.builtin) {
        return Err("内置代理节点不能被覆盖".to_string());
    }

    let id = normalized.id.unwrap_or_else(|| Uuid::new_v4().to_string());
    if id == DIRECT_NODE_ID || id == LOCAL_NODE_ID {
        return Err("代理节点 ID 与内置节点冲突".to_string());
    }

    let sort_order = match existing.as_ref() {
        Some(meta) => meta.sort_order,
        None => next_sort_order(&conn)?,
    };
    let created_at = existing
        .as_ref()
        .map(|meta| meta.created_at.clone())
        .unwrap_or_else(|| now.clone());
    let password = match normalized.password {
        Some(password) => password,
        None => existing
            .as_ref()
            .map(|meta| meta.password.clone())
            .unwrap_or_default(),
    };
    let raw_config = build_standard_config_json(
        &normalized.protocol,
        &normalized.host,
        normalized.port,
        &normalized.username,
        &password,
    )?;

    conn.execute(
        "INSERT INTO proxy_nodes (
            id, name, protocol, host, port, username, password, raw_config, standard_config,
            group_name, dns, source_id, source_name, sort_order, enabled, builtin,
            latency_ms, latency_status, ip_health_json, ip_health_summary, created_at, updated_at
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8,
            ?9, '', NULL, '', ?10, ?11, 0,
            NULL, '', '', '', ?12, ?13
         )
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            protocol = excluded.protocol,
            host = excluded.host,
            port = excluded.port,
            username = excluded.username,
            password = excluded.password,
            raw_config = excluded.raw_config,
            standard_config = excluded.standard_config,
            group_name = excluded.group_name,
            enabled = excluded.enabled,
            updated_at = excluded.updated_at",
        params![
            id,
            normalized.name,
            normalized.protocol,
            normalized.host,
            normalized.port,
            normalized.username,
            password,
            raw_config,
            normalized.group,
            sort_order,
            if normalized.enabled { 1 } else { 0 },
            created_at,
            now,
        ],
    )
    .map_err(|err| format!("保存代理节点失败: {}", err))?;

    get_node(&conn, &id)?.ok_or_else(|| "代理节点保存后读取失败".to_string())
}

pub fn delete_node(id: &str) -> Result<(), String> {
    delete_nodes(&[id.to_string()])
}

pub fn delete_nodes(ids: &[String]) -> Result<(), String> {
    if ids.is_empty() {
        return Ok(());
    }

    let db_path = proxy_pool_db_path()?;
    let mut conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;

    for id in ids {
        let Some(meta) = load_existing_node_meta(&conn, id)? else {
            continue;
        };
        if meta.builtin || id == DIRECT_NODE_ID {
            return Err(format!("内置代理节点不能删除: {}", id));
        }
    }

    let tx = conn
        .transaction()
        .map_err(|err| format!("删除代理节点失败: {}", err))?;
    for id in ids {
        tx.execute("DELETE FROM proxy_nodes WHERE id = ?1", params![id])
            .map_err(|err| format!("删除代理节点失败: {}", err))?;
    }
    tx.commit()
        .map_err(|err| format!("删除代理节点失败: {}", err))?;

    Ok(())
}

pub fn set_node_enabled(id: &str, enabled: bool) -> Result<ProxyPoolNode, String> {
    let db_path = proxy_pool_db_path()?;
    let conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;

    let Some(meta) = load_existing_node_meta(&conn, id)? else {
        return Err("代理节点不存在".to_string());
    };
    if id == DIRECT_NODE_ID && !enabled {
        return Err("内置直连节点不能禁用".to_string());
    }

    conn.execute(
        "UPDATE proxy_nodes SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
        params![if enabled { 1 } else { 0 }, now_iso(), id],
    )
    .map_err(|err| format!("更新代理节点状态失败: {}", err))?;

    get_node(&conn, id)?.ok_or_else(|| {
        format!(
            "代理节点状态更新后读取失败: {}",
            if meta.builtin { "内置节点" } else { "自定义节点" }
        )
    })
}

pub fn preview_import(
    request: ProxyImportPreviewRequest,
) -> Result<ProxyImportPreviewResponse, String> {
    parser::preview_import(&request)
}

pub fn apply_import(request: ProxyImportApplyRequest) -> Result<ProxyImportApplyResponse, String> {
    if request.selected_preview_ids.is_empty() {
        return Err("请选择要导入的代理节点".to_string());
    }

    let selected_ids: HashSet<String> = request.selected_preview_ids.into_iter().collect();
    let preview_request = ProxyImportPreviewRequest {
        content: request.content,
        group: request.group,
        name_prefix: request.name_prefix,
    };
    let parsed = parser::parse_import_request(&preview_request)?;
    let selected_nodes: Vec<_> = parsed
        .nodes
        .into_iter()
        .filter(|node| selected_ids.contains(&node.preview_id))
        .collect();

    if selected_nodes.is_empty() {
        return Err("所选代理节点已失效，请重新预览后再导入".to_string());
    }

    let db_path = proxy_pool_db_path()?;
    let mut conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;

    let mut sort_order = next_sort_order(&conn)?;
    let now = now_iso();
    let tx = conn
        .transaction()
        .map_err(|err| format!("导入代理节点失败: {}", err))?;
    let mut imported = 0usize;
    for node in &selected_nodes {
        insert_imported_node(&tx, node, sort_order, &now)?;
        sort_order += 1;
        imported += 1;
    }
    tx.commit()
        .map_err(|err| format!("导入代理节点失败: {}", err))?;

    let list = list_nodes()?;
    Ok(ProxyImportApplyResponse {
        imported,
        skipped: selected_ids.len().saturating_sub(imported),
        nodes: list.nodes,
    })
}

pub fn proxy_pool_db_path() -> Result<PathBuf, String> {
    let dir = data_dir::get_data_dir()?.join(DB_DIR_NAME);
    fs::create_dir_all(&dir)
        .map_err(|err| format!("创建代理池数据目录失败 {}: {}", dir.display(), err))?;
    Ok(dir.join(DB_FILE_NAME))
}

fn open_connection_at(db_path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(db_path).map_err(|err| {
        format!(
            "打开代理池数据库失败 {}: {}",
            db_path.display(),
            err
        )
    })?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|err| format!("设置代理池数据库超时失败: {}", err))?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|err| format!("启用代理池数据库 WAL 失败: {}", err))?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|err| format!("启用代理池数据库外键失败: {}", err))?;
    Ok(conn)
}

fn initialize_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS proxy_nodes (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            protocol TEXT NOT NULL,
            host TEXT NOT NULL DEFAULT '',
            port INTEGER NOT NULL DEFAULT 0,
            username TEXT NOT NULL DEFAULT '',
            password TEXT NOT NULL DEFAULT '',
            raw_config TEXT NOT NULL DEFAULT '{}',
            standard_config TEXT NOT NULL DEFAULT '{}',
            group_name TEXT NOT NULL DEFAULT '',
            dns TEXT NOT NULL DEFAULT '',
            source_id TEXT,
            source_name TEXT NOT NULL DEFAULT '',
            sort_order INTEGER NOT NULL DEFAULT 0,
            enabled INTEGER NOT NULL DEFAULT 1,
            builtin INTEGER NOT NULL DEFAULT 0,
            latency_ms INTEGER,
            latency_status TEXT NOT NULL DEFAULT '',
            ip_health_json TEXT NOT NULL DEFAULT '',
            ip_health_summary TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_proxy_nodes_group ON proxy_nodes(group_name);
        CREATE INDEX IF NOT EXISTS idx_proxy_nodes_protocol ON proxy_nodes(protocol);
        CREATE INDEX IF NOT EXISTS idx_proxy_nodes_source ON proxy_nodes(source_id);
        CREATE INDEX IF NOT EXISTS idx_proxy_nodes_enabled ON proxy_nodes(enabled);

        CREATE TABLE IF NOT EXISTS proxy_sources (
            id TEXT PRIMARY KEY,
            url TEXT NOT NULL,
            name_prefix TEXT NOT NULL DEFAULT '',
            group_name TEXT NOT NULL DEFAULT '',
            dns TEXT NOT NULL DEFAULT '',
            auto_refresh_enabled INTEGER NOT NULL DEFAULT 0,
            refresh_interval_minutes INTEGER NOT NULL DEFAULT 360,
            last_refresh_at TEXT,
            last_error TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS proxy_service_state (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            enabled INTEGER NOT NULL DEFAULT 0,
            preferred_port INTEGER NOT NULL DEFAULT 7897,
            actual_port INTEGER,
            current_node_id TEXT NOT NULL DEFAULT '__direct__',
            global_proxy_mode TEXT NOT NULL DEFAULT 'manual',
            updated_at TEXT NOT NULL
        );
        ",
    )
    .map_err(|err| format!("初始化代理池数据库失败: {}", err))?;

    conn.execute(
        "INSERT OR IGNORE INTO schema_migrations (version, applied_at) VALUES (1, ?1)",
        params![now_iso()],
    )
    .map_err(|err| format!("写入代理池数据库迁移记录失败: {}", err))?;

    Ok(())
}

fn seed_builtin_nodes(conn: &Connection) -> Result<(), String> {
    let now = now_iso();
    seed_builtin_node(
        conn,
        BuiltinNodeSeed {
            id: DIRECT_NODE_ID,
            name: "直连",
            protocol: "direct",
            host: "",
            port: 0,
            enabled: true,
            sort_order: 0,
            now: &now,
        },
    )?;
    seed_builtin_node(
        conn,
        BuiltinNodeSeed {
            id: LOCAL_NODE_ID,
            name: "本地代理 127.0.0.1:7890",
            protocol: "http",
            host: "127.0.0.1",
            port: 7890,
            enabled: false,
            sort_order: 1,
            now: &now,
        },
    )?;

    conn.execute(
        "INSERT OR IGNORE INTO proxy_service_state (
            id, enabled, preferred_port, actual_port, current_node_id, global_proxy_mode, updated_at
         ) VALUES (1, 0, 7897, NULL, ?1, 'manual', ?2)",
        params![DIRECT_NODE_ID, now],
    )
    .map_err(|err| format!("初始化代理池服务状态失败: {}", err))?;

    Ok(())
}

struct BuiltinNodeSeed<'a> {
    id: &'a str,
    name: &'a str,
    protocol: &'a str,
    host: &'a str,
    port: u16,
    enabled: bool,
    sort_order: i64,
    now: &'a str,
}

fn seed_builtin_node(conn: &Connection, seed: BuiltinNodeSeed<'_>) -> Result<(), String> {
    let standard_config = build_standard_config_json(seed.protocol, seed.host, seed.port, "", "")?;
    conn.execute(
        "INSERT OR IGNORE INTO proxy_nodes (
            id, name, protocol, host, port, username, password, raw_config, standard_config,
            group_name, dns, source_id, source_name, sort_order, enabled, builtin,
            latency_ms, latency_status, ip_health_json, ip_health_summary, created_at, updated_at
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, '', '', ?6, ?6,
            ?7, '', NULL, '', ?8, ?9, 1,
            NULL, '', '', '', ?10, ?10
         )",
        params![
            seed.id,
            seed.name,
            seed.protocol,
            seed.host,
            seed.port,
            standard_config,
            BUILTIN_GROUP,
            seed.sort_order,
            if seed.enabled { 1 } else { 0 },
            seed.now,
        ],
    )
    .map_err(|err| format!("初始化内置代理节点失败: {}", err))?;

    Ok(())
}

fn get_node(conn: &Connection, id: &str) -> Result<Option<ProxyPoolNode>, String> {
    conn.query_row(
        "SELECT
            id, name, protocol, host, port, username, password, group_name,
            source_id, source_name, sort_order, enabled, builtin, latency_ms,
            latency_status, ip_health_summary, created_at, updated_at
         FROM proxy_nodes
         WHERE id = ?1",
        params![id],
        map_node_row,
    )
    .optional()
    .map_err(|err| format!("读取代理节点失败: {}", err))
}

fn insert_imported_node(
    tx: &Transaction<'_>,
    node: &parser::ParsedProxyNode,
    sort_order: i64,
    now: &str,
) -> Result<(), String> {
    let id = Uuid::new_v4().to_string();
    let raw_config = serde_json::to_string(&node.raw_config)
        .map_err(|err| format!("序列化导入代理节点原始配置失败: {}", err))?;
    let standard_config = serde_json::to_string(&node.standard_config)
        .map_err(|err| format!("序列化导入代理节点标准配置失败: {}", err))?;

    tx.execute(
        "INSERT INTO proxy_nodes (
            id, name, protocol, host, port, username, password, raw_config, standard_config,
            group_name, dns, source_id, source_name, sort_order, enabled, builtin,
            latency_ms, latency_status, ip_health_json, ip_health_summary, created_at, updated_at
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9,
            ?10, '', NULL, ?11, ?12, 1, 0,
            NULL, '', '', '', ?13, ?13
         )",
        params![
            id,
            node.name,
            node.protocol,
            node.host,
            node.port,
            node.username,
            node.password,
            raw_config,
            standard_config,
            node.group,
            node.source_kind,
            sort_order,
            now,
        ],
    )
    .map_err(|err| format!("导入代理节点失败: {}", err))?;

    Ok(())
}

fn map_node_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProxyPoolNode> {
    let protocol: String = row.get(2)?;
    let host: String = row.get(3)?;
    let port_raw: i64 = row.get(4)?;
    let username: String = row.get(5)?;
    let password: String = row.get(6)?;
    let builtin_raw: i64 = row.get(12)?;
    let port = u16::try_from(port_raw).unwrap_or(0);
    let has_password = !password.is_empty();

    Ok(ProxyPoolNode {
        id: row.get(0)?,
        name: row.get(1)?,
        protocol: protocol.clone(),
        host: host.clone(),
        port,
        username: mask_username(&username),
        has_password,
        group: row.get(7)?,
        source_id: row.get(8)?,
        source_name: row.get(9)?,
        sort_order: row.get(10)?,
        enabled: row.get::<_, i64>(11)? != 0,
        builtin: builtin_raw != 0,
        latency_ms: row.get(13)?,
        latency_status: row.get(14)?,
        ip_health_summary: row.get(15)?,
        masked_url: build_masked_url(&protocol, &host, port, &username, has_password),
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}

#[derive(Debug, Clone)]
struct ExistingNodeMeta {
    password: String,
    sort_order: i64,
    builtin: bool,
    created_at: String,
}

fn load_existing_node_meta(
    conn: &Connection,
    id: &str,
) -> Result<Option<ExistingNodeMeta>, String> {
    conn.query_row(
        "SELECT password, sort_order, builtin, created_at FROM proxy_nodes WHERE id = ?1",
        params![id],
        |row| {
            Ok(ExistingNodeMeta {
                password: row.get(0)?,
                sort_order: row.get(1)?,
                builtin: row.get::<_, i64>(2)? != 0,
                created_at: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(|err| format!("读取代理节点元信息失败: {}", err))
}

fn next_sort_order(conn: &Connection) -> Result<i64, String> {
    let current_max: Option<i64> = conn
        .query_row(
            "SELECT MAX(sort_order) FROM proxy_nodes WHERE builtin = 0",
            [],
            |row| row.get(0),
        )
        .map_err(|err| format!("读取代理节点排序失败: {}", err))?;
    Ok(current_max.unwrap_or(1) + 1)
}

#[derive(Debug, Clone)]
struct NormalizedNodeInput {
    id: Option<String>,
    name: String,
    protocol: String,
    host: String,
    port: u16,
    username: String,
    password: Option<String>,
    group: String,
    enabled: bool,
}

impl NormalizedNodeInput {
    fn from_request(request: ProxyNodeSaveRequest) -> Result<Self, String> {
        let id = request.id.map(|value| value.trim().to_string()).filter(|value| !value.is_empty());
        let name = normalize_required_text(&request.name, "节点名称", 120)?;
        let protocol = request.protocol.trim().to_ascii_lowercase();
        if !SUPPORTED_MANUAL_PROTOCOLS.contains(&protocol.as_str()) {
            return Err("当前阶段仅支持手动添加 http、https、socks5 节点".to_string());
        }

        let host = normalize_host(&request.host)?;
        if request.port == 0 {
            return Err("端口必须在 1-65535 之间".to_string());
        }

        let username = normalize_optional_text(request.username, 240)?;
        let password = request.password.map(|value| value.trim().to_string());
        let group = normalize_optional_text(request.group, 80)?;
        let group = if group.is_empty() {
            DEFAULT_GROUP.to_string()
        } else {
            group
        };

        Ok(Self {
            id,
            name,
            protocol,
            host,
            port: request.port,
            username,
            password,
            group,
            enabled: request.enabled.unwrap_or(true),
        })
    }
}

fn normalize_required_text(value: &str, label: &str, max_len: usize) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{}不能为空", label));
    }
    if trimmed.chars().count() > max_len {
        return Err(format!("{}不能超过 {} 个字符", label, max_len));
    }
    Ok(trimmed.to_string())
}

fn normalize_optional_text(value: Option<String>, max_len: usize) -> Result<String, String> {
    let trimmed = value.unwrap_or_default().trim().to_string();
    if trimmed.chars().count() > max_len {
        return Err(format!("文本不能超过 {} 个字符", max_len));
    }
    Ok(trimmed)
}

fn normalize_host(value: &str) -> Result<String, String> {
    let host = normalize_required_text(value, "节点地址", 255)?;
    if host.contains("://") || host.contains('/') || host.chars().any(char::is_whitespace) {
        return Err("节点地址只填写主机名或 IP，不要包含协议、路径或空格".to_string());
    }
    Ok(host)
}

fn build_standard_config_json(
    protocol: &str,
    host: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<String, String> {
    serde_json::to_string(&json!({
        "protocol": protocol,
        "host": host,
        "port": port,
        "username": username,
        "password": password,
    }))
    .map_err(|err| format!("序列化代理节点配置失败: {}", err))
}

fn collect_groups(nodes: &[ProxyPoolNode]) -> Vec<String> {
    let mut groups = BTreeSet::new();
    for group in nodes.iter().map(|node| node.group.trim()).filter(|group| !group.is_empty()) {
        groups.insert(group.to_string());
    }
    groups.into_iter().collect()
}

fn build_masked_url(
    protocol: &str,
    host: &str,
    port: u16,
    username: &str,
    has_password: bool,
) -> String {
    if protocol == "direct" {
        return "direct://".to_string();
    }

    let auth = if username.is_empty() {
        String::new()
    } else if has_password {
        format!("{}:***@", mask_username(username))
    } else {
        format!("{}@", mask_username(username))
    };

    format!("{}://{}{}:{}", protocol, auth, host, port)
}

fn mask_username(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    if chars.next().is_none() {
        return "*".to_string();
    }

    format!("{}***", first)
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_proxy_credentials() {
        assert_eq!(
            build_masked_url("socks5", "example.com", 1080, "lemon", true),
            "socks5://l***:***@example.com:1080"
        );
        assert_eq!(
            build_masked_url("http", "127.0.0.1", 7890, "", false),
            "http://127.0.0.1:7890"
        );
    }

    #[test]
    fn rejects_manual_protocols_outside_phase_two_scope() {
        let request = ProxyNodeSaveRequest {
            id: None,
            name: "vmess".to_string(),
            protocol: "vmess".to_string(),
            host: "example.com".to_string(),
            port: 443,
            username: None,
            password: None,
            group: None,
            enabled: None,
        };

        let error = NormalizedNodeInput::from_request(request).expect_err("vmess is later phase");
        assert!(error.contains("http、https、socks5"));
    }
}
