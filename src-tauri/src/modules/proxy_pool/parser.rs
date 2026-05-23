use super::models::{
    ProxyImportPreviewItem, ProxyImportPreviewRequest, ProxyImportPreviewResponse,
};
use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use url::Url;

const MAX_IMPORT_CONTENT_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_GROUP: &str = "默认";
const SUPPORTED_PROTOCOLS: &[&str] = &[
    "http",
    "https",
    "socks5",
    "vmess",
    "vless",
    "trojan",
    "ss",
    "hysteria",
    "hysteria2",
    "tuic",
    "anytls",
];

#[derive(Debug, Clone)]
pub struct ParsedImport {
    pub nodes: Vec<ParsedProxyNode>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedProxyNode {
    pub preview_id: String,
    pub name: String,
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub group: String,
    pub source_kind: String,
    pub raw_config: Value,
    pub standard_config: Value,
}

pub fn preview_import(
    request: &ProxyImportPreviewRequest,
) -> Result<ProxyImportPreviewResponse, String> {
    let parsed = parse_import_request(request)?;
    Ok(ProxyImportPreviewResponse {
        items: parsed
            .nodes
            .iter()
            .map(ParsedProxyNode::to_preview_item)
            .collect(),
        errors: parsed.errors,
    })
}

pub fn parse_import_request(request: &ProxyImportPreviewRequest) -> Result<ParsedImport, String> {
    let content = request.content.trim();
    if content.is_empty() {
        return Err("导入内容不能为空".to_string());
    }
    if content.len() > MAX_IMPORT_CONTENT_BYTES {
        return Err("导入内容不能超过 2 MB".to_string());
    }

    let group = normalize_optional_label(request.group.as_deref(), DEFAULT_GROUP, 80);
    let name_prefix = normalize_optional_label(request.name_prefix.as_deref(), "", 80);
    let mut nodes = Vec::new();
    let mut errors = Vec::new();
    let mut seen = HashSet::new();

    match parse_clash_yaml(content, &group, &name_prefix) {
        Ok((mut yaml_nodes, mut yaml_errors)) => {
            nodes.append(&mut yaml_nodes);
            errors.append(&mut yaml_errors);
        }
        Err(error) if looks_like_yaml(content) => errors.push(error),
        Err(_) => {}
    }

    for candidate in collect_link_candidates(content) {
        match parse_share_link(&candidate, &group, &name_prefix) {
            Ok(node) => nodes.push(node),
            Err(error) => errors.push(error),
        }
    }

    let mut unique_nodes = Vec::new();
    for mut node in nodes {
        let key = format!(
            "{}|{}|{}|{}",
            node.protocol,
            node.host.to_ascii_lowercase(),
            node.port,
            node.name
        );
        if !seen.insert(key) {
            continue;
        }
        node.preview_id = format!("import-{}", unique_nodes.len() + 1);
        unique_nodes.push(node);
    }

    if unique_nodes.is_empty() && errors.is_empty() {
        errors.push("未识别到可导入的代理节点".to_string());
    }

    Ok(ParsedImport {
        nodes: unique_nodes,
        errors,
    })
}

impl ParsedProxyNode {
    fn to_preview_item(&self) -> ProxyImportPreviewItem {
        ProxyImportPreviewItem {
            preview_id: self.preview_id.clone(),
            name: self.name.clone(),
            protocol: self.protocol.clone(),
            host: self.host.clone(),
            port: self.port,
            group: self.group.clone(),
            source_kind: self.source_kind.clone(),
            masked_url: build_masked_url(&self.protocol, &self.host, self.port),
            latency_ms: None,
            latency_status: String::new(),
            ip_health: None,
            ip_health_summary: String::new(),
        }
    }
}

fn collect_link_candidates(content: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    collect_lines_with_scheme(content, &mut candidates);

    if let Some(decoded) = decode_base64_text(content) {
        collect_lines_with_scheme(&decoded, &mut candidates);
    }

    candidates
}

fn collect_lines_with_scheme(content: &str, candidates: &mut Vec<String>) {
    for line in content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        if line.contains("://") {
            candidates.push(line.trim_matches('"').trim_matches('\'').to_string());
        }
    }
}

fn parse_share_link(
    raw_link: &str,
    group: &str,
    name_prefix: &str,
) -> Result<ParsedProxyNode, String> {
    let scheme = raw_link
        .split_once("://")
        .map(|(scheme, _)| scheme.to_ascii_lowercase())
        .ok_or_else(|| format!("代理链接缺少协议: {}", raw_link))?;

    match scheme.as_str() {
        "http" | "https" | "socks5" => parse_standard_url(raw_link, &scheme, group, name_prefix),
        "vmess" => parse_vmess_url(raw_link, group, name_prefix),
        "vless" | "trojan" | "hysteria" | "hysteria2" | "tuic" | "anytls" => {
            parse_user_host_url(raw_link, &scheme, group, name_prefix)
        }
        "ss" => parse_ss_url(raw_link, group, name_prefix),
        _ => Err(format!("暂不支持的代理协议: {}", scheme)),
    }
}

