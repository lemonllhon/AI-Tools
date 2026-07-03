use crate::models::codex::{CodexAccount, CodexQuota, CodexQuotaErrorInfo};
use crate::modules::{codex_account, codex_local_access, logger, websocket};
use futures::stream::{self, StreamExt};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

// 使用 wham/usage 端点（Quotio 使用的）
const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const COCKPIT_API_PROVIDER_ID: &str = "cockpit_api";
const LEGACY_NEW_API_PROVIDER_ID: &str = "new_api";
const COCKPIT_API_PLAN_TYPE: &str = "Cockpit Api";
const LEGACY_NEW_API_EXCLUSIVE_PLAN_TYPE: &str = "NEW_API_EXCLUSIVE";
const COCKPIT_API_BASE_URL: &str = "https://chongcodex.cn/v1";
const NEW_API_USAGE_PATH: &str = "/api/usage/token/";
const QUOTA_HTTP_TIMEOUT_SECONDS: u64 = 20;
const REFRESH_ALL_MAX_CONCURRENT: usize = 5;
const BACKGROUND_QUOTA_STALE_SECONDS: i64 = 15 * 60;
const BACKGROUND_QUOTA_ERROR_RETRY_SECONDS: i64 = 10 * 60;
const QUOTA_RESET_REFRESH_GRACE_SECONDS: i64 = 60;
const TRANSIENT_QUOTA_ERROR_CODE: &str = "quota_refresh_transient";
static CODEX_QUOTA_REFRESH_LOCKS: LazyLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexQuotaRefreshAllProgress {
    pub current: usize,
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub account_id: String,
    pub account_email: Option<String>,
    pub ok: bool,
    pub error: Option<String>,
}

pub type CodexQuotaRefreshAllProgressCallback =
    Arc<dyn Fn(CodexQuotaRefreshAllProgress) + Send + Sync + 'static>;

fn quota_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(QUOTA_HTTP_TIMEOUT_SECONDS))
        .build()
        .map_err(|err| format!("创建额度查询 HTTP 客户端失败: {}", err))
}

fn quota_refresh_lock(account_id: &str) -> Arc<tokio::sync::Mutex<()>> {
    let mut locks = CODEX_QUOTA_REFRESH_LOCKS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    locks
        .entry(account_id.to_string())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

fn get_header_value(headers: &HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string()
}

fn extract_detail_code_from_body(body: &str) -> Option<String> {
    codex_account::extract_codex_error_code_from_text(body)
}

fn extract_error_code_from_message(message: &str) -> Option<String> {
    let marker = "[error_code:";
    if let Some(start) = message.find(marker) {
        let code_start = start + marker.len();
        let end = message[code_start..].find(']')?;
        return Some(message[code_start..code_start + end].to_string());
    }

    let marker = "error_code=";
    let start = message.find(marker)?;
    let code_start = start + marker.len();
    let tail = &message[code_start..];
    let end = tail
        .find(|ch: char| ch == ',' || ch == ']' || ch.is_whitespace())
        .unwrap_or(tail.len());
    let code = tail[..end].trim();
    if code.is_empty() {
        None
    } else {
        Some(code.to_string())
    }
}

fn is_transient_quota_error_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("请求失败")
        || lower.contains("读取响应失败")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("operation timed out")
        || lower.contains("elapsed")
        || lower.contains("network")
        || lower.contains("connection")
        || lower.contains("connect error")
        || lower.contains("connection refused")
        || lower.contains("connection reset")
        || lower.contains("connection closed")
        || lower.contains("connection aborted")
        || lower.contains("broken pipe")
        || lower.contains("dns")
        || lower.contains("tls")
        || lower.contains("ssl")
        || lower.contains("certificate")
        || lower.contains("handshake")
        || lower.contains("proxy")
        || lower.contains("socks")
        || lower.contains("tunnel")
        || lower.contains("transport")
        || lower.contains("超时")
        || lower.contains("连接")
        || lower.contains("无法连接")
        || lower.contains("连接失败")
        || lower.contains("连接重置")
        || lower.contains("连接被")
        || lower.contains("代理")
        || lower.contains("证书")
        || lower.contains("握手")
        || lower.contains("temporarily unavailable")
        || lower.contains("temporary")
        || lower.contains("bad gateway")
        || lower.contains("service unavailable")
        || lower.contains("gateway timeout")
        || lower.contains("unsupported_country_region_territory")
        || lower.contains("当前网络地区不支持")
        || lower.contains("网络")
        || lower.contains("too many requests")
        || lower.contains("429")
        || lower.contains("api 返回错误 408")
        || lower.contains("api 返回错误 5")
        || lower.contains("http 408")
        || lower.contains("http 5")
        || lower.contains("http 502")
        || lower.contains("http 503")
        || lower.contains("http 504")
}

