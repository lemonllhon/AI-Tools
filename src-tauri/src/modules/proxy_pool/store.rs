use super::models::{
    ProxyImportApplyRequest, ProxyImportApplyResponse, ProxyImportPreviewRequest,
    ProxyImportPreviewCheckItem, ProxyImportPreviewCheckRequest, ProxyImportPreviewCheckResponse,
    ProxyImportPreviewResponse, ProxyNodeSaveRequest, ProxyPoolCheckProgressEvent,
    ProxyPoolIpHealthResponse, ProxyPoolIpHealthResult, ProxyPoolLatencyTestResponse,
    ProxyPoolLatencyTestResult, ProxyPoolListResponse, ProxyPoolNode, ProxyPoolServiceState,
    ProxyPoolServiceUpdateRequest, ProxySource, ProxySourceUpdateRequest,
    ProxySubscriptionApplyRequest, ProxySubscriptionApplyResponse, ProxySubscriptionPreviewCheckRequest,
    ProxySubscriptionPreviewRequest, ProxySubscriptionRefreshItem, ProxySubscriptionRefreshRequest,
    ProxySubscriptionRefreshResponse, DIRECT_NODE_ID, LOCAL_NODE_ID, OUTLET_MODE_DIRECT,
    OUTLET_MODE_LOCAL, OUTLET_MODE_NODE_POOL,
};
use super::health::{self, ProxyCheckTarget};
use super::parser;
use super::subscription;
use crate::modules::data_dir;
use chrono::{SecondsFormat, Utc};
use futures::{stream, StreamExt};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use url::Url;
use uuid::Uuid;

const DB_DIR_NAME: &str = "proxy-pool";
const DB_FILE_NAME: &str = "proxy_pool.db";
const BUILTIN_GROUP: &str = "内置";
const DEFAULT_GROUP: &str = "默认";
const SUPPORTED_MANUAL_PROTOCOLS: &[&str] = &["http", "https", "socks5"];
const LATENCY_BATCH_CONCURRENCY: usize = 12;
const IP_HEALTH_BATCH_CONCURRENCY: usize = 6;
const PREVIEW_BRIDGE_BATCH_CONCURRENCY: usize = 4;
const PREVIEW_LATENCY_TARGET_TIMEOUT: Duration = Duration::from_secs(22);
const PREVIEW_IP_HEALTH_TARGET_TIMEOUT: Duration = Duration::from_secs(34);
const DEFAULT_PROXY_GATEWAY_PORT: u16 = 7897;
const DEFAULT_LOCAL_PROXY_PORT: u16 = 7890;

