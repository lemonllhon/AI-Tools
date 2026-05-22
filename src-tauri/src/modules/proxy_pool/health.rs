use super::bridge;
use super::models::{ProxyPoolIpHealthResult, ProxyPoolLatencyTestResult};
use super::store::GatewayOutboundTarget;
use reqwest::{Client, Proxy, Url};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const LATENCY_TEST_URL: &str = "http://www.gstatic.com/generate_204";
const IPPURE_INFO_URL: &str = "https://my.ippure.com/v1/info";
const LATENCY_TIMEOUT_SECONDS: u64 = 10;
const IP_HEALTH_TIMEOUT_SECONDS: u64 = 20;

#[derive(Debug, Clone)]
pub struct ProxyCheckTarget {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub standard_config: Value,
}

struct ProxyClientContext {
    client: Client,
    _temporary_bridge: Option<bridge::TemporaryBridge>,
}

pub async fn test_latency(target: ProxyCheckTarget) -> ProxyPoolLatencyTestResult {
    if target.protocol.eq_ignore_ascii_case("direct") {
        return ProxyPoolLatencyTestResult {
            node_id: target.id,
            ok: true,
            latency_ms: Some(0),
            error: String::new(),
        };
    }

    let client_context =
        match build_proxy_client(&target, Duration::from_secs(LATENCY_TIMEOUT_SECONDS)).await {
            Ok(client_context) => client_context,
            Err(error) => {
                return ProxyPoolLatencyTestResult {
                    node_id: target.id,
                    ok: false,
                    latency_ms: None,
                    error,
                };
            }
        };

    let started_at = Instant::now();
    let response = client_context
        .client
        .get(LATENCY_TEST_URL)
        .header("Cache-Control", "no-cache")
        .send()
        .await;
    let latency_ms = elapsed_ms(started_at);

    match response {
        Ok(response) if response.status().is_success() => ProxyPoolLatencyTestResult {
            node_id: target.id,
            ok: true,
            latency_ms: Some(latency_ms),
            error: String::new(),
        },
        Ok(response) => ProxyPoolLatencyTestResult {
            node_id: target.id,
            ok: false,
            latency_ms: Some(latency_ms),
            error: format!("HTTP {}", response.status().as_u16()),
        },
        Err(error) => {
            ProxyPoolLatencyTestResult {
                node_id: target.id,
                ok: false,
                latency_ms: Some(latency_ms),
                error: format!("测速失败: {}", error),
            }
        }
    }
}

pub async fn check_ip_health(target: ProxyCheckTarget) -> ProxyPoolIpHealthResult {
    let client_context =
        match build_proxy_client(&target, Duration::from_secs(IP_HEALTH_TIMEOUT_SECONDS)).await {
            Ok(client_context) => client_context,
            Err(error) => return failed_ip_health_result(target.id, error),
        };

    let response = client_context
        .client
        .get(IPPURE_INFO_URL)
        .header("Accept", "application/json")
        .header("Cache-Control", "no-cache")
        .send()
        .await;

    let response = match response {
        Ok(response) => response,
        Err(error) => {
            return failed_ip_health_result(target.id, format!("调用 IPPure 接口失败: {}", error));
        }
    };

    let status = response.status();
    let body = match response.text().await {
        Ok(body) => body,
        Err(error) => {
            return failed_ip_health_result(target.id, format!("读取 IPPure 响应失败: {}", error));
        }
    };

    if !status.is_success() {
        return failed_ip_health_result(
            target.id,
            format!("IPPure HTTP {}: {}", status.as_u16(), body_snippet(&body, 180)),
        );
    }

    let raw_data = match serde_json::from_str::<Value>(&body) {
        Ok(value) => value,
        Err(error) => {
            return failed_ip_health_result(target.id, format!("IPPure JSON 解析失败: {}", error));
        }
    };

    ProxyPoolIpHealthResult {
        node_id: target.id,
        ok: true,
        source: "ippure".to_string(),
        error: String::new(),
        ip: value_string(&raw_data, "ip"),
        fraud_score: value_i64(&raw_data, "fraudScore"),
        is_residential: value_bool(&raw_data, "isResidential"),
        is_broadcast: value_bool(&raw_data, "isBroadcast"),
        country: value_string(&raw_data, "country"),
        region: value_string(&raw_data, "region"),
        city: value_string(&raw_data, "city"),
        as_organization: value_string(&raw_data, "asOrganization"),
        raw_data,
        updated_at: now_iso(),
    }
}

pub fn summarize_ip_health(result: &ProxyPoolIpHealthResult) -> String {
    if !result.ok {
        return truncate_text(&result.error, 240);
    }

    let location = [result.country.as_str(), result.region.as_str(), result.city.as_str()]
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" / ");
    let mut parts = Vec::new();
    if !result.ip.is_empty() {
        parts.push(result.ip.clone());
    }
    if !location.is_empty() {
        parts.push(location);
    }
    if !result.as_organization.is_empty() {
        parts.push(result.as_organization.clone());
    }
    if let Some(score) = result.fraud_score {
        parts.push(format!("风险 {}", score));
    }
    if result.is_residential == Some(true) {
        parts.push("住宅".to_string());
    }
    if result.is_broadcast == Some(true) {
        parts.push("广播".to_string());
    }

    if parts.is_empty() {
        "IPPure 已返回".to_string()
    } else {
        truncate_text(&parts.join(" · "), 240)
    }
}