fn write_quota_error(account: &mut CodexAccount, message: String) {
    let code = codex_account::extract_codex_error_code_from_text(&message)
        .or_else(|| extract_error_code_from_message(&message))
        .or_else(|| {
            is_transient_quota_error_message(&message)
                .then(|| TRANSIENT_QUOTA_ERROR_CODE.to_string())
        });
    account.quota_error = Some(CodexQuotaErrorInfo {
        code,
        message,
        timestamp: chrono::Utc::now().timestamp(),
    });
}

fn is_deactivated_workspace_delete_message(message: &str) -> bool {
    if !codex_account::is_deactivated_workspace_error_message(message) {
        return false;
    }
    let lower = message.to_ascii_lowercase();
    lower.contains("402") || lower.contains("payment required")
}

async fn delete_deactivated_workspace_account_if_needed(account: &CodexAccount, message: &str) {
    if !is_deactivated_workspace_delete_message(message) {
        return;
    }

    let reason = "账号工作区已停用 (deactivated_workspace)";
    let account_id = account.id.clone();
    let removed_account_ids = vec![account_id.clone()];
    match codex_account::remove_account(&account_id) {
        Ok(()) => {
            logger::log_info(&format!(
                "Codex 配额刷新检测到异常账号并已自动物理删除: account_id={}, email={}, reason={}",
                account.id, account.email, reason
            ));
            if let Err(error) =
                codex_local_access::remove_deleted_account_references(&removed_account_ids, reason)
                    .await
            {
                logger::log_warn(&format!(
                    "Codex 配额刷新删除异常账号后剥离 API 服务引用失败: account_id={}, error={}",
                    account.id, error
                ));
            }
            websocket::broadcast_data_changed("codex_accounts_deleted");
        }
        Err(error) => logger::log_warn(&format!(
            "Codex 配额刷新检测到异常账号但自动物理删除失败: account_id={}, email={}, reason={}, error={}",
            account.id, account.email, reason, error
        )),
    }
}

/// 使用率窗口（5小时/周）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WindowInfo {
    #[serde(rename = "used_percent")]
    used_percent: Option<i32>,
    #[serde(rename = "limit_window_seconds")]
    limit_window_seconds: Option<i64>,
    #[serde(rename = "reset_after_seconds")]
    reset_after_seconds: Option<i64>,
    #[serde(rename = "reset_at")]
    reset_at: Option<i64>,
}

/// 速率限制信息
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RateLimitInfo {
    allowed: Option<bool>,
    #[serde(rename = "limit_reached")]
    limit_reached: Option<bool>,
    #[serde(rename = "primary_window")]
    primary_window: Option<WindowInfo>,
    #[serde(rename = "secondary_window")]
    secondary_window: Option<WindowInfo>,
}

/// 使用率响应
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UsageResponse {
    #[serde(rename = "plan_type")]
    plan_type: Option<String>,
    #[serde(rename = "rate_limit")]
    rate_limit: Option<RateLimitInfo>,
    #[serde(rename = "code_review_rate_limit")]
    code_review_rate_limit: Option<RateLimitInfo>,
}

fn normalize_remaining_percentage(window: &WindowInfo) -> i32 {
    let used = window.used_percent.unwrap_or(0).clamp(0, 100);
    100 - used
}

fn normalize_window_minutes(window: &WindowInfo) -> Option<i64> {
    let seconds = window.limit_window_seconds?;
    if seconds <= 0 {
        return None;
    }
    Some((seconds + 59) / 60)
}

fn normalize_reset_time(window: &WindowInfo) -> Option<i64> {
    if let Some(reset_at) = window.reset_at {
        return Some(reset_at);
    }

    let reset_after_seconds = window.reset_after_seconds?;
    if reset_after_seconds < 0 {
        return None;
    }

    Some(chrono::Utc::now().timestamp() + reset_after_seconds)
}