pub type ProxyPoolProgressEmitter = Arc<dyn Fn(ProxyPoolCheckProgressEvent) + Send + Sync>;

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
                latency_status, ip_health_json, ip_health_summary, created_at, updated_at
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
    let sources = list_sources_from_conn(&conn)?;
    let service_state = service_state_from_conn(&conn)?;

    Ok(ProxyPoolListResponse {
        db_path: display_path(&db_path),
        nodes,
        groups,
        sources,
        service_state,
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

    normalize_service_state_in_conn(&conn, &now)?;

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
    let current_node_id: Option<String> = tx
        .query_row(
            "SELECT current_node_id FROM proxy_service_state WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|err| format!("读取当前出口节点失败: {}", err))?;
    if ids
        .iter()
        .any(|id| current_node_id.as_deref() == Some(id.as_str()))
    {
        tx.execute(
            "UPDATE proxy_service_state SET current_node_id = ?1, updated_at = ?2 WHERE id = 1",
            params![DIRECT_NODE_ID, now_iso()],
        )
        .map_err(|err| format!("切换当前出口节点失败: {}", err))?;
    }
    for id in ids {
        tx.execute("DELETE FROM proxy_nodes WHERE id = ?1", params![id])
            .map_err(|err| format!("删除代理节点失败: {}", err))?;
    }
    reset_current_node_if_missing_in_tx(&tx, &now_iso())?;
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
    let current_state = service_state_from_conn(&conn)?;
    let current_outlet_mode = current_state.outlet_mode;
    let current_node_id = current_state.current_node_id;
    let mut selected_node_ids = current_state.selected_node_ids;
    let request = if id == DIRECT_NODE_ID {
        if !enabled {
            return Err("直连出口不能单独停用，请切换到本地代理或节点池".to_string());
        }
        ProxyPoolServiceUpdateRequest {
            enabled: None,
            preferred_port: None,
            outlet_mode: Some(OUTLET_MODE_DIRECT.to_string()),
            selected_node_ids: Some(Vec::new()),
            current_node_id: Some(DIRECT_NODE_ID.to_string()),
            local_proxy_port: None,
        }
    } else if id == LOCAL_NODE_ID {
        if !enabled {
            return Err("本地代理出口不能单独停用，请切换到直连或节点池".to_string());
        }
        ProxyPoolServiceUpdateRequest {
            enabled: None,
            preferred_port: None,
            outlet_mode: Some(OUTLET_MODE_LOCAL.to_string()),
            selected_node_ids: Some(Vec::new()),
            current_node_id: Some(LOCAL_NODE_ID.to_string()),
            local_proxy_port: None,
        }
    } else {
        if meta.builtin {
            return Err("内置节点不能加入节点池".to_string());
        }
        if enabled {
            if !selected_node_ids.iter().any(|node_id| node_id == id) {
                selected_node_ids.push(id.to_string());
            }
            let current_node_id = if current_outlet_mode == OUTLET_MODE_NODE_POOL
                && selected_node_ids
                    .iter()
                    .any(|node_id| node_id == &current_node_id)
            {
                current_node_id
            } else {
                id.to_string()
            };
            ProxyPoolServiceUpdateRequest {
                enabled: None,
                preferred_port: None,
                outlet_mode: Some(OUTLET_MODE_NODE_POOL.to_string()),
                selected_node_ids: Some(selected_node_ids),
                current_node_id: Some(current_node_id),
                local_proxy_port: None,
            }
        } else {
            selected_node_ids.retain(|node_id| node_id != id);
            if selected_node_ids.is_empty() {
                ProxyPoolServiceUpdateRequest {
                    enabled: None,
                    preferred_port: None,
                    outlet_mode: Some(OUTLET_MODE_DIRECT.to_string()),
                    selected_node_ids: Some(Vec::new()),
                    current_node_id: Some(DIRECT_NODE_ID.to_string()),
                    local_proxy_port: None,
                }
            } else {
                let current_node_id = if current_node_id == id {
                    selected_node_ids[0].clone()
                } else {
                    current_node_id
                };
                ProxyPoolServiceUpdateRequest {
                    enabled: None,
                    preferred_port: None,
                    outlet_mode: Some(OUTLET_MODE_NODE_POOL.to_string()),
                    selected_node_ids: Some(selected_node_ids),
                    current_node_id: Some(current_node_id),
                    local_proxy_port: None,
                }
            }
        }
    };

    update_service_state_config(request)?;
    let conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;

    get_node(&conn, id)?.ok_or_else(|| {
        format!(
            "代理节点状态更新后读取失败: {}",
            if meta.builtin {
                "内置节点"
            } else {
                "自定义节点"
            }
        )
    })
}

pub fn preview_import(
    request: ProxyImportPreviewRequest,
) -> Result<ProxyImportPreviewResponse, String> {
    parser::preview_import(&request)
}

pub async fn check_import_preview(
    request: ProxyImportPreviewCheckRequest,
) -> Result<ProxyImportPreviewCheckResponse, String> {
    let selected_ids = normalize_preview_check_ids(request.selected_preview_ids)?;
    let check_kind = parse_preview_check_kind(&request.check_kind)?;
    let preview_request = ProxyImportPreviewRequest {
        content: request.content,
        group: request.group,
        name_prefix: request.name_prefix,
    };
    let parsed = parser::parse_import_request(&preview_request)?;
    check_parsed_preview_nodes(parsed.nodes, &selected_ids, check_kind).await
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
        insert_imported_node(&tx, node, sort_order, &now, None)?;
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

pub async fn preview_subscription(
    request: ProxySubscriptionPreviewRequest,
) -> Result<ProxyImportPreviewResponse, String> {
    let fetched = subscription::fetch_subscription(&request.url).await?;
    parser::preview_import(&ProxyImportPreviewRequest {
        content: fetched.content,
        group: request.group,
        name_prefix: request.name_prefix,
    })
}

pub async fn check_subscription_preview(
    request: ProxySubscriptionPreviewCheckRequest,
) -> Result<ProxyImportPreviewCheckResponse, String> {
    let selected_ids = normalize_preview_check_ids(request.selected_preview_ids)?;
    let check_kind = parse_preview_check_kind(&request.check_kind)?;
    let fetched = subscription::fetch_subscription(&request.url).await?;
    let preview_request = ProxyImportPreviewRequest {
        content: fetched.content,
        group: request.group,
        name_prefix: request.name_prefix,
    };
    let parsed = parser::parse_import_request(&preview_request)?;
    check_parsed_preview_nodes(parsed.nodes, &selected_ids, check_kind).await
}

#[derive(Debug, Clone, Copy)]
enum PreviewCheckKind {
    Latency,
    IpHealth,
}

fn normalize_preview_check_ids(ids: Vec<String>) -> Result<HashSet<String>, String> {
    let ids = ids
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect::<HashSet<_>>();
    if ids.is_empty() {
        return Err("请选择要检测的预览节点".to_string());
    }
    Ok(ids)
}

fn parse_preview_check_kind(raw: &str) -> Result<PreviewCheckKind, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "latency" => Ok(PreviewCheckKind::Latency),
        "ip_health" | "ip-health" => Ok(PreviewCheckKind::IpHealth),
        other => Err(format!("不支持的预览检测类型: {}", other)),
    }
}

async fn check_parsed_preview_nodes(
    nodes: Vec<parser::ParsedProxyNode>,
    selected_ids: &HashSet<String>,
    check_kind: PreviewCheckKind,
) -> Result<ProxyImportPreviewCheckResponse, String> {
    let targets = nodes
        .into_iter()
        .filter(|node| selected_ids.contains(&node.preview_id))
        .map(preview_node_to_check_target)
        .collect::<Vec<_>>();

    if targets.is_empty() {
        return Err("所选预览节点已失效，请重新预览后再检测".to_string());
    }

    let items = match check_kind {
        PreviewCheckKind::Latency => check_preview_latency_targets(targets).await,
        PreviewCheckKind::IpHealth => check_preview_ip_health_targets(targets).await,
    };

    Ok(ProxyImportPreviewCheckResponse { items })
}

fn preview_node_to_check_target(node: parser::ParsedProxyNode) -> ProxyCheckTarget {
    ProxyCheckTarget {
        id: node.preview_id,
        name: node.name,
        protocol: node.protocol,
        host: node.host,
        port: node.port,
        username: node.username,
        password: node.password,
        standard_config: node.standard_config,
    }
}

async fn check_preview_latency_targets(
    targets: Vec<ProxyCheckTarget>,
) -> Vec<ProxyImportPreviewCheckItem> {
    let (bridge_targets, direct_targets): (Vec<_>, Vec<_>) =
        targets.into_iter().partition(health::is_bridge_check_target);
    let mut items = stream::iter(direct_targets)
        .map(check_preview_latency_target)
        .buffer_unordered(LATENCY_BATCH_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;

    let mut bridge_items = stream::iter(bridge_targets)
        .map(check_preview_latency_target)
        .buffer_unordered(PREVIEW_BRIDGE_BATCH_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
    items.append(&mut bridge_items);

    items
}

async fn check_preview_ip_health_targets(
    targets: Vec<ProxyCheckTarget>,
) -> Vec<ProxyImportPreviewCheckItem> {
    let (bridge_targets, direct_targets): (Vec<_>, Vec<_>) =
        targets.into_iter().partition(health::is_bridge_check_target);
    let mut items = stream::iter(direct_targets)
        .map(check_preview_ip_health_target)
        .buffer_unordered(IP_HEALTH_BATCH_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;

    let mut bridge_items = stream::iter(bridge_targets)
        .map(check_preview_ip_health_target)
        .buffer_unordered(PREVIEW_BRIDGE_BATCH_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
    items.append(&mut bridge_items);

    items
}

async fn check_preview_latency_target(target: ProxyCheckTarget) -> ProxyImportPreviewCheckItem {
    let preview_id = target.id.clone();
    match tokio::time::timeout(
        PREVIEW_LATENCY_TARGET_TIMEOUT,
        health::test_latency(target),
    )
    .await
    {
        Ok(result) => preview_latency_check_item(result),
        Err(_) => ProxyImportPreviewCheckItem {
            preview_id,
            latency_ms: None,
            latency_status: "timeout".to_string(),
            ip_health: None,
            ip_health_summary: String::new(),
            error: "预览测速超时，请稍后重试或导入后单独检测".to_string(),
        },
    }
}

async fn check_preview_ip_health_target(target: ProxyCheckTarget) -> ProxyImportPreviewCheckItem {
    let preview_id = target.id.clone();
    match tokio::time::timeout(
        PREVIEW_IP_HEALTH_TARGET_TIMEOUT,
        health::check_ip_health(target),
    )
    .await
    {
        Ok(result) => preview_ip_health_check_item(result),
        Err(_) => ProxyImportPreviewCheckItem {
            preview_id,
            latency_ms: None,
            latency_status: String::new(),
            ip_health: None,
            ip_health_summary: "IP 健康检测超时，请稍后重试或导入后单独检测".to_string(),
            error: "IP 健康检测超时，请稍后重试或导入后单独检测".to_string(),
        },
    }
}

fn preview_latency_check_item(result: ProxyPoolLatencyTestResult) -> ProxyImportPreviewCheckItem {
    let latency_status = latency_status_for_result(&result);
    let error = result.error;
    ProxyImportPreviewCheckItem {
        preview_id: result.node_id,
        latency_ms: result.latency_ms,
        latency_status,
        ip_health: None,
        ip_health_summary: String::new(),
        error,
    }
}

fn preview_ip_health_check_item(result: ProxyPoolIpHealthResult) -> ProxyImportPreviewCheckItem {
    let ip_health_summary = health::summarize_ip_health(&result);
    let error = result.error.clone();
    ProxyImportPreviewCheckItem {
        preview_id: result.node_id.clone(),
        latency_ms: None,
        latency_status: String::new(),
        ip_health: Some(result),
        ip_health_summary,
        error,
    }
}

pub async fn apply_subscription(
    request: ProxySubscriptionApplyRequest,
) -> Result<ProxySubscriptionApplyResponse, String> {
    if request.selected_preview_ids.is_empty() {
        return Err("请选择要导入的订阅节点".to_string());
    }

    let selected_ids: HashSet<String> = request.selected_preview_ids.into_iter().collect();
    let fetched = subscription::fetch_subscription(&request.url).await?;
    let source_id = subscription_source_id(&fetched.url)?;
    let source_name = build_source_display_name(&fetched.url);
    let group = normalize_optional_text(request.group.clone(), 80)?;
    let group = if group.is_empty() {
        DEFAULT_GROUP.to_string()
    } else {
        group
    };
    let name_prefix = normalize_optional_text(request.name_prefix.clone(), 80)?;

    let preview_request = ProxyImportPreviewRequest {
        content: fetched.content,
        group: Some(group.clone()),
        name_prefix: Some(name_prefix.clone()),
    };
    let parsed = parser::parse_import_request(&preview_request)?;
    let selected_nodes: Vec<_> = parsed
        .nodes
        .into_iter()
        .filter(|node| selected_ids.contains(&node.preview_id))
        .collect();
    if selected_nodes.is_empty() {
        return Err("所选订阅节点已失效，请重新预览后再导入".to_string());
    }

    let db_path = proxy_pool_db_path()?;
    let mut conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;

    let now = now_iso();
    let mut sort_order = next_sort_order(&conn)?;
    let tx = conn
        .transaction()
        .map_err(|err| format!("导入订阅节点失败: {}", err))?;
    upsert_subscription_source(
        &tx,
        SubscriptionSourceUpsert {
            id: &source_id,
            url: &fetched.url,
            name_prefix: &name_prefix,
            group: &group,
            now: &now,
        },
    )?;
    tx.execute(
        "DELETE FROM proxy_nodes WHERE source_id = ?1 AND builtin = 0",
        params![&source_id],
    )
    .map_err(|err| format!("刷新订阅节点失败: {}", err))?;

    let source_meta = InsertSourceMeta {
        id: source_id.clone(),
        name: source_name,
    };
    let mut imported = 0usize;
    for node in &selected_nodes {
        insert_imported_node(&tx, node, sort_order, &now, Some(&source_meta))?;
        sort_order += 1;
        imported += 1;
    }
    reset_current_node_if_missing_in_tx(&tx, &now)?;
    tx.commit()
        .map_err(|err| format!("导入订阅节点失败: {}", err))?;

    let list = list_nodes()?;
    let source = list
        .sources
        .iter()
        .find(|item| item.id == source_meta.id.as_str())
        .cloned()
        .ok_or_else(|| "订阅来源保存后读取失败".to_string())?;

    Ok(ProxySubscriptionApplyResponse {
        imported,
        skipped: selected_ids.len().saturating_sub(imported),
        nodes: list.nodes,
        source,
    })
}

pub async fn refresh_subscription(
    request: ProxySubscriptionRefreshRequest,
) -> Result<ProxySubscriptionRefreshResponse, String> {
    let source_id = normalize_required_text(&request.source_id, "订阅来源", 120)?;
    let db_path = proxy_pool_db_path()?;
    let conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;
    let Some(source) = load_subscription_source_record(&conn, &source_id)? else {
        return Err("订阅来源不存在".to_string());
    };
    drop(conn);

    let results = vec![refresh_subscription_record(source).await];
    build_subscription_refresh_response(results)
}

pub async fn refresh_all_subscriptions() -> Result<ProxySubscriptionRefreshResponse, String> {
    let db_path = proxy_pool_db_path()?;
    let conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;
    let sources = list_subscription_source_records(&conn)?;
    drop(conn);

    let mut results = Vec::with_capacity(sources.len());
    for source in sources {
        results.push(refresh_subscription_record(source).await);
    }

    build_subscription_refresh_response(results)
}

pub fn update_subscription_source(
    request: ProxySourceUpdateRequest,
) -> Result<ProxyPoolListResponse, String> {
    let source_id = normalize_required_text(&request.source_id, "订阅来源", 120)?;
    let url = subscription::normalize_subscription_url(&request.url)?;
    let group = normalize_optional_text(request.group, 80)?;
    let group = if group.is_empty() {
        DEFAULT_GROUP.to_string()
    } else {
        group
    };
    let name_prefix = normalize_optional_text(request.name_prefix, 80)?;
    let dns = normalize_optional_text(request.dns, 240)?;

    let db_path = proxy_pool_db_path()?;
    let mut conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;

    let Some(existing) = load_subscription_source_record(&conn, &source_id)? else {
        return Err("订阅来源不存在".to_string());
    };

    let now = now_iso();
    let source_name = build_source_display_name(&url);
    let tx = conn
        .transaction()
        .map_err(|err| format!("更新订阅来源失败: {}", err))?;
    tx.execute(
        "UPDATE proxy_sources
         SET url = ?1, name_prefix = ?2, group_name = ?3, dns = ?4,
             last_error = CASE WHEN url <> ?1 THEN '' ELSE last_error END,
             updated_at = ?5
         WHERE id = ?6",
        params![&url, &name_prefix, &group, &dns, &now, &source_id],
    )
    .map_err(|err| format!("更新订阅来源失败: {}", err))?;
    tx.execute(
        "UPDATE proxy_nodes
         SET group_name = ?1, source_name = ?2, updated_at = ?3
         WHERE source_id = ?4 AND builtin = 0",
        params![&group, &source_name, &now, &source_id],
    )
    .map_err(|err| format!("更新订阅来源节点失败: {}", err))?;
    tx.commit()
        .map_err(|err| format!("更新订阅来源失败: {}", err))?;

    if existing.url != url {
        tracing::info!(
            "[ProxyPool] subscription source URL updated: source_id={}, old={}, new={}",
            source_id,
            existing.url,
            url
        );
    }

    list_nodes()
}

pub fn delete_subscription_source(source_id: &str) -> Result<ProxyPoolListResponse, String> {
    let source_id = normalize_required_text(source_id, "订阅来源", 120)?;
    let db_path = proxy_pool_db_path()?;
    let mut conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;

    if load_subscription_source_record(&conn, &source_id)?.is_none() {
        return Err("订阅来源不存在".to_string());
    }

    let tx = conn
        .transaction()
        .map_err(|err| format!("删除订阅来源失败: {}", err))?;
    tx.execute(
        "DELETE FROM proxy_nodes WHERE source_id = ?1 AND builtin = 0",
        params![&source_id],
    )
    .map_err(|err| format!("删除订阅来源节点失败: {}", err))?;
    tx.execute("DELETE FROM proxy_sources WHERE id = ?1", params![&source_id])
        .map_err(|err| format!("删除订阅来源失败: {}", err))?;
    reset_current_node_if_missing_in_tx(&tx, &now_iso())?;
    tx.commit()
        .map_err(|err| format!("删除订阅来源失败: {}", err))?;

    list_nodes()
}

pub async fn test_node_latency(node_id: &str) -> Result<ProxyPoolLatencyTestResponse, String> {
    let target = load_check_target_by_id(node_id)?;
    let results = vec![health::test_latency(target).await];
    persist_latency_results(&results)?;
    build_latency_response(results)
}

pub async fn test_all_latency() -> Result<ProxyPoolLatencyTestResponse, String> {
    test_all_latency_with_progress(None, None).await
}

pub async fn test_all_latency_with_progress(
    task_id: Option<String>,
    progress: Option<ProxyPoolProgressEmitter>,
) -> Result<ProxyPoolLatencyTestResponse, String> {
    let targets = load_enabled_check_targets()?;
    let total = targets.len();
    emit_progress(
        progress.as_ref(),
        build_progress_event(
            task_id.as_deref(),
            "latency",
            "started",
            "",
            None,
            0,
            total,
        ),
    );
    let (bridge_targets, direct_targets): (Vec<_>, Vec<_>) =
        targets.into_iter().partition(health::is_bridge_check_target);
    let mut completed = 0usize;
    let mut results = Vec::with_capacity(total);
    let mut direct_stream = stream::iter(direct_targets)
        .map(health::test_latency)
        .buffer_unordered(LATENCY_BATCH_CONCURRENCY);
    while let Some(result) = direct_stream.next().await {
        persist_latency_results(std::slice::from_ref(&result))?;
        completed += 1;
        emit_progress(
            progress.as_ref(),
            build_progress_event(
                task_id.as_deref(),
                "latency",
                "node_done",
                result.node_id.as_str(),
                Some(ProxyPoolProgressResult::Latency(result.clone())),
                completed,
                total,
            ),
        );
        results.push(result);
    }
    for target in bridge_targets {
        let result = health::test_latency(target).await;
        persist_latency_results(std::slice::from_ref(&result))?;
        completed += 1;
        emit_progress(
            progress.as_ref(),
            build_progress_event(
                task_id.as_deref(),
                "latency",
                "node_done",
                result.node_id.as_str(),
                Some(ProxyPoolProgressResult::Latency(result.clone())),
                completed,
                total,
            ),
        );
        results.push(result);
    }
    emit_progress(
        progress.as_ref(),
        build_progress_event(
            task_id.as_deref(),
            "latency",
            "finished",
            "",
            None,
            completed,
            total,
        ),
    );
    build_latency_response(results)
}

pub async fn check_node_ip_health(node_id: &str) -> Result<ProxyPoolIpHealthResponse, String> {
    let target = load_check_target_by_id(node_id)?;
    let results = vec![health::check_ip_health(target).await];
    persist_ip_health_results(&results)?;
    build_ip_health_response(results)
}

pub async fn check_all_ip_health() -> Result<ProxyPoolIpHealthResponse, String> {
    check_all_ip_health_with_progress(None, None).await
}

pub async fn check_all_ip_health_with_progress(
    task_id: Option<String>,
    progress: Option<ProxyPoolProgressEmitter>,
) -> Result<ProxyPoolIpHealthResponse, String> {
    let targets = load_enabled_check_targets()?;
    let total = targets.len();
    emit_progress(
        progress.as_ref(),
        build_progress_event(
            task_id.as_deref(),
            "ip_health",
            "started",
            "",
            None,
            0,
            total,
        ),
    );
    let (bridge_targets, direct_targets): (Vec<_>, Vec<_>) =
        targets.into_iter().partition(health::is_bridge_check_target);
    let mut completed = 0usize;
    let mut results = Vec::with_capacity(total);
    let mut direct_stream = stream::iter(direct_targets)
        .map(health::check_ip_health)
        .buffer_unordered(IP_HEALTH_BATCH_CONCURRENCY);
    while let Some(result) = direct_stream.next().await {
        persist_ip_health_results(std::slice::from_ref(&result))?;
        completed += 1;
        emit_progress(
            progress.as_ref(),
            build_progress_event(
                task_id.as_deref(),
                "ip_health",
                "node_done",
                result.node_id.as_str(),
                Some(ProxyPoolProgressResult::IpHealth(result.clone())),
                completed,
                total,
            ),
        );
        results.push(result);
    }
    for target in bridge_targets {
        let result = health::check_ip_health(target).await;
        persist_ip_health_results(std::slice::from_ref(&result))?;
        completed += 1;
        emit_progress(
            progress.as_ref(),
            build_progress_event(
                task_id.as_deref(),
                "ip_health",
                "node_done",
                result.node_id.as_str(),
                Some(ProxyPoolProgressResult::IpHealth(result.clone())),
                completed,
                total,
            ),
        );
        results.push(result);
    }
    emit_progress(
        progress.as_ref(),
        build_progress_event(
            task_id.as_deref(),
            "ip_health",
            "finished",
            "",
            None,
            completed,
            total,
        ),
    );
    build_ip_health_response(results)
}

enum ProxyPoolProgressResult {
    Latency(ProxyPoolLatencyTestResult),
    IpHealth(ProxyPoolIpHealthResult),
}

fn emit_progress(progress: Option<&ProxyPoolProgressEmitter>, event: ProxyPoolCheckProgressEvent) {
    if let Some(progress) = progress {
        progress(event);
    }
}

fn build_progress_event(
    task_id: Option<&str>,
    kind: &str,
    phase: &str,
    node_id: &str,
    result: Option<ProxyPoolProgressResult>,
    completed: usize,
    total: usize,
) -> ProxyPoolCheckProgressEvent {
    let mut event = ProxyPoolCheckProgressEvent {
        task_id: task_id.unwrap_or_default().to_string(),
        kind: kind.to_string(),
        phase: phase.to_string(),
        node_id: node_id.to_string(),
        ok: None,
        latency_ms: None,
        latency_status: String::new(),
        ip_health: None,
        ip_health_summary: String::new(),
        error: String::new(),
        completed,
        total,
    };

    match result {
        Some(ProxyPoolProgressResult::Latency(result)) => {
            event.ok = Some(result.ok);
            event.latency_ms = result.latency_ms;
            event.latency_status = latency_status_for_result(&result);
            event.error = result.error;
        }
        Some(ProxyPoolProgressResult::IpHealth(result)) => {
            event.ok = Some(result.ok);
            event.ip_health_summary = health::summarize_ip_health(&result);
            event.error = result.error.clone();
            event.ip_health = Some(result);
        }
        None => {}
    }

    event
}

fn latency_status_for_result(result: &ProxyPoolLatencyTestResult) -> String {
    if result.ok {
        "ok".to_string()
    } else {
        truncate_refresh_error(&result.error)
    }
}

pub fn proxy_pool_db_path() -> Result<PathBuf, String> {
    let dir = data_dir::get_data_dir()?.join(DB_DIR_NAME);
    fs::create_dir_all(&dir)
        .map_err(|err| format!("创建代理池数据目录失败 {}: {}", dir.display(), err))?;
    Ok(dir.join(DB_FILE_NAME))
}

fn open_connection_at(db_path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(db_path)
        .map_err(|err| format!("打开代理池数据库失败 {}: {}", db_path.display(), err))?;
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
            outlet_mode TEXT NOT NULL DEFAULT 'direct',
            selected_node_ids_json TEXT NOT NULL DEFAULT '[]',
            global_proxy_mode TEXT NOT NULL DEFAULT 'manual',
            updated_at TEXT NOT NULL
        );
        ",
    )
    .map_err(|err| format!("初始化代理池数据库失败: {}", err))?;

    ensure_proxy_service_state_columns(conn)?;

    conn.execute(
        "INSERT OR IGNORE INTO schema_migrations (version, applied_at) VALUES (1, ?1)",
        params![now_iso()],
    )
    .map_err(|err| format!("写入代理池数据库迁移记录失败: {}", err))?;

    Ok(())
}

fn ensure_proxy_service_state_columns(conn: &Connection) -> Result<(), String> {
    if !table_has_column(conn, "proxy_service_state", "outlet_mode")? {
        conn.execute(
            "ALTER TABLE proxy_service_state ADD COLUMN outlet_mode TEXT NOT NULL DEFAULT 'direct'",
            [],
        )
        .map_err(|err| format!("迁移代理池出口模式失败: {}", err))?;
    }
    if !table_has_column(conn, "proxy_service_state", "selected_node_ids_json")? {
        conn.execute(
            "ALTER TABLE proxy_service_state ADD COLUMN selected_node_ids_json TEXT NOT NULL DEFAULT '[]'",
            [],
        )
        .map_err(|err| format!("迁移代理池节点池选择失败: {}", err))?;
    }
    Ok(())
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|err| format!("读取代理池数据库结构失败: {}", err))?;
    let mut rows = stmt
        .query([])
        .map_err(|err| format!("读取代理池数据库结构失败: {}", err))?;
    while let Some(row) = rows
        .next()
        .map_err(|err| format!("读取代理池数据库结构失败: {}", err))?
    {
        let name: String = row
            .get(1)
            .map_err(|err| format!("读取代理池数据库结构失败: {}", err))?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
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
            port: DEFAULT_LOCAL_PROXY_PORT,
            enabled: false,
            sort_order: 1,
            now: &now,
        },
    )?;

    conn.execute(
        "INSERT OR IGNORE INTO proxy_service_state (
            id, enabled, preferred_port, actual_port, current_node_id, outlet_mode,
            selected_node_ids_json, global_proxy_mode, updated_at
         ) VALUES (1, 0, ?1, NULL, ?2, 'direct', '[]', 'manual', ?3)",
        params![DEFAULT_PROXY_GATEWAY_PORT, DIRECT_NODE_ID, now],
    )
    .map_err(|err| format!("初始化代理池服务状态失败: {}", err))?;

    normalize_service_state_in_conn(conn, &now)?;

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
            latency_status, ip_health_json, ip_health_summary, created_at, updated_at
         FROM proxy_nodes
         WHERE id = ?1",
        params![id],
        map_node_row,
    )
    .optional()
    .map_err(|err| format!("读取代理节点失败: {}", err))
}