fn parse_standard_url(
    raw_link: &str,
    protocol: &str,
    group: &str,
    name_prefix: &str,
) -> Result<ParsedProxyNode, String> {
    let url = Url::parse(raw_link).map_err(|err| format!("解析代理链接失败: {}", err))?;
    let host = url
        .host_str()
        .ok_or_else(|| "代理链接缺少主机地址".to_string())?
        .to_string();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "代理链接缺少端口".to_string())?;
    let username = decode_component(url.username());
    let password = url.password().map(decode_component).unwrap_or_default();
    let name = build_node_name(
        url.fragment().map(decode_component),
        protocol,
        &host,
        port,
        name_prefix,
    );
    let query = url_query_to_json(&url);
    let raw_config = json!({
        "format": "share-link",
        "link": raw_link,
        "query": query.clone(),
    });
    let standard_config = json!({
        "protocol": protocol,
        "host": host,
        "port": port,
        "username": username,
        "password": password,
        "query": query.clone(),
    });

    Ok(ParsedProxyNode {
        preview_id: String::new(),
        name,
        protocol: protocol.to_string(),
        host,
        port,
        username,
        password,
        group: group.to_string(),
        source_kind: "share-link".to_string(),
        raw_config,
        standard_config,
    })
}

fn parse_user_host_url(
    raw_link: &str,
    protocol: &str,
    group: &str,
    name_prefix: &str,
) -> Result<ParsedProxyNode, String> {
    let url = Url::parse(raw_link).map_err(|err| format!("解析代理链接失败: {}", err))?;
    let host = url
        .host_str()
        .ok_or_else(|| "代理链接缺少主机地址".to_string())?
        .to_string();
    let port = url
        .port()
        .ok_or_else(|| "代理链接缺少端口".to_string())?;
    let user_secret = decode_component(url.username());
    let password = url.password().map(decode_component).unwrap_or_default();
    let query = url_query_to_json(&url);
    let name = build_node_name(
        url.fragment().map(decode_component),
        protocol,
        &host,
        port,
        name_prefix,
    );
    let raw_config = json!({
        "format": "share-link",
        "link": raw_link,
        "query": query.clone(),
    });
    let standard_config = json!({
        "protocol": protocol,
        "host": host,
        "port": port,
        "id": user_secret,
        "password": password,
        "query": query.clone(),
    });

    Ok(ParsedProxyNode {
        preview_id: String::new(),
        name,
        protocol: protocol.to_string(),
        host,
        port,
        username: user_secret,
        password,
        group: group.to_string(),
        source_kind: "share-link".to_string(),
        raw_config,
        standard_config,
    })
}

fn parse_vmess_url(
    raw_link: &str,
    group: &str,
    name_prefix: &str,
) -> Result<ParsedProxyNode, String> {
    let payload = raw_link
        .strip_prefix("vmess://")
        .ok_or_else(|| "vmess 链接格式错误".to_string())?;
    let decoded =
        decode_base64_text(payload).ok_or_else(|| "vmess 链接 Base64 解码失败".to_string())?;
    let value: Value =
        serde_json::from_str(&decoded).map_err(|err| format!("vmess JSON 解析失败: {}", err))?;
    let host = json_string(&value, "add").ok_or_else(|| "vmess 缺少 add 字段".to_string())?;
    let port = json_u16(&value, "port").ok_or_else(|| "vmess 缺少 port 字段".to_string())?;
    let id = json_string(&value, "id").unwrap_or_default();
    let name = build_node_name(
        json_string(&value, "ps"),
        "vmess",
        &host,
        port,
        name_prefix,
    );
    let raw_config = json!({
        "format": "share-link",
        "link": raw_link,
        "vmess": value.clone(),
    });
    let standard_config = json!({
        "protocol": "vmess",
        "host": host,
        "port": port,
        "id": id,
        "options": value.clone(),
    });

    Ok(ParsedProxyNode {
        preview_id: String::new(),
        name,
        protocol: "vmess".to_string(),
        host,
        port,
        username: id,
        password: String::new(),
        group: group.to_string(),
        source_kind: "share-link".to_string(),
        raw_config,
        standard_config,
    })
}