/// 配额查询结果（包含 plan_type）
pub struct FetchQuotaResult {
    pub quota: CodexQuota,
    pub plan_type: Option<String>,
}

async fn refresh_account_tokens(account: &mut CodexAccount, reason: &str) -> Result<(), String> {
    logger::log_info(&format!(
        "Codex 账号 {} 触发强制 Token 刷新: {}",
        account.email, reason
    ));

    let refreshed = codex_account::force_refresh_managed_account(&account.id, reason)
        .await
        .map_err(|e| format!("{}，刷新 Token 失败: {}", reason, e))?;
    *account = refreshed;
    Ok(())
}

/// 查询单个账号的配额
pub async fn fetch_quota(account: &CodexAccount) -> Result<FetchQuotaResult, String> {
    let client = quota_http_client()?;

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", account.tokens.access_token))
            .map_err(|e| format!("构建 Authorization 头失败: {}", e))?,
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

    // 添加 ChatGPT-Account-Id 头（关键！）
    let account_id = account.account_id.clone().or_else(|| {
        codex_account::extract_chatgpt_account_id_from_access_token(&account.tokens.access_token)
    });

    if let Some(ref acc_id) = account_id {
        if !acc_id.is_empty() {
            headers.insert(
                "ChatGPT-Account-Id",
                HeaderValue::from_str(acc_id)
                    .map_err(|e| format!("构建 Account-Id 头失败: {}", e))?,
            );
        }
    }

    logger::log_info(&format!(
        "Codex 配额请求: {} (account_id: {:?})",
        USAGE_URL, account_id
    ));

    let response = client
        .get(USAGE_URL)
        .headers(headers)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .text()
        .await
        .map_err(|e| format!("读取响应失败: {}", e))?;

    let request_id = get_header_value(&headers, "request-id");
    let x_request_id = get_header_value(&headers, "x-request-id");
    let cf_ray = get_header_value(&headers, "cf-ray");
    let body_len = body.len();

    logger::log_info(&format!(
        "Codex 配额响应元信息: url={}, status={}, request-id={}, x-request-id={}, cf-ray={}, body_len={}",
        USAGE_URL, status, request_id, x_request_id, cf_ray, body_len
    ));

    if !status.is_success() {
        let detail_code = extract_detail_code_from_body(&body);

        logger::log_error(&format!(
            "Codex 配额接口返回非成功状态: url={}, status={}, request-id={}, x-request-id={}, cf-ray={}, detail_code={:?}, body_len={}",
            USAGE_URL,
            status,
            request_id,
            x_request_id,
            cf_ray,
            detail_code,
            body_len
        ));

        let mut error_message = format!("API 返回错误 {}", status);
        if let Some(code) = detail_code {
            error_message.push_str(&format!(" [error_code:{}]", code));
        }
        error_message.push_str(&format!(" [body_len:{}]", body_len));
        return Err(error_message);
    }

    // 解析响应
    let usage: UsageResponse =
        serde_json::from_str(&body).map_err(|e| format!("解析 JSON 失败: {}", e))?;

    let quota = parse_quota_from_usage(&usage, &body)?;
    let plan_type = usage.plan_type.clone();

    Ok(FetchQuotaResult { quota, plan_type })
}

/// 从使用率响应中解析配额信息
fn parse_quota_from_usage(usage: &UsageResponse, raw_body: &str) -> Result<CodexQuota, String> {
    let rate_limit = usage.rate_limit.as_ref();
    let primary_window = rate_limit.and_then(|r| r.primary_window.as_ref());
    let secondary_window = rate_limit.and_then(|r| r.secondary_window.as_ref());

    // Primary window = 5小时配额（session）
    let (hourly_percentage, hourly_reset_time, hourly_window_minutes) =
        if let Some(primary) = primary_window {
            (
                normalize_remaining_percentage(primary),
                normalize_reset_time(primary),
                normalize_window_minutes(primary),
            )
        } else {
            (100, None, None)
        };

    // Secondary window = 周配额
    let (weekly_percentage, weekly_reset_time, weekly_window_minutes) =
        if let Some(secondary) = secondary_window {
            (
                normalize_remaining_percentage(secondary),
                normalize_reset_time(secondary),
                normalize_window_minutes(secondary),
            )
        } else {
            (100, None, None)
        };

    // 保存原始响应
    let raw_data: Option<serde_json::Value> = serde_json::from_str(raw_body).ok();

    Ok(CodexQuota {
        hourly_percentage,
        hourly_reset_time,
        hourly_window_minutes,
        hourly_window_present: Some(primary_window.is_some()),
        weekly_percentage,
        weekly_reset_time,
        weekly_window_minutes,
        weekly_window_present: Some(secondary_window.is_some()),
        raw_data,
    })
}