pub fn get_service_state() -> Result<ProxyPoolServiceState, String> {
    let db_path = proxy_pool_db_path()?;
    let conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;
    service_state_from_conn(&conn)
}

pub fn update_service_state(
    request: ProxyPoolServiceUpdateRequest,
) -> Result<ProxyPoolListResponse, String> {
    update_service_state_config(request)?;
    list_nodes()
}

pub fn update_service_state_config(
    request: ProxyPoolServiceUpdateRequest,
) -> Result<ProxyPoolServiceState, String> {
    let db_path = proxy_pool_db_path()?;
    let mut conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;
    let current_state = service_state_from_conn(&conn)?;
    let next_preferred_port = request
        .preferred_port
        .unwrap_or(current_state.preferred_port);
    let next_local_proxy_port = request
        .local_proxy_port
        .unwrap_or(current_state.local_proxy_port);
    if next_preferred_port == next_local_proxy_port {
        return Err("内置代理网关端口不能和外部本地代理端口相同".to_string());
    }

    let tx = conn
        .transaction()
        .map_err(|err| format!("更新内置代理网关配置失败: {}", err))?;
    let now = now_iso();

    if let Some(enabled) = request.enabled {
        tx.execute(
            "UPDATE proxy_service_state SET enabled = ?1, global_proxy_mode = 'proxy_pool', updated_at = ?2 WHERE id = 1",
            params![if enabled { 1 } else { 0 }, &now],
        )
        .map_err(|err| format!("更新内置代理网关开关失败: {}", err))?;
    }

    if let Some(port) = request.preferred_port {
        validate_port(port, "内置代理网关端口")?;
        tx.execute(
            "UPDATE proxy_service_state SET preferred_port = ?1, updated_at = ?2 WHERE id = 1",
            params![port, &now],
        )
        .map_err(|err| format!("更新内置代理网关端口失败: {}", err))?;
    }

    if let Some(port) = request.local_proxy_port {
        validate_port(port, "本地代理端口")?;
        update_local_proxy_node_in_tx(&tx, port, &now)?;
    }

    let has_selection_update = request.outlet_mode.is_some()
        || request.selected_node_ids.is_some()
        || request.current_node_id.is_some();
    if has_selection_update {
        let requested_selected_ids = request.selected_node_ids.is_some();
        let mut next_mode = request
            .outlet_mode
            .as_deref()
            .map(normalize_outlet_mode)
            .transpose()?
            .unwrap_or_else(|| current_state.outlet_mode.clone());
        let mut next_selected = match request.selected_node_ids {
            Some(ids) => normalize_requested_selected_node_ids(&*tx, ids)?,
            None => current_state.selected_node_ids.clone(),
        };
        let mut next_current = current_state.current_node_id.clone();

        if let Some(raw_node_id) = request.current_node_id {
            let node_id = normalize_required_text(&raw_node_id, "出口节点", 120)?;
            ensure_node_exists_for_service_state(&tx, &node_id)?;
            if request.outlet_mode.is_none() {
                next_mode = infer_outlet_mode_from_node_id(&node_id);
            }
            if next_mode == OUTLET_MODE_NODE_POOL
                && !matches!(load_node_builtin(&*tx, &node_id)?, Some(false))
            {
                return Err("节点池模式只能把普通代理节点设为当前出口".to_string());
            }
            if next_mode == OUTLET_MODE_NODE_POOL && !next_selected.iter().any(|id| id == &node_id)
            {
                next_selected.push(node_id.clone());
            }
            next_current = node_id;
        } else if request.outlet_mode.is_none()
            && requested_selected_ids
            && !next_selected.is_empty()
        {
            next_mode = OUTLET_MODE_NODE_POOL.to_string();
        }

        if next_mode == OUTLET_MODE_NODE_POOL && next_selected.is_empty() {
            return Err("节点池模式至少需要选择一个代理节点".to_string());
        }

        let selection = resolve_service_selection(&*tx, next_mode, next_current, next_selected)?;
        persist_service_selection(&*tx, &selection, &now)?;
    } else {
        normalize_service_state_in_conn(&*tx, &now)?;
    }

    tx.commit()
        .map_err(|err| format!("更新内置代理网关配置失败: {}", err))?;

    let conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;
    service_state_from_conn(&conn)
}