pub fn is_bridge_check_target(target: &ProxyCheckTarget) -> bool {
    bridge::is_bridge_protocol(&target.protocol)
}

async fn build_proxy_client(
    target: &ProxyCheckTarget,
    timeout: Duration,
) -> Result<ProxyClientContext, String> {
    let mut builder = Client::builder()
        .timeout(timeout)
        .user_agent("ai-lemon-tools-proxy-health/1.0");
    let mut temporary_bridge = None;

    if !target.protocol.eq_ignore_ascii_case("direct") {
        let (proxy_url, bridge_guard) = build_proxy_url(target).await?;
        temporary_bridge = bridge_guard;
        let proxy = Proxy::all(proxy_url.as_str())
            .map_err(|err| format!("代理客户端初始化失败: {}", err))?;
        builder = builder.proxy(proxy);
    }

    let client = builder
        .build()
        .map_err(|err| format!("代理 HTTP 客户端初始化失败: {}", err))?;
    Ok(ProxyClientContext {
        client,
        _temporary_bridge: temporary_bridge,
    })
}

async fn build_proxy_url(
    target: &ProxyCheckTarget,
) -> Result<(Url, Option<bridge::TemporaryBridge>), String> {
    let scheme = match target.protocol.to_ascii_lowercase().as_str() {
        "http" => "http",
        "https" => "https",
        "socks5" => "socks5h",
        "vmess" | "vless" | "trojan" | "ss" | "hysteria" | "hysteria2" | "tuic"
        | "anytls" => {
            return build_bridge_proxy_url(target).await;
        }
        other => return Err(format!("暂不支持检测 {} 协议节点", other)),
    };

    let host = target.host.trim();
    if host.is_empty() || target.port == 0 {
        return Err("代理节点地址或端口为空".to_string());
    }

    let mut url = Url::parse(&format!("{scheme}://127.0.0.1"))
        .map_err(|err| format!("代理 URL 初始化失败: {}", err))?;
    url.set_host(Some(host))
        .map_err(|_| "代理节点地址格式错误".to_string())?;
    url.set_port(Some(target.port))
        .map_err(|_| "代理节点端口格式错误".to_string())?;
    if !target.username.is_empty() {
        url.set_username(&target.username)
            .map_err(|_| "代理账号格式错误".to_string())?;
        if !target.password.is_empty() {
            url.set_password(Some(&target.password))
                .map_err(|_| "代理密码格式错误".to_string())?;
        }
    }

    Ok((url, None))
}

async fn build_bridge_proxy_url(
    target: &ProxyCheckTarget,
) -> Result<(Url, Option<bridge::TemporaryBridge>), String> {
    if target.host.trim().is_empty() || target.port == 0 {
        return Err("代理节点地址或端口为空".to_string());
    }

    let outbound = GatewayOutboundTarget {
        id: target.id.clone(),
        name: target.name.clone(),
        protocol: target.protocol.clone(),
        host: target.host.clone(),
        port: target.port,
        username: target.username.clone(),
        password: target.password.clone(),
        gateway_port: 0,
        standard_config: target.standard_config.clone(),
    };
    let temporary_bridge = bridge::start_temporary_bridge(&outbound).await?;
    let endpoint = temporary_bridge.endpoint().clone();
    let mut url = Url::parse("socks5h://127.0.0.1")
        .map_err(|err| format!("代理桥接 URL 初始化失败: {}", err))?;
    url.set_host(Some(&endpoint.host))
        .map_err(|_| "代理桥接地址格式错误".to_string())?;
    url.set_port(Some(endpoint.port))
        .map_err(|_| "代理桥接端口格式错误".to_string())?;
    Ok((url, Some(temporary_bridge)))
}

fn failed_ip_health_result(node_id: String, error: String) -> ProxyPoolIpHealthResult {
    ProxyPoolIpHealthResult {
        node_id,
        ok: false,
        source: "ippure".to_string(),
        error,
        ip: String::new(),
        fraud_score: None,
        is_residential: None,
        is_broadcast: None,
        country: String::new(),
        region: String::new(),
        city: String::new(),
        as_organization: String::new(),
        raw_data: json!({}),
        updated_at: now_iso(),
    }
}

fn value_string(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        _ => String::new(),
    }
}

fn value_i64(value: &Value, key: &str) -> Option<i64> {
    match value.get(key) {
        Some(Value::Number(number)) => number
            .as_i64()
            .or_else(|| number.as_u64().and_then(|value| i64::try_from(value).ok()))
            .or_else(|| number.as_f64().map(|value| value as i64)),
        Some(Value::String(text)) => text
            .parse::<i64>()
            .ok()
            .or_else(|| text.parse::<f64>().ok().map(|value| value as i64)),
        _ => None,
    }
}

fn value_bool(value: &Value, key: &str) -> Option<bool> {
    match value.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        Some(Value::Number(number)) => number.as_i64().map(|value| value != 0),
        Some(Value::String(text)) => {
            let normalized = text.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "true" | "1" | "yes" => Some(true),
                "false" | "0" | "no" => Some(false),
                _ => None,
            }
        }
        _ => None,
    }
}

fn body_snippet(body: &str, max_len: usize) -> String {
    truncate_text(body.trim(), max_len)
}

fn truncate_text(value: &str, max_len: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_len {
        return trimmed.to_string();
    }
    trimmed.chars().take(max_len).collect::<String>() + "..."
}

fn elapsed_ms(started_at: Instant) -> i64 {
    let millis = started_at.elapsed().as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