fn is_new_api_account(account: &CodexAccount) -> bool {
    account
        .api_provider_id
        .as_deref()
        .map(|value| {
            let value = value.trim();
            value.eq_ignore_ascii_case(COCKPIT_API_PROVIDER_ID)
                || value.eq_ignore_ascii_case(LEGACY_NEW_API_PROVIDER_ID)
        })
        .unwrap_or(false)
        || is_cockpit_api_base_url(account.api_base_url.as_deref())
        || account
            .plan_type
            .as_deref()
            .map(|value| {
                let value = value.trim();
                value.eq_ignore_ascii_case(COCKPIT_API_PLAN_TYPE)
                    || value.eq_ignore_ascii_case(LEGACY_NEW_API_EXCLUSIVE_PLAN_TYPE)
            })
            .unwrap_or(false)
}

fn is_new_api_provider_account(account: &CodexAccount) -> bool {
    account
        .api_provider_id
        .as_deref()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            normalized == LEGACY_NEW_API_PROVIDER_ID || normalized == "newapi"
        })
        .unwrap_or(false)
}

fn normalize_api_base_url_for_match(raw: Option<&str>) -> Option<String> {
    let parsed = reqwest::Url::parse(raw?.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = parsed.host_str()?;
    let port = parsed
        .port()
        .map(|value| format!(":{}", value))
        .unwrap_or_default();
    let path = parsed.path().trim_end_matches('/');
    Some(format!("{}://{}{}{}", parsed.scheme(), host, port, path).to_ascii_lowercase())
}

fn is_cockpit_api_base_url(raw: Option<&str>) -> bool {
    let Some(actual) = normalize_api_base_url_for_match(raw) else {
        return false;
    };
    let Some(expected) = normalize_api_base_url_for_match(Some(COCKPIT_API_BASE_URL)) else {
        return false;
    };
    actual == expected
}

fn build_new_api_profile_url(account: &CodexAccount) -> Result<String, String> {
    let base_url = account
        .api_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("Cockpit Api 账号缺少 Base URL")?;
    let mut parsed = reqwest::Url::parse(base_url)
        .map_err(|err| format!("Cockpit Api Base URL 无效: {}", err))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Cockpit Api Base URL 仅支持 http/https".to_string());
    }
    parsed.set_path("/api/ai-lemon-tools/token-profile");
    parsed.set_query(None);
    parsed.set_fragment(None);
    Ok(parsed.to_string())
}

fn build_new_api_usage_url(account: &CodexAccount) -> Result<String, String> {
    let base_url = account
        .api_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("New API 账号缺少 Base URL")?;
    let mut parsed =
        reqwest::Url::parse(base_url).map_err(|err| format!("New API Base URL 无效: {}", err))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("New API Base URL 仅支持 http/https".to_string());
    }
    parsed.set_path(NEW_API_USAGE_PATH);
    parsed.set_query(None);
    parsed.set_fragment(None);
    Ok(parsed.to_string())
}

fn read_i64(value: &serde_json::Value, key: &str) -> i64 {
    value
        .get(key)
        .and_then(|item| {
            item.as_i64()
                .or_else(|| item.as_u64().and_then(|raw| i64::try_from(raw).ok()))
        })
        .unwrap_or(0)
}

fn read_bool(value: &serde_json::Value, key: &str) -> bool {
    value
        .get(key)
        .and_then(|item| item.as_bool())
        .unwrap_or(false)
}

fn new_api_percentage(available: i64, total: i64, unlimited: bool) -> i32 {
    if unlimited {
        return 100;
    }
    if total <= 0 {
        return 0;
    }
    let percentage = (available.max(0) as f64 / total.max(1) as f64) * 100.0;
    percentage.round().clamp(0.0, 100.0) as i32
}