fn service_state_from_conn(conn: &Connection) -> Result<ProxyPoolServiceState, String> {
    let (enabled_raw, preferred_port_raw, actual_port_raw): (
        i64,
        i64,
        Option<i64>,
    ) = conn
        .query_row(
            "SELECT enabled, preferred_port, actual_port FROM proxy_service_state WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|err| format!("读取内置代理网关状态失败: {}", err))?;

    let preferred_port = u16::try_from(preferred_port_raw).unwrap_or(DEFAULT_PROXY_GATEWAY_PORT);
    let actual_port = actual_port_raw.and_then(|port| u16::try_from(port).ok());
    let selection = load_service_selection_from_conn(conn)?;
    let current_node = get_node(conn, &selection.current_node_id)?
        .or_else(|| get_node(conn, DIRECT_NODE_ID).ok().flatten())
        .ok_or_else(|| "读取当前出口节点失败".to_string())?;
    let local_proxy_port = get_node(conn, LOCAL_NODE_ID)?
        .map(|node| node.port)
        .unwrap_or(DEFAULT_LOCAL_PROXY_PORT);

    Ok(ProxyPoolServiceState {
        enabled: enabled_raw != 0,
        preferred_port,
        actual_port,
        gateway_url: proxy_gateway_url(preferred_port),
        outlet_mode: selection.outlet_mode,
        selected_node_ids: selection.selected_node_ids,
        current_node_id: current_node.id,
        current_node_name: current_node.name,
        current_node_protocol: current_node.protocol,
        local_proxy_port,
    })
}

fn ensure_node_exists_for_service_state(tx: &Transaction<'_>, node_id: &str) -> Result<(), String> {
    let exists: Option<i64> = tx
        .query_row(
            "SELECT 1 FROM proxy_nodes WHERE id = ?1",
            params![node_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|err| format!("读取内置代理出口节点失败: {}", err))?;
    if exists.is_none() {
        return Err("出口节点不存在".to_string());
    }
    Ok(())
}

fn update_local_proxy_node_in_tx(
    tx: &Transaction<'_>,
    port: u16,
    now: &str,
) -> Result<(), String> {
    let name = format!("本地代理 127.0.0.1:{port}");
    let standard_config = build_standard_config_json("http", "127.0.0.1", port, "", "")?;
    tx.execute(
        "UPDATE proxy_nodes
         SET name = ?1, host = '127.0.0.1', port = ?2, raw_config = ?3, standard_config = ?3, updated_at = ?4
         WHERE id = ?5",
        params![name, port, standard_config, now, LOCAL_NODE_ID],
    )
    .map_err(|err| format!("更新本地代理端口失败: {}", err))?;
    Ok(())
}

#[derive(Debug, Clone)]
struct ServiceOutletSelection {
    outlet_mode: String,
    current_node_id: String,
    selected_node_ids: Vec<String>,
}

fn normalize_outlet_mode(value: &str) -> Result<String, String> {
    match value.trim() {
        OUTLET_MODE_DIRECT => Ok(OUTLET_MODE_DIRECT.to_string()),
        OUTLET_MODE_LOCAL => Ok(OUTLET_MODE_LOCAL.to_string()),
        OUTLET_MODE_NODE_POOL => Ok(OUTLET_MODE_NODE_POOL.to_string()),
        _ => Err("出口模式必须是 direct、local 或 node_pool".to_string()),
    }
}

fn normalize_outlet_mode_or_direct(value: &str) -> String {
    normalize_outlet_mode(value).unwrap_or_else(|_| OUTLET_MODE_DIRECT.to_string())
}

fn infer_outlet_mode_from_node_id(node_id: &str) -> String {
    match node_id {
        DIRECT_NODE_ID => OUTLET_MODE_DIRECT.to_string(),
        LOCAL_NODE_ID => OUTLET_MODE_LOCAL.to_string(),
        _ => OUTLET_MODE_NODE_POOL.to_string(),
    }
}

fn parse_selected_node_ids(value: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(value).unwrap_or_default()
}

fn selected_node_ids_json(ids: &[String]) -> Result<String, String> {
    serde_json::to_string(ids).map_err(|err| format!("保存节点池选择失败: {}", err))
}

fn load_node_builtin(conn: &Connection, node_id: &str) -> Result<Option<bool>, String> {
    conn.query_row(
        "SELECT builtin FROM proxy_nodes WHERE id = ?1",
        params![node_id],
        |row| Ok(row.get::<_, i64>(0)? != 0),
    )
    .optional()
    .map_err(|err| format!("读取代理节点状态失败: {}", err))
}

fn filter_existing_normal_node_ids(
    conn: &Connection,
    ids: Vec<String>,
) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut filtered = Vec::new();
    for id in ids {
        let trimmed = id.trim();
        if trimmed.is_empty() || !seen.insert(trimmed.to_string()) {
            continue;
        }
        if matches!(load_node_builtin(conn, trimmed)?, Some(false)) {
            filtered.push(trimmed.to_string());
        }
    }
    Ok(filtered)
}

fn normalize_requested_selected_node_ids(
    conn: &Connection,
    ids: Vec<String>,
) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for raw_id in ids {
        let id = normalize_required_text(&raw_id, "节点池出口节点", 120)?;
        if !seen.insert(id.clone()) {
            continue;
        }
        match load_node_builtin(conn, &id)? {
            Some(false) => normalized.push(id),
            Some(true) => return Err("节点池模式只能选择普通代理节点".to_string()),
            None => return Err("节点池出口节点不存在".to_string()),
        }
    }
    Ok(normalized)
}

