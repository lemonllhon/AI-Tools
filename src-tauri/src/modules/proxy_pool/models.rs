use serde::{Deserialize, Serialize};

pub const DIRECT_NODE_ID: &str = "__direct__";
pub const LOCAL_NODE_ID: &str = "__local__";

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