async fn fetch_new_api_quota(account: &CodexAccount) -> Result<FetchQuotaResult, String> {
    if is_new_api_provider_account(account) {
        return fetch_new_api_usage_quota(account).await;
    }

    let api_key = account
        .openai_api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("Cockpit Api 账号缺少 OPENAI_API_KEY")?;
    let profile_url = build_new_api_profile_url(account)?;
    let client = quota_http_client()?;
    let response = client
        .get(&profile_url)
        .bearer_auth(api_key)
        .header(ACCEPT, "application/json")
        .send()
        .await
        .map_err(|err| format!("请求 Cockpit Api 额度失败: {}", err))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| format!("读取 Cockpit Api 额度响应失败: {}", err))?;
    if !status.is_success() {
        return Err(format!("Cockpit Api 额度接口返回 HTTP {}", status.as_u16()));
    }

    let root: serde_json::Value = serde_json::from_str(&body)
        .map_err(|err| format!("解析 Cockpit Api 额度 JSON 失败: {}", err))?;
    if root.get("success").and_then(|item| item.as_bool()) == Some(false) {
        let message = root
            .get("message")
            .and_then(|item| item.as_str())
            .unwrap_or("Cockpit Api 额度接口返回失败");
        return Err(message.to_string());
    }
    let data = root.get("data").unwrap_or(&root);
    let usage = data.get("usage").ok_or("Cockpit Api 额度响应缺少 usage")?;
    let total = read_i64(usage, "total_granted");
    let used = read_i64(usage, "total_used");
    let available = read_i64(usage, "total_available");
    let unlimited = read_bool(usage, "unlimited_quota");
    let percentage = new_api_percentage(available, total, unlimited);
    let expires_at = read_i64(usage, "expires_at");
    let reset_time = if expires_at > 0 {
        Some(expires_at)
    } else {
        None
    };

    Ok(FetchQuotaResult {
        quota: CodexQuota {
            hourly_percentage: percentage,
            hourly_reset_time: reset_time,
            hourly_window_minutes: None,
            hourly_window_present: Some(true),
            weekly_percentage: 0,
            weekly_reset_time: None,
            weekly_window_minutes: None,
            weekly_window_present: Some(false),
            raw_data: Some(json!({
                "provider": "cockpit-api",
                "object": "codex_cockpit_api_quota",
                "profile": data,
                "usage": usage,
                "total_granted": total,
                "total_used": used,
                "total_available": available,
                "unlimited_quota": unlimited
            })),
        },
        plan_type: Some(
            data.get("plan_type")
                .and_then(|item| item.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| COCKPIT_API_PLAN_TYPE.to_string()),
        ),
    })
}

async fn fetch_new_api_usage_quota(account: &CodexAccount) -> Result<FetchQuotaResult, String> {
    let api_key = account
        .openai_api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("New API 账号缺少 OPENAI_API_KEY")?;
    let usage_url = build_new_api_usage_url(account)?;
    let client = quota_http_client()?;
    let response = client
        .get(&usage_url)
        .bearer_auth(api_key)
        .header(ACCEPT, "application/json")
        .send()
        .await
        .map_err(|err| format!("请求 New API 额度失败: {}", err))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| format!("读取 New API 额度响应失败: {}", err))?;
    if !status.is_success() {
        return Err(format!("New API 额度接口返回 HTTP {}", status.as_u16()));
    }

    let root: serde_json::Value =
        serde_json::from_str(&body).map_err(|err| format!("解析 New API 额度 JSON 失败: {}", err))?;
    if root.get("success").and_then(|item| item.as_bool()) == Some(false)
        || root.get("code").and_then(|item| item.as_bool()) == Some(false)
    {
        let message = root
            .get("message")
            .and_then(|item| item.as_str())
            .unwrap_or("New API 额度接口返回失败");
        return Err(message.to_string());
    }
    let data = root.get("data").unwrap_or(&root);
    let total = read_i64(data, "total_granted");
    let used = read_i64(data, "total_used");
    let available = read_i64(data, "total_available");
    let unlimited = read_bool(data, "unlimited_quota");
    let percentage = new_api_percentage(available, total, unlimited);
    let expires_at = read_i64(data, "expires_at");
    let reset_time = if expires_at > 0 {
        Some(expires_at)
    } else {
        None
    };

    Ok(FetchQuotaResult {
        quota: CodexQuota {
            hourly_percentage: percentage,
            hourly_reset_time: reset_time,
            hourly_window_minutes: None,
            hourly_window_present: Some(true),
            weekly_percentage: 0,
            weekly_reset_time: None,
            weekly_window_minutes: None,
            weekly_window_present: Some(false),
            raw_data: Some(json!({
                "provider": "new-api",
                "object": "codex_new_api_quota",
                "usage": data,
                "total_granted": total,
                "total_used": used,
                "total_available": available,
                "unlimited_quota": unlimited,
                "summary_display": if unlimited {
                    "不限量".to_string()
                } else {
                    format!("{} / {}", available, total)
                },
            })),
        },
        plan_type: Some(LEGACY_NEW_API_EXCLUSIVE_PLAN_TYPE.to_string()),
    })
}