fn resolve_service_selection(
    conn: &Connection,
    outlet_mode: String,
    current_node_id: String,
    selected_node_ids: Vec<String>,
) -> Result<ServiceOutletSelection, String> {
    let mut mode = normalize_outlet_mode_or_direct(&outlet_mode);
    let mut selected = filter_existing_normal_node_ids(conn, selected_node_ids)?;
    let current_id = current_node_id.trim().to_string();

    if mode == OUTLET_MODE_DIRECT && current_id != DIRECT_NODE_ID {
        mode = infer_outlet_mode_from_node_id(&current_id);
    }

    if mode == OUTLET_MODE_NODE_POOL {
        if selected.is_empty() && matches!(load_node_builtin(conn, &current_id)?, Some(false)) {
            selected.push(current_id.clone());
        }
        if selected.is_empty() {
            mode = OUTLET_MODE_DIRECT.to_string();
        }
    }

    if mode == OUTLET_MODE_DIRECT {
        return Ok(ServiceOutletSelection {
            outlet_mode: OUTLET_MODE_DIRECT.to_string(),
            current_node_id: DIRECT_NODE_ID.to_string(),
            selected_node_ids: Vec::new(),
        });
    }

    if mode == OUTLET_MODE_LOCAL {
        return Ok(ServiceOutletSelection {
            outlet_mode: OUTLET_MODE_LOCAL.to_string(),
            current_node_id: LOCAL_NODE_ID.to_string(),
            selected_node_ids: Vec::new(),
        });
    }

    let current_node_id = if selected.iter().any(|id| id == &current_id) {
        current_id
    } else {
        selected
            .first()
            .cloned()
            .ok_or_else(|| "节点池模式至少需要选择一个代理节点".to_string())?
    };

    Ok(ServiceOutletSelection {
        outlet_mode: OUTLET_MODE_NODE_POOL.to_string(),
        current_node_id,
        selected_node_ids: selected,
    })
}