fn parse_ss_url(
    raw_link: &str,
    group: &str,
    name_prefix: &str,
) -> Result<ParsedProxyNode, String> {
    let url = Url::parse(raw_link).map_err(|err| format!("解析 ss 链接失败: {}", err))?;
    let host = url
        .host_str()
        .ok_or_else(|| "ss 链接缺少主机地址".to_string())?
        .to_string();
    let port = url.port().ok_or_else(|| "ss 链接缺少端口".to_string())?;
    let user_info = decode_ss_user_info(raw_link, &url)?;
    let (method, password) = user_info
        .split_once(':')
        .ok_or_else(|| "ss 链接缺少加密方式或密码".to_string())?;
    let query = url_query_to_json(&url);
    let name = build_node_name(
        url.fragment().map(decode_component),
        "ss",
        &host,
        port,
        name_prefix,
    );
    let raw_config = json!({
        "format": "share-link",
        "link": raw_link,
        "query": query.clone(),
    });
    let standard_config = json!({
        "protocol": "ss",
        "host": host,
        "port": port,
        "method": method,
        "password": password,
        "query": query.clone(),
    });

    Ok(ParsedProxyNode {
        preview_id: String::new(),
        name,
        protocol: "ss".to_string(),
        host,
        port,
        username: method.to_string(),
        password: password.to_string(),
        group: group.to_string(),
        source_kind: "share-link".to_string(),
        raw_config,
        standard_config,
    })
}

fn decode_ss_user_info(raw_link: &str, url: &Url) -> Result<String, String> {
    let username = url.username();
    if username.contains(':') {
        return Ok(decode_component(username));
    }
    if let Some(decoded) = decode_base64_text(username) {
        return Ok(decoded);
    }

    let without_scheme = raw_link
        .strip_prefix("ss://")
        .ok_or_else(|| "ss 链接格式错误".to_string())?;
    let before_fragment = without_scheme.split('#').next().unwrap_or(without_scheme);
    let before_query = before_fragment.split('?').next().unwrap_or(before_fragment);
    if let Some((encoded_user, _server)) = before_query.rsplit_once('@') {
        if let Some(decoded) = decode_base64_text(encoded_user) {
            return Ok(decoded);
        }
    }

    Err("ss 链接用户信息解码失败".to_string())
}

fn parse_clash_yaml(
    content: &str,
    group: &str,
    name_prefix: &str,
) -> Result<(Vec<ParsedProxyNode>, Vec<String>), String> {
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(content).map_err(|err| format!("Clash YAML 解析失败: {}", err))?;
    let proxies_key = serde_yaml::Value::String("proxies".to_string());
    let proxies = yaml
        .as_mapping()
        .and_then(|mapping| mapping.get(&proxies_key))
        .and_then(serde_yaml::Value::as_sequence)
        .ok_or_else(|| "Clash YAML 缺少 proxies 数组".to_string())?;

    let mut nodes = Vec::new();
    let mut errors = Vec::new();
    for (index, item) in proxies.iter().enumerate() {
        match parse_clash_proxy(item, group, name_prefix) {
            Ok(node) => nodes.push(node),
            Err(error) => {
                let label = yaml_lookup_str(item, "name").unwrap_or_else(|| format!("#{}", index + 1));
                errors.push(format!("Clash 节点 {} 解析失败: {}", label, error));
            }
        }
    }

    Ok((nodes, errors))
}

fn parse_clash_proxy(
    item: &serde_yaml::Value,
    group: &str,
    name_prefix: &str,
) -> Result<ParsedProxyNode, String> {
    let protocol = yaml_lookup_str(item, "type")
        .ok_or_else(|| "缺少 type".to_string())?
        .to_ascii_lowercase();
    if !SUPPORTED_PROTOCOLS.contains(&protocol.as_str()) {
        return Err(format!("暂不支持的协议 {}", protocol));
    }

    let host = yaml_lookup_str(item, "server").ok_or_else(|| "缺少 server".to_string())?;
    let port = yaml_lookup_u16(item, "port").ok_or_else(|| "缺少 port".to_string())?;
    let base_name = yaml_lookup_str(item, "name");
    let name = build_node_name(base_name, &protocol, &host, port, name_prefix);
    let username = yaml_lookup_str(item, "username")
        .or_else(|| yaml_lookup_str(item, "user"))
        .or_else(|| yaml_lookup_str(item, "uuid"))
        .unwrap_or_default();
    let password = yaml_lookup_str(item, "password")
        .or_else(|| yaml_lookup_str(item, "passwd"))
        .unwrap_or_default();
    let raw_json = serde_json::to_value(item).unwrap_or_else(|_| json!({}));
    let standard_config = json!({
        "protocol": protocol,
        "host": host,
        "port": port,
        "username": username,
        "password": password,
        "options": raw_json.clone(),
    });

    Ok(ParsedProxyNode {
        preview_id: String::new(),
        name,
        protocol,
        host,
        port,
        username,
        password,
        group: group.to_string(),
        source_kind: "clash-yaml".to_string(),
        raw_config: json!({
            "format": "clash-yaml",
            "proxy": raw_json.clone(),
        }),
        standard_config,
    })
}