/// 从 id_token 中提取订阅标识并同步更新账号和索引
fn sync_subscription_from_token(
    account: &mut CodexAccount,
    plan_type: Option<String>,
    subscription_active_until: Option<String>,
) {
    let mut changed = false;
    if let Some(ref new_plan) = plan_type {
        let old_plan = account.plan_type.clone();
        if account.plan_type.as_deref() != Some(new_plan) {
            logger::log_info(&format!(
                "Codex 账号 {} 订阅标识已更新: {:?} -> {:?}",
                account.email, old_plan, plan_type
            ));
            account.plan_type = plan_type;
            changed = true;
        }
    }

    if let Some(ref next_expiry) = subscription_active_until {
        if account.subscription_active_until.as_deref() != Some(next_expiry) {
            account.subscription_active_until = Some(next_expiry.clone());
            changed = true;
        }
    }

    if changed {
        if let Err(e) = codex_account::update_account_plan_type_in_index(
            &account.id,
            &account.plan_type,
            &account.subscription_active_until,
        ) {
            logger::log_warn(&format!("更新索引 plan_type 失败: {}", e));
        }
    }
}

fn sync_subscription_expiry_from_current_id_token(account: &mut CodexAccount) {
    if let Ok((_, _, _, subscription_active_until, _, _)) =
        codex_account::extract_user_info(&account.tokens.id_token)
    {
        sync_subscription_from_token(account, None, subscription_active_until);
    }
}

/// 刷新账号配额并保存（包含 token 自动刷新）
async fn refresh_account_quota_once(account_id: &str) -> Result<CodexQuota, String> {
    let mut account = codex_account::prepare_account_for_injection(account_id).await?;
    if account.is_api_key_auth() {
        if is_new_api_account(&account) {
            let result = match fetch_new_api_quota(&account).await {
                Ok(result) => result,
                Err(e) => {
                    write_quota_error(&mut account, e.clone());
                    if let Err(save_err) = codex_account::save_account(&account) {
                        logger::log_warn(&format!("写入 Cockpit Api 配额错误失败: {}", save_err));
                    }
                    delete_deactivated_workspace_account_if_needed(&account, &e).await;
                    return Err(e);
                }
            };
            if result.plan_type.is_some() {
                sync_subscription_from_token(&mut account, result.plan_type, None);
            }
            account.quota = Some(result.quota.clone());
            account.quota_error = None;
            account.usage_updated_at = Some(chrono::Utc::now().timestamp());
            codex_account::save_account(&account)?;
            return Ok(result.quota);
        }
        account.quota = None;
        account.quota_error = None;
        account.usage_updated_at = None;
        let _ = codex_account::save_account(&account);
        return Err("API Key 账号不支持刷新配额，请在网页端查看。".to_string());
    }

    // 检查 token 是否过期，如果过期则刷新
    if crate::modules::codex_oauth::is_token_expired(&account.tokens.access_token) {
        match refresh_account_tokens(&mut account, "Token 已过期").await {
            Ok(()) => {
                logger::log_info(&format!("账号 {} 的 Token 刷新成功", account.email));

                sync_subscription_expiry_from_current_id_token(&mut account);

                codex_account::save_account(&account)?;
            }
            Err(e) => {
                logger::log_error(&format!("账号 {} Token 刷新失败: {}", account.email, e));
                let message = e;
                write_quota_error(&mut account, message.clone());
                if let Err(save_err) = codex_account::save_account(&account) {
                    logger::log_warn(&format!("写入 Codex 配额错误失败: {}", save_err));
                }
                delete_deactivated_workspace_account_if_needed(&account, &message).await;
                return Err(message);
            }
        }
    }

    let result = match fetch_quota(&account).await {
        Ok(result) => result,
        Err(e) => {
            write_quota_error(&mut account, e.clone());
            if let Err(save_err) = codex_account::save_account(&account) {
                logger::log_warn(&format!("写入 Codex 配额错误失败: {}", save_err));
            }
            delete_deactivated_workspace_account_if_needed(&account, &e).await;
            return Err(e);
        }
    };

    // 从 usage 响应中的 plan_type 更新订阅标识
    if result.plan_type.is_some() {
        sync_subscription_from_token(&mut account, result.plan_type, None);
    }

    account.quota = Some(result.quota.clone());
    account.quota_error = None;
    account.usage_updated_at = Some(chrono::Utc::now().timestamp());
    codex_account::save_account(&account)?;

    Ok(result.quota)
}