fn load_service_selection_from_conn(conn: &Connection) -> Result<ServiceOutletSelection, String> {
    let (outlet_mode, current_node_id, selected_node_ids_json): (String, String, String) = conn
        .query_row(
            "SELECT outlet_mode, current_node_id, selected_node_ids_json FROM proxy_service_state WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|err| format!("读取内置代理出口模式失败: {}", err))?;

    resolve_service_selection(
        conn,
        outlet_mode,
        current_node_id,
        parse_selected_node_ids(&selected_node_ids_json),
    )
}

fn apply_node_enabled_flags(
    conn: &Connection,
    selection: &ServiceOutletSelection,
    now: &str,
) -> Result<(), String> {
    if selection.outlet_mode == OUTLET_MODE_NODE_POOL {
        conn.execute(
            "UPDATE proxy_nodes SET enabled = 0, updated_at = ?1 WHERE enabled != 0",
            params![now],
        )
        .map_err(|err| format!("停用非节点池出口失败: {}", err))?;
        for node_id in &selection.selected_node_ids {
            conn.execute(
                "UPDATE proxy_nodes SET enabled = 1, updated_at = ?1 WHERE id = ?2 AND enabled != 1",
                params![now, node_id],
            )
            .map_err(|err| format!("启用节点池出口失败: {}", err))?;
        }
        return Ok(());
    }

    let active_id = if selection.outlet_mode == OUTLET_MODE_LOCAL {
        LOCAL_NODE_ID
    } else {
        DIRECT_NODE_ID
    };
    conn.execute(
        "UPDATE proxy_nodes
         SET enabled = CASE WHEN id = ?1 THEN 1 ELSE 0 END, updated_at = ?2
         WHERE enabled != CASE WHEN id = ?1 THEN 1 ELSE 0 END",
        params![active_id, now],
    )
    .map_err(|err| format!("同步出口节点启用状态失败: {}", err))?;
    Ok(())
}

fn persist_service_selection(
    conn: &Connection,
    selection: &ServiceOutletSelection,
    now: &str,
) -> Result<(), String> {
    let selected_json = selected_node_ids_json(&selection.selected_node_ids)?;
    conn.execute(
        "UPDATE proxy_service_state
         SET outlet_mode = ?1, current_node_id = ?2, selected_node_ids_json = ?3, updated_at = ?4
         WHERE id = 1",
        params![
            &selection.outlet_mode,
            &selection.current_node_id,
            selected_json,
            now,
        ],
    )
    .map_err(|err| format!("保存内置代理出口模式失败: {}", err))?;
    apply_node_enabled_flags(conn, selection, now)
}

fn normalize_service_state_in_conn(conn: &Connection, now: &str) -> Result<(), String> {
    let selection = load_service_selection_from_conn(conn)?;
    persist_service_selection(conn, &selection, now)
}

#[derive(Debug, Clone)]
pub struct GatewayOutboundTarget {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub gateway_port: u16,
    pub standard_config: Value,
}

pub fn load_current_gateway_outbound() -> Result<GatewayOutboundTarget, String> {
    let db_path = proxy_pool_db_path()?;
    let conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;
    let service_state = service_state_from_conn(&conn)?;
    let mut target =
        load_gateway_outbound_by_id(&conn, &service_state.current_node_id)?
            .or_else(|| load_gateway_outbound_by_id(&conn, DIRECT_NODE_ID).ok().flatten())
            .ok_or_else(|| "读取内置代理网关出口节点失败".to_string())?;
    target.gateway_port = service_state.preferred_port;
    if target.id != DIRECT_NODE_ID {
        let enabled: Option<i64> = conn
            .query_row(
                "SELECT enabled FROM proxy_nodes WHERE id = ?1",
                params![&target.id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|err| format!("读取内置代理网关出口节点状态失败: {}", err))?;
        if enabled.unwrap_or(0) == 0 {
            target = load_gateway_outbound_by_id(&conn, DIRECT_NODE_ID)?
                .ok_or_else(|| "读取内置直连节点失败".to_string())?;
            target.gateway_port = service_state.preferred_port;
        }
    }
    Ok(target)
}

pub fn load_gateway_outbound_candidates() -> Result<Vec<GatewayOutboundTarget>, String> {
    let db_path = proxy_pool_db_path()?;
    let conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;
    let service_state = service_state_from_conn(&conn)?;

    if service_state.outlet_mode != OUTLET_MODE_NODE_POOL {
        return load_current_gateway_outbound().map(|target| vec![target]);
    }

    let mut candidate_ids = Vec::new();
    if !service_state.current_node_id.trim().is_empty() {
        candidate_ids.push(service_state.current_node_id.clone());
    }
    for node_id in &service_state.selected_node_ids {
        if !candidate_ids.iter().any(|candidate_id| candidate_id == node_id) {
            candidate_ids.push(node_id.clone());
        }
    }

    let mut targets = Vec::new();
    for node_id in candidate_ids {
        if let Some(mut target) = load_gateway_outbound_by_id(&conn, &node_id)? {
            target.gateway_port = service_state.preferred_port;
            targets.push(target);
        }
    }

    if targets.is_empty() {
        return load_gateway_outbound_by_id(&conn, DIRECT_NODE_ID)?
            .map(|mut target| {
                target.gateway_port = service_state.preferred_port;
                vec![target]
            })
            .ok_or_else(|| "读取内置直连节点失败".to_string());
    }

    Ok(targets)
}

fn load_gateway_outbound_by_id(
    conn: &Connection,
    id: &str,
) -> Result<Option<GatewayOutboundTarget>, String> {
    conn.query_row(
        "SELECT id, name, protocol, host, port, username, password, standard_config
         FROM proxy_nodes
         WHERE id = ?1",
        params![id],
        |row| {
            let port_raw: i64 = row.get(4)?;
            let standard_config_text: String = row.get(7)?;
            let standard_config =
                serde_json::from_str(&standard_config_text).unwrap_or_else(|_| json!({}));
            Ok(GatewayOutboundTarget {
                id: row.get(0)?,
                name: row.get(1)?,
                protocol: row.get(2)?,
                host: row.get(3)?,
                port: u16::try_from(port_raw).unwrap_or(0),
                username: row.get(5)?,
                password: row.get(6)?,
                gateway_port: DEFAULT_PROXY_GATEWAY_PORT,
                standard_config,
            })
        },
    )
    .optional()
    .map_err(|err| format!("读取内置代理网关出口节点失败: {}", err))
}

pub fn update_service_actual_port(actual_port: Option<u16>) -> Result<(), String> {
    let db_path = proxy_pool_db_path()?;
    let conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;
    conn.execute(
        "UPDATE proxy_service_state SET actual_port = ?1, updated_at = ?2 WHERE id = 1",
        params![actual_port.map(i64::from), now_iso()],
    )
    .map_err(|err| format!("更新内置代理网关实际端口失败: {}", err))?;
    Ok(())
}

pub fn promote_gateway_outbound_after_failover(
    previous_current_node_id: &str,
    successful_node_id: &str,
) -> Result<Option<ProxyPoolServiceState>, String> {
    let previous_current_node_id =
        normalize_required_text(previous_current_node_id, "原当前出口节点", 120)?;
    let successful_node_id = normalize_required_text(successful_node_id, "备用出口节点", 120)?;
    if previous_current_node_id == successful_node_id {
        return Ok(None);
    }

    let db_path = proxy_pool_db_path()?;
    let mut conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;
    let now = now_iso();

    let tx = conn
        .transaction()
        .map_err(|err| format!("持久化自动故障切换失败: {}", err))?;
    let mut selection = load_service_selection_from_conn(&*tx)?;
    if selection.outlet_mode != OUTLET_MODE_NODE_POOL {
        return Ok(None);
    }
    if selection.current_node_id != previous_current_node_id {
        return Ok(None);
    }
    if !selection
        .selected_node_ids
        .iter()
        .any(|id| id == &successful_node_id)
    {
        return Ok(None);
    }
    if !matches!(load_node_builtin(&*tx, &successful_node_id)?, Some(false)) {
        return Ok(None);
    }

    selection.current_node_id = successful_node_id;
    persist_service_selection(&*tx, &selection, &now)?;
    tx.commit()
        .map_err(|err| format!("持久化自动故障切换失败: {}", err))?;

    let conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;
    service_state_from_conn(&conn).map(Some)
}

fn reset_current_node_if_missing_in_tx(tx: &Transaction<'_>, now: &str) -> Result<(), String> {
    normalize_service_state_in_conn(&*tx, now).map_err(|err| format!("恢复当前出口节点失败: {}", err))
}

pub fn proxy_gateway_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

fn validate_port(port: u16, label: &str) -> Result<(), String> {
    if port == 0 {
        return Err(format!("{}必须在 1-65535 之间", label));
    }
    Ok(())
}

fn load_check_target_by_id(node_id: &str) -> Result<ProxyCheckTarget, String> {
    let node_id = normalize_required_text(node_id, "代理节点", 120)?;
    let db_path = proxy_pool_db_path()?;
    let conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;

    conn.query_row(
        "SELECT id, name, protocol, host, port, username, password, standard_config
         FROM proxy_nodes
         WHERE id = ?1",
        params![node_id],
        map_check_target_row,
    )
    .optional()
    .map_err(|err| format!("读取代理节点检测配置失败: {}", err))?
    .ok_or_else(|| "代理节点不存在".to_string())
}

fn load_enabled_check_targets() -> Result<Vec<ProxyCheckTarget>, String> {
    let db_path = proxy_pool_db_path()?;
    let conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;

    let mut stmt = conn
        .prepare(
            "SELECT id, name, protocol, host, port, username, password, standard_config
             FROM proxy_nodes
             WHERE enabled = 1
             ORDER BY builtin DESC, sort_order ASC, created_at ASC",
        )
        .map_err(|err| format!("读取代理节点检测配置失败: {}", err))?;
    let targets = stmt
        .query_map([], map_check_target_row)
        .map_err(|err| format!("读取代理节点检测配置失败: {}", err))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取代理节点检测配置失败: {}", err))?;

    Ok(targets)
}

fn map_check_target_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProxyCheckTarget> {
    let port_raw: i64 = row.get(4)?;
    let standard_config_text: String = row.get(7)?;
    let standard_config = serde_json::from_str(&standard_config_text).unwrap_or_else(|_| json!({}));
    Ok(ProxyCheckTarget {
        id: row.get(0)?,
        name: row.get(1)?,
        protocol: row.get(2)?,
        host: row.get(3)?,
        port: u16::try_from(port_raw).unwrap_or(0),
        username: row.get(5)?,
        password: row.get(6)?,
        standard_config,
    })
}

fn list_sources_from_conn(conn: &Connection) -> Result<Vec<ProxySource>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT
                source.id, source.url, source.name_prefix, source.group_name, source.dns,
                source.auto_refresh_enabled, source.refresh_interval_minutes,
                source.last_refresh_at, source.last_error, source.created_at, source.updated_at,
                COUNT(node.id) AS node_count
             FROM proxy_sources source
             LEFT JOIN proxy_nodes node ON node.source_id = source.id
             GROUP BY source.id
             ORDER BY source.updated_at DESC, source.created_at DESC",
        )
        .map_err(|err| format!("读取订阅来源失败: {}", err))?;

    let sources = stmt
        .query_map([], map_source_row)
        .map_err(|err| format!("读取订阅来源失败: {}", err))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取订阅来源失败: {}", err))?;

    Ok(sources)
}

