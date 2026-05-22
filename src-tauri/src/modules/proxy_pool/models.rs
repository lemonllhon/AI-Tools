use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DIRECT_NODE_ID: &str = "__direct__";
pub const LOCAL_NODE_ID: &str = "__local__";
pub const OUTLET_MODE_DIRECT: &str = "direct";
pub const OUTLET_MODE_LOCAL: &str = "local";
pub const OUTLET_MODE_NODE_POOL: &str = "node_pool";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyPoolNode {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub has_password: bool,
    pub group: String,
    pub source_id: Option<String>,
    pub source_name: String,
    pub sort_order: i64,
    pub enabled: bool,
    pub builtin: bool,
    pub latency_ms: Option<i64>,
    pub latency_status: String,
    pub ip_health_summary: String,
    pub masked_url: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySource {
    pub id: String,
    pub url: String,
    pub display_name: String,
    pub name_prefix: String,
    pub group: String,
    pub dns: String,
    pub auto_refresh_enabled: bool,
    pub refresh_interval_minutes: i64,
    pub last_refresh_at: Option<String>,
    pub last_error: String,
    pub node_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyPoolListResponse {
    pub db_path: String,
    pub nodes: Vec<ProxyPoolNode>,
    pub groups: Vec<String>,
    pub sources: Vec<ProxySource>,
    pub service_state: ProxyPoolServiceState,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyNodeSaveRequest {
    pub id: Option<String>,
    pub name: String,
    pub protocol: String,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyImportPreviewRequest {
    pub content: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub name_prefix: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyImportApplyRequest {
    pub content: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub name_prefix: Option<String>,
    pub selected_preview_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySubscriptionPreviewRequest {
    pub url: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub name_prefix: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySubscriptionApplyRequest {
    pub url: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub name_prefix: Option<String>,
    pub selected_preview_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySubscriptionRefreshRequest {
    pub source_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySourceUpdateRequest {
    pub source_id: String,
    pub url: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub name_prefix: Option<String>,
    #[serde(default)]
    pub dns: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyPoolServiceState {
    pub enabled: bool,
    pub preferred_port: u16,
    pub actual_port: Option<u16>,
    pub gateway_url: String,
    pub outlet_mode: String,
    pub selected_node_ids: Vec<String>,
    pub current_node_id: String,
    pub current_node_name: String,
    pub current_node_protocol: String,
    pub local_proxy_port: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyPoolServiceUpdateRequest {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub preferred_port: Option<u16>,
    #[serde(default)]
    pub outlet_mode: Option<String>,
    #[serde(default)]
    pub selected_node_ids: Option<Vec<String>>,
    #[serde(default)]
    pub current_node_id: Option<String>,
    #[serde(default)]
    pub local_proxy_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyPoolLatencyTestResult {
    pub node_id: String,
    pub ok: bool,
    pub latency_ms: Option<i64>,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyPoolLatencyTestResponse {
    pub tested: usize,
    pub failed: usize,
    pub results: Vec<ProxyPoolLatencyTestResult>,
    pub nodes: Vec<ProxyPoolNode>,
    pub groups: Vec<String>,
    pub sources: Vec<ProxySource>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyPoolIpHealthResult {
    pub node_id: String,
    pub ok: bool,
    pub source: String,
    pub error: String,
    pub ip: String,
    pub fraud_score: Option<i64>,
    pub is_residential: Option<bool>,
    pub is_broadcast: Option<bool>,
    pub country: String,
    pub region: String,
    pub city: String,
    pub as_organization: String,
    pub raw_data: Value,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyPoolIpHealthResponse {
    pub checked: usize,
    pub failed: usize,
    pub results: Vec<ProxyPoolIpHealthResult>,
    pub nodes: Vec<ProxyPoolNode>,
    pub groups: Vec<String>,
    pub sources: Vec<ProxySource>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyImportPreviewItem {
    pub preview_id: String,
    pub name: String,
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub group: String,
    pub source_kind: String,
    pub masked_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyImportPreviewResponse {
    pub items: Vec<ProxyImportPreviewItem>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyImportApplyResponse {
    pub imported: usize,
    pub skipped: usize,
    pub nodes: Vec<ProxyPoolNode>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySubscriptionApplyResponse {
    pub imported: usize,
    pub skipped: usize,
    pub nodes: Vec<ProxyPoolNode>,
    pub source: ProxySource,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySubscriptionRefreshItem {
    pub source_id: String,
    pub display_name: String,
    pub imported: usize,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySubscriptionRefreshResponse {
    pub refreshed: usize,
    pub failed: usize,
    pub nodes: Vec<ProxyPoolNode>,
    pub groups: Vec<String>,
    pub sources: Vec<ProxySource>,
    pub results: Vec<ProxySubscriptionRefreshItem>,
}