pub async fn refresh_account_quota(account_id: &str) -> Result<CodexQuota, String> {
    let lock = quota_refresh_lock(account_id);
    let _guard = lock.lock().await;
    refresh_account_quota_once(account_id).await
}

fn quota_reset_refresh_due(account: &CodexAccount, now: i64) -> bool {
    let Some(quota) = account.quota.as_ref() else {
        return false;
    };
    let updated_at = account.usage_updated_at.unwrap_or(0);
    let reset_times = [quota.hourly_reset_time, quota.weekly_reset_time];
    reset_times.into_iter().flatten().any(|reset_at| {
        reset_at > 0
            && reset_at <= now + QUOTA_RESET_REFRESH_GRACE_SECONDS
            && updated_at < reset_at
    })
}

fn should_refresh_quota_in_background(account: &CodexAccount, now: i64) -> bool {
    if account.quota.is_none() && account.quota_error.is_none() {
        return true;
    }
    if let Some(quota_error) = account.quota_error.as_ref() {
        return now.saturating_sub(quota_error.timestamp) >= BACKGROUND_QUOTA_ERROR_RETRY_SECONDS;
    }
    if quota_reset_refresh_due(account, now) {
        return true;
    }
    let updated_at = account.usage_updated_at.unwrap_or(0);
    updated_at <= 0 || now.saturating_sub(updated_at) >= BACKGROUND_QUOTA_STALE_SECONDS
}

/// 刷新所有账号配额
pub async fn refresh_all_quotas(
    force: bool,
) -> Result<Vec<(String, Result<CodexQuota, String>)>, String> {
    refresh_all_quotas_with_progress(force, None).await
}

/// 刷新所有账号配额，并在每个账号完成后回传真实进度。
pub async fn refresh_all_quotas_with_progress(
    force: bool,
    progress_callback: Option<CodexQuotaRefreshAllProgressCallback>,
) -> Result<Vec<(String, Result<CodexQuota, String>)>, String> {
    let now = chrono::Utc::now().timestamp();
    let accounts: Vec<_> = codex_account::list_accounts()
        .into_iter()
        .filter(|account| !account.is_api_key_auth() || is_new_api_account(account))
        .filter(|account| force || should_refresh_quota_in_background(account, now))
        .collect();
    let total = accounts.len();
    let completed = Arc::new(AtomicUsize::new(0));
    let success = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicUsize::new(0));

    let results = stream::iter(accounts.into_iter().map(|account| {
        let account_id = account.id.clone();
        let account_email = Some(account.email.clone());
        let completed = Arc::clone(&completed);
        let success = Arc::clone(&success);
        let failed = Arc::clone(&failed);
        let progress_callback = progress_callback.clone();
        async move {
            let result = refresh_account_quota(&account_id).await;
            if let Some(progress_callback) = progress_callback.as_ref() {
                let ok = result.is_ok();
                let error = result.as_ref().err().cloned();
                let current = completed.fetch_add(1, Ordering::SeqCst) + 1;
                let success_count = if ok {
                    success.fetch_add(1, Ordering::SeqCst) + 1
                } else {
                    success.load(Ordering::SeqCst)
                };
                let failed_count = if ok {
                    failed.load(Ordering::SeqCst)
                } else {
                    failed.fetch_add(1, Ordering::SeqCst) + 1
                };
                progress_callback(CodexQuotaRefreshAllProgress {
                    current,
                    total,
                    success: success_count,
                    failed: failed_count,
                    account_id: account_id.clone(),
                    account_email,
                    ok,
                    error,
                });
            }
            (account_id, result)
        }
    }))
        .buffer_unordered(REFRESH_ALL_MAX_CONCURRENT)
        .collect::<Vec<_>>()
        .await;

    Ok(results)
}