fn map_source_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProxySource> {
    let url: String = row.get(1)?;
    Ok(ProxySource {
        id: row.get(0)?,
        display_name: build_source_display_name(&url),
        url,
        name_prefix: row.get(2)?,
        group: row.get(3)?,
        dns: row.get(4)?,
        auto_refresh_enabled: row.get::<_, i64>(5)? != 0,
        refresh_interval_minutes: row.get(6)?,
        last_refresh_at: row.get(7)?,
        last_error: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        node_count: row.get(11)?,
    })
}

#[derive(Debug, Clone)]
struct SubscriptionSourceRecord {
    id: String,
    url: String,
    name_prefix: String,
    group: String,
}

fn load_subscription_source_record(
    conn: &Connection,
    source_id: &str,
) -> Result<Option<SubscriptionSourceRecord>, String> {
    conn.query_row(
        "SELECT id, url, name_prefix, group_name FROM proxy_sources WHERE id = ?1",
        params![source_id],
        map_source_record_row,
    )
    .optional()
    .map_err(|err| format!("读取订阅来源失败: {}", err))
}

fn list_subscription_source_records(
    conn: &Connection,
) -> Result<Vec<SubscriptionSourceRecord>, String> {
    let mut stmt = conn
        .prepare("SELECT id, url, name_prefix, group_name FROM proxy_sources ORDER BY updated_at DESC, created_at DESC")
        .map_err(|err| format!("读取订阅来源失败: {}", err))?;

    let sources = stmt
        .query_map([], map_source_record_row)
        .map_err(|err| format!("读取订阅来源失败: {}", err))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取订阅来源失败: {}", err))?;

    Ok(sources)
}

fn map_source_record_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SubscriptionSourceRecord> {
    Ok(SubscriptionSourceRecord {
        id: row.get(0)?,
        url: row.get(1)?,
        name_prefix: row.get(2)?,
        group: row.get(3)?,
    })
}

struct SubscriptionSourceUpsert<'a> {
    id: &'a str,
    url: &'a str,
    name_prefix: &'a str,
    group: &'a str,
    now: &'a str,
}

fn upsert_subscription_source(
    tx: &Transaction<'_>,
    source: SubscriptionSourceUpsert<'_>,
) -> Result<(), String> {
    tx.execute(
        "INSERT INTO proxy_sources (
            id, url, name_prefix, group_name, dns, auto_refresh_enabled,
            refresh_interval_minutes, last_refresh_at, last_error, created_at, updated_at
         ) VALUES (
            ?1, ?2, ?3, ?4, '', 0,
            360, ?5, '', ?5, ?5
         )
         ON CONFLICT(id) DO UPDATE SET
            url = excluded.url,
            name_prefix = excluded.name_prefix,
            group_name = excluded.group_name,
            last_refresh_at = excluded.last_refresh_at,
            last_error = '',
            updated_at = excluded.updated_at",
        params![
            source.id,
            source.url,
            source.name_prefix,
            source.group,
            source.now,
        ],
    )
    .map_err(|err| format!("保存订阅来源失败: {}", err))?;

    Ok(())
}

async fn refresh_subscription_record(
    source: SubscriptionSourceRecord,
) -> ProxySubscriptionRefreshItem {
    let display_name = build_source_display_name(&source.url);
    match fetch_and_parse_subscription_source(&source).await {
        Ok((fetched_url, nodes)) => {
            match replace_subscription_source_nodes(&source, &fetched_url, &nodes) {
                Ok(imported) => ProxySubscriptionRefreshItem {
                    source_id: source.id,
                    display_name,
                    imported,
                    success: true,
                    error: None,
                },
                Err(error) => {
                    let _ = mark_subscription_source_error(&source.id, &error);
                    ProxySubscriptionRefreshItem {
                        source_id: source.id,
                        display_name,
                        imported: 0,
                        success: false,
                        error: Some(error),
                    }
                }
            }
        }
        Err(error) => {
            let _ = mark_subscription_source_error(&source.id, &error);
            ProxySubscriptionRefreshItem {
                source_id: source.id,
                display_name,
                imported: 0,
                success: false,
                error: Some(error),
            }
        }
    }
}

async fn fetch_and_parse_subscription_source(
    source: &SubscriptionSourceRecord,
) -> Result<(String, Vec<parser::ParsedProxyNode>), String> {
    let fetched = subscription::fetch_subscription(&source.url).await?;
    let group = if source.group.trim().is_empty() {
        DEFAULT_GROUP.to_string()
    } else {
        source.group.clone()
    };
    let parsed = parser::parse_import_request(&ProxyImportPreviewRequest {
        content: fetched.content,
        group: Some(group),
        name_prefix: Some(source.name_prefix.clone()),
    })?;
    if parsed.nodes.is_empty() {
        let message = if parsed.errors.is_empty() {
            "订阅中没有可导入的代理节点".to_string()
        } else {
            parsed.errors.join("; ")
        };
        return Err(message);
    }

    Ok((fetched.url, parsed.nodes))
}

fn replace_subscription_source_nodes(
    source: &SubscriptionSourceRecord,
    fetched_url: &str,
    nodes: &[parser::ParsedProxyNode],
) -> Result<usize, String> {
    let db_path = proxy_pool_db_path()?;
    let mut conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;

    let now = now_iso();
    let mut sort_order = next_sort_order(&conn)?;
    let source_name = build_source_display_name(fetched_url);
    let source_meta = InsertSourceMeta {
        id: source.id.clone(),
        name: source_name,
    };
    let tx = conn
        .transaction()
        .map_err(|err| format!("刷新订阅节点失败: {}", err))?;
    tx.execute(
        "UPDATE proxy_sources
         SET url = ?1, last_refresh_at = ?2, last_error = '', updated_at = ?2
         WHERE id = ?3",
        params![fetched_url, now, &source.id],
    )
    .map_err(|err| format!("更新订阅来源刷新状态失败: {}", err))?;
    tx.execute(
        "DELETE FROM proxy_nodes WHERE source_id = ?1 AND builtin = 0",
        params![&source.id],
    )
    .map_err(|err| format!("刷新订阅节点失败: {}", err))?;

    let mut imported = 0usize;
    for node in nodes {
        insert_imported_node(&tx, node, sort_order, &now, Some(&source_meta))?;
        sort_order += 1;
        imported += 1;
    }
    reset_current_node_if_missing_in_tx(&tx, &now)?;

    tx.commit()
        .map_err(|err| format!("刷新订阅节点失败: {}", err))?;

    Ok(imported)
}