fn yaml_lookup_str(value: &serde_yaml::Value, key: &str) -> Option<String> {
    let mapping = value.as_mapping()?;
    let key = serde_yaml::Value::String(key.to_string());
    let raw = mapping.get(&key)?;
    match raw {
        serde_yaml::Value::String(value) => Some(value.trim().to_string()).filter(|value| !value.is_empty()),
        serde_yaml::Value::Number(value) => Some(value.to_string()),
        serde_yaml::Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn yaml_lookup_u16(value: &serde_yaml::Value, key: &str) -> Option<u16> {
    let raw = yaml_lookup_str(value, key)?;
    raw.parse::<u16>().ok().filter(|port| *port > 0)
}

fn looks_like_yaml(content: &str) -> bool {
    content.contains("proxies:") || content.contains("proxy-providers:")
}

fn url_query_to_json(url: &Url) -> Value {
    let mut map = Map::new();
    for (key, value) in url.query_pairs() {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
    Value::Object(map)
}

fn json_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|raw| match raw {
        Value::String(value) => Some(value.trim().to_string()).filter(|value| !value.is_empty()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    })
}

fn json_u16(value: &Value, key: &str) -> Option<u16> {
    json_string(value, key)?.parse::<u16>().ok().filter(|port| *port > 0)
}

fn decode_base64_text(input: &str) -> Option<String> {
    let compact: String = input.chars().filter(|ch| !ch.is_whitespace()).collect();
    if compact.is_empty() {
        return None;
    }

    let padded = pad_base64(&compact);
    for engine in [
        &general_purpose::STANDARD,
        &general_purpose::STANDARD_NO_PAD,
        &general_purpose::URL_SAFE,
        &general_purpose::URL_SAFE_NO_PAD,
    ] {
        if let Ok(bytes) = engine.decode(&padded) {
            if let Ok(text) = String::from_utf8(bytes) {
                return Some(text);
            }
        }
        if let Ok(bytes) = engine.decode(&compact) {
            if let Ok(text) = String::from_utf8(bytes) {
                return Some(text);
            }
        }
    }
    None
}

fn pad_base64(input: &str) -> String {
    let remainder = input.len() % 4;
    if remainder == 0 {
        input.to_string()
    } else {
        format!("{}{}", input, "=".repeat(4 - remainder))
    }
}

fn decode_component(input: &str) -> String {
    urlencoding::decode(input)
        .map(|value| value.into_owned())
        .unwrap_or_else(|_| input.to_string())
}

fn build_node_name(
    explicit_name: Option<String>,
    protocol: &str,
    host: &str,
    port: u16,
    name_prefix: &str,
) -> String {
    let fallback = format!("{} {}:{}", protocol, host, port);
    let base = explicit_name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback);
    if name_prefix.is_empty() {
        base
    } else {
        format!("{}{}", name_prefix, base)
    }
}

fn normalize_optional_label(value: Option<&str>, fallback: &str, max_chars: usize) -> String {
    let trimmed = value.unwrap_or_default().trim();
    if trimmed.is_empty() {
        return fallback.to_string();
    }
    trimmed.chars().take(max_chars).collect()
}

fn build_masked_url(protocol: &str, host: &str, port: u16) -> String {
    format!("{}://{}:{}", protocol, host, port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_base64_subscription_lines() {
        let encoded = general_purpose::STANDARD.encode("http://user:pass@example.com:8080#demo");
        let parsed = parse_import_request(&ProxyImportPreviewRequest {
            content: encoded,
            group: Some("G".to_string()),
            name_prefix: Some("P-".to_string()),
        })
        .expect("parse base64");

        assert_eq!(parsed.nodes.len(), 1);
        assert_eq!(parsed.nodes[0].name, "P-demo");
        assert_eq!(parsed.nodes[0].protocol, "http");
        assert_eq!(parsed.nodes[0].password, "pass");
    }

    #[test]
    fn parses_clash_yaml_proxy() {
        let parsed = parse_import_request(&ProxyImportPreviewRequest {
            content: r#"
proxies:
  - name: hk
    type: socks5
    server: 127.0.0.1
    port: 7890
"#
            .to_string(),
            group: None,
            name_prefix: None,
        })
        .expect("parse clash yaml");

        assert_eq!(parsed.nodes.len(), 1);
        assert_eq!(parsed.nodes[0].protocol, "socks5");
        assert_eq!(parsed.nodes[0].host, "127.0.0.1");
    }
}