fn mark_subscription_source_error(source_id: &str, error: &str) -> Result<(), String> {
    let db_path = proxy_pool_db_path()?;
    let conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;
    conn.execute(
        "UPDATE proxy_sources SET last_error = ?1, updated_at = ?2 WHERE id = ?3",
        params![truncate_refresh_error(error), now_iso(), source_id],
    )
    .map_err(|err| format!("记录订阅刷新错误失败: {}", err))?;
    Ok(())
}

fn build_subscription_refresh_response(
    results: Vec<ProxySubscriptionRefreshItem>,
) -> Result<ProxySubscriptionRefreshResponse, String> {
    let refreshed = results.iter().filter(|item| item.success).count();
    let failed = results.len().saturating_sub(refreshed);
    let list = list_nodes()?;

    Ok(ProxySubscriptionRefreshResponse {
        refreshed,
        failed,
        nodes: list.nodes,
        groups: list.groups,
        sources: list.sources,
        results,
    })
}

fn build_latency_response(
    results: Vec<ProxyPoolLatencyTestResult>,
) -> Result<ProxyPoolLatencyTestResponse, String> {
    let failed = results.iter().filter(|item| !item.ok).count();
    let list = list_nodes()?;

    Ok(ProxyPoolLatencyTestResponse {
        tested: results.len(),
        failed,
        results,
        nodes: list.nodes,
        groups: list.groups,
        sources: list.sources,
    })
}

fn build_ip_health_response(
    results: Vec<ProxyPoolIpHealthResult>,
) -> Result<ProxyPoolIpHealthResponse, String> {
    let failed = results.iter().filter(|item| !item.ok).count();
    let list = list_nodes()?;

    Ok(ProxyPoolIpHealthResponse {
        checked: results.len(),
        failed,
        results,
        nodes: list.nodes,
        groups: list.groups,
        sources: list.sources,
    })
}

fn persist_latency_results(results: &[ProxyPoolLatencyTestResult]) -> Result<(), String> {
    if results.is_empty() {
        return Ok(());
    }

    let db_path = proxy_pool_db_path()?;
    let mut conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;

    let now = now_iso();
    let tx = conn
        .transaction()
        .map_err(|err| format!("保存代理测速结果失败: {}", err))?;
    for result in results {
        let status = if result.ok {
            "ok".to_string()
        } else {
            truncate_refresh_error(&result.error)
        };
        tx.execute(
            "UPDATE proxy_nodes
             SET latency_ms = ?1, latency_status = ?2, updated_at = ?3
             WHERE id = ?4",
            params![result.latency_ms, status, &now, &result.node_id],
        )
        .map_err(|err| format!("保存代理测速结果失败: {}", err))?;
    }
    tx.commit()
        .map_err(|err| format!("保存代理测速结果失败: {}", err))?;

    Ok(())
}

fn persist_ip_health_results(results: &[ProxyPoolIpHealthResult]) -> Result<(), String> {
    if results.is_empty() {
        return Ok(());
    }

    let db_path = proxy_pool_db_path()?;
    let mut conn = open_connection_at(&db_path)?;
    initialize_schema(&conn)?;
    seed_builtin_nodes(&conn)?;

    let now = now_iso();
    let tx = conn
        .transaction()
        .map_err(|err| format!("保存 IP 健康检测结果失败: {}", err))?;
    for result in results {
        let raw_json = serde_json::to_string(result)
            .map_err(|err| format!("序列化 IP 健康检测结果失败: {}", err))?;
        let summary = health::summarize_ip_health(result);
        tx.execute(
            "UPDATE proxy_nodes
             SET ip_health_json = ?1, ip_health_summary = ?2, updated_at = ?3
             WHERE id = ?4",
            params![raw_json, summary, &now, &result.node_id],
        )
        .map_err(|err| format!("保存 IP 健康检测结果失败: {}", err))?;
    }
    tx.commit()
        .map_err(|err| format!("保存 IP 健康检测结果失败: {}", err))?;

    Ok(())
}

fn truncate_refresh_error(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.chars().count() <= 500 {
        return trimmed.to_string();
    }
    trimmed.chars().take(500).collect()
}

#[derive(Debug, Clone)]
struct InsertSourceMeta {
    id: String,
    name: String,
}

fn insert_imported_node(
    tx: &Transaction<'_>,
    node: &parser::ParsedProxyNode,
    sort_order: i64,
    now: &str,
    source: Option<&InsertSourceMeta>,
) -> Result<(), String> {
    let id = Uuid::new_v4().to_string();
    let raw_config = serde_json::to_string(&node.raw_config)
        .map_err(|err| format!("序列化导入代理节点原始配置失败: {}", err))?;
    let standard_config = serde_json::to_string(&node.standard_config)
        .map_err(|err| format!("序列化导入代理节点标准配置失败: {}", err))?;
    let source_id = source.map(|item| item.id.as_str());
    let source_name = source
        .map(|item| item.name.as_str())
        .unwrap_or(node.source_kind.as_str());

    tx.execute(
        "INSERT INTO proxy_nodes (
            id, name, protocol, host, port, username, password, raw_config, standard_config,
            group_name, dns, source_id, source_name, sort_order, enabled, builtin,
            latency_ms, latency_status, ip_health_json, ip_health_summary, created_at, updated_at
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9,
            ?10, '', ?11, ?12, ?13, 0, 0,
            NULL, '', '', '', ?14, ?14
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
            source_id,
            source_name,
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
    let ip_health_text: String = row.get(15)?;
    let ip_health = parse_ip_health_result(&ip_health_text);
    let ip_health_summary = normalize_ip_health_summary(row.get(16)?, ip_health.as_ref());

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
        ip_health,
        ip_health_summary,
        masked_url: build_masked_url(&protocol, &host, port, &username, has_password),
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
    })
}

fn parse_ip_health_result(value: &str) -> Option<ProxyPoolIpHealthResult> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let result: ProxyPoolIpHealthResult = serde_json::from_str(trimmed).ok()?;
    if is_legacy_bridge_pending_message(&result.error) {
        return None;
    }
    Some(result)
}

fn normalize_ip_health_summary(
    summary: String,
    ip_health: Option<&ProxyPoolIpHealthResult>,
) -> String {
    let trimmed = summary.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if is_legacy_bridge_pending_message(trimmed) {
        return String::new();
    }

    if ip_health
        .is_some_and(|health| is_legacy_bridge_pending_message(&health.error))
    {
        return String::new();
    }

    summary
}

fn is_legacy_bridge_pending_message(value: &str) -> bool {
    value.contains("需要先完成内置桥接后才能检测")
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
        let id = request
            .id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
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
            enabled: request.enabled.unwrap_or(false),
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
    for group in nodes
        .iter()
        .map(|node| node.group.trim())
        .filter(|group| !group.is_empty())
    {
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

fn subscription_source_id(url: &str) -> Result<String, String> {
    let normalized = subscription::normalize_subscription_url(url)?;
    let digest = Sha256::digest(normalized.as_bytes());
    Ok(format!("sub-{digest:x}"))
}

fn build_source_display_name(url: &str) -> String {
    let Ok(parsed) = Url::parse(url) else {
        return "URL 订阅".to_string();
    };

    let host = parsed.host_str().unwrap_or("subscription");
    let tail = parsed
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .unwrap_or("")
        .trim_matches('/');

    if tail.is_empty() {
        host.to_string()
    } else {
        format!("{host}/{tail}")
    }
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

    #[test]
    fn builds_stable_subscription_source_id_from_normalized_url() {
        let first = subscription_source_id("https://example.com/sub").expect("valid url");
        let second = subscription_source_id("https://example.com/sub").expect("valid url");

        assert_eq!(first, second);
        assert!(first.starts_with("sub-"));
    }

    #[test]
    fn builds_readable_subscription_display_name() {
        assert_eq!(
            build_source_display_name("https://example.com/path/sub.yaml?token=secret"),
            "example.com/sub.yaml"
        );
        assert_eq!(
            build_source_display_name("https://example.com"),
            "example.com"
        );
    }
}
