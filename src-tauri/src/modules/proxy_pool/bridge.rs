use super::runtime;
use super::store::GatewayOutboundTarget;
use crate::modules::data_dir;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::Mutex as TokioMutex;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const BRIDGE_BIND_HOST: &str = "127.0.0.1";
const BRIDGE_DIR_NAME: &str = "bridge";
const XRAY_RUNTIME: &str = "xray";
const SING_BOX_RUNTIME: &str = "sing-box";
const BRIDGE_READY_TIMEOUT: Duration = Duration::from_secs(8);
const BRIDGE_READY_INTERVAL: Duration = Duration::from_millis(120);

static BRIDGE_RUNTIME: OnceLock<TokioMutex<BridgeRuntime>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct BridgeEndpoint {
    pub host: String,
    pub port: u16,
}

#[derive(Default)]
struct BridgeRuntime {
    child: Option<Child>,
    key: String,
    endpoint: Option<BridgeEndpoint>,
    config_path: Option<PathBuf>,
}

enum BridgeRuntimeKind {
    Xray,
    SingBox,
}

fn bridge_runtime() -> &'static TokioMutex<BridgeRuntime> {
    BRIDGE_RUNTIME.get_or_init(|| TokioMutex::new(BridgeRuntime::default()))
}

pub fn is_bridge_protocol(protocol: &str) -> bool {
    matches!(
        protocol.to_ascii_lowercase().as_str(),
        "vmess" | "vless" | "trojan" | "ss" | "hysteria" | "hysteria2" | "tuic" | "anytls"
    )
}

pub async fn ensure_bridge(target: &GatewayOutboundTarget) -> Result<BridgeEndpoint, String> {
    let kind = runtime_kind_for_protocol(&target.protocol)?;
    let key = bridge_key(target);
    let mut runtime_state = bridge_runtime().lock().await;

    if runtime_state.key == key {
        if let Some(endpoint) = runtime_state.endpoint.clone() {
            if endpoint_is_ready(&endpoint).await {
                return Ok(endpoint);
            }
        }
    }

    stop_bridge_locked(&mut runtime_state);

    let port = reserve_local_port()?;
    let endpoint = BridgeEndpoint {
        host: BRIDGE_BIND_HOST.to_string(),
        port,
    };
    let config = match &kind {
        BridgeRuntimeKind::Xray => build_xray_config(target, port)?,
        BridgeRuntimeKind::SingBox => build_sing_box_config(target, port)?,
    };
    let config_path = write_bridge_config(target, runtime_name(&kind), &key, &config)?;
    let binary_path = resolve_runtime_binary(runtime_name(&kind)).await?;
    let child = spawn_bridge_process(&binary_path, &config_path, &kind)?;

    runtime_state.child = Some(child);
    runtime_state.key = key;
    runtime_state.endpoint = Some(endpoint.clone());
    runtime_state.config_path = Some(config_path);

    if let Err(error) = wait_for_bridge_ready(&mut runtime_state, &endpoint).await {
        stop_bridge_locked(&mut runtime_state);
        return Err(error);
    }

    Ok(endpoint)
}

pub async fn stop_bridge() {
    let mut runtime_state = bridge_runtime().lock().await;
    stop_bridge_locked(&mut runtime_state);
}

fn stop_bridge_locked(runtime_state: &mut BridgeRuntime) {
    if let Some(mut child) = runtime_state.child.take() {
        match child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
    runtime_state.key.clear();
    runtime_state.endpoint = None;
    runtime_state.config_path = None;
}

async fn wait_for_bridge_ready(
    runtime_state: &mut BridgeRuntime,
    endpoint: &BridgeEndpoint,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + BRIDGE_READY_TIMEOUT;
    loop {
        if endpoint_is_ready(endpoint).await {
            return Ok(());
        }
        if let Some(child) = runtime_state.child.as_mut() {
            if let Some(status) = child
                .try_wait()
                .map_err(|err| format!("检查代理桥接进程失败: {}", err))?
            {
                return Err(format!("代理桥接进程已退出: {}", status));
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "代理桥接启动超时: socks5://{}:{}",
                endpoint.host, endpoint.port
            ));
        }
        tokio::time::sleep(BRIDGE_READY_INTERVAL).await;
    }
}

async fn endpoint_is_ready(endpoint: &BridgeEndpoint) -> bool {
    matches!(
        tokio::time::timeout(
            Duration::from_millis(350),
            TcpStream::connect((endpoint.host.as_str(), endpoint.port)),
        )
        .await,
        Ok(Ok(_))
    )
}

async fn resolve_runtime_binary(runtime_name: &'static str) -> Result<PathBuf, String> {
    let app = crate::get_app_handle()
        .cloned()
        .ok_or_else(|| "代理内核尚未初始化，请重启应用后重试".to_string())?;
    tokio::task::spawn_blocking(move || runtime::ensure_runtime_binary(&app, runtime_name))
        .await
        .map_err(|err| format!("加载代理内核失败: {}", err))?
}

fn spawn_bridge_process(
    binary_path: &PathBuf,
    config_path: &PathBuf,
    kind: &BridgeRuntimeKind,
) -> Result<Child, String> {
    let mut command = Command::new(binary_path);
    match kind {
        BridgeRuntimeKind::Xray => {
            command.arg("run").arg("-config").arg(config_path);
        }
        BridgeRuntimeKind::SingBox => {
            command.arg("run").arg("-c").arg(config_path);
        }
    }
    command.stdin(Stdio::null());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    command
        .spawn()
        .map_err(|err| format!("启动代理桥接内核失败 {}: {}", binary_path.display(), err))
}

fn reserve_local_port() -> Result<u16, String> {
    let listener = TcpListener::bind((BRIDGE_BIND_HOST, 0))
        .map_err(|err| format!("分配代理桥接端口失败: {}", err))?;
    let addr: SocketAddr = listener
        .local_addr()
        .map_err(|err| format!("读取代理桥接端口失败: {}", err))?;
    Ok(addr.port())
}

fn write_bridge_config(
    target: &GatewayOutboundTarget,
    runtime_name: &str,
    key: &str,
    config: &Value,
) -> Result<PathBuf, String> {
    let dir = data_dir::get_data_dir()?
        .join("proxy-pool")
        .join(BRIDGE_DIR_NAME);
    fs::create_dir_all(&dir)
        .map_err(|err| format!("创建代理桥接配置目录失败 {}: {}", dir.display(), err))?;
    let file_name = format!(
        "{}-{}-{}.json",
        runtime_name,
        sanitize_file_part(&target.id),
        &key[..12]
    );
    let path = dir.join(file_name);
    let content = serde_json::to_string_pretty(config)
        .map_err(|err| format!("生成代理桥接配置失败: {}", err))?;
    fs::write(&path, content)
        .map_err(|err| format!("写入代理桥接配置失败 {}: {}", path.display(), err))?;
    Ok(path)
}

fn sanitize_file_part(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    if sanitized.is_empty() {
        "node".to_string()
    } else {
        sanitized
    }
}

fn bridge_key(target: &GatewayOutboundTarget) -> String {
    let mut hasher = Sha256::new();
    hasher.update(target.id.as_bytes());
    hasher.update(target.protocol.as_bytes());
    hasher.update(target.host.as_bytes());
    hasher.update(target.port.to_be_bytes());
    hasher.update(target.username.as_bytes());
    hasher.update(target.password.as_bytes());
    hasher.update(target.standard_config.to_string().as_bytes());
    format!("{:x}", hasher.finalize())
}

fn runtime_kind_for_protocol(protocol: &str) -> Result<BridgeRuntimeKind, String> {
    match protocol.to_ascii_lowercase().as_str() {
        "vmess" | "vless" | "trojan" | "ss" => Ok(BridgeRuntimeKind::Xray),
        "hysteria" | "hysteria2" | "tuic" | "anytls" => Ok(BridgeRuntimeKind::SingBox),
        other => Err(format!("{} 协议不需要或暂不支持内核桥接", other)),
    }
}

fn runtime_name(kind: &BridgeRuntimeKind) -> &'static str {
    match kind {
        BridgeRuntimeKind::Xray => XRAY_RUNTIME,
        BridgeRuntimeKind::SingBox => SING_BOX_RUNTIME,
    }
}

fn build_xray_config(target: &GatewayOutboundTarget, local_port: u16) -> Result<Value, String> {
    Ok(json!({
        "log": {
            "loglevel": "warning"
        },
        "inbounds": [{
            "tag": "socks-in",
            "listen": BRIDGE_BIND_HOST,
            "port": local_port,
            "protocol": "socks",
            "settings": {
                "auth": "noauth",
                "udp": true
            }
        }],
        "outbounds": [
            build_xray_outbound(target)?
        ]
    }))
}

fn build_xray_outbound(target: &GatewayOutboundTarget) -> Result<Value, String> {
    match target.protocol.to_ascii_lowercase().as_str() {
        "vmess" => {
            let id = first_text(
                target,
                &["id", "username", "options.id", "options.uuid"],
            )
            .unwrap_or_else(|| target.username.clone());
            if id.trim().is_empty() {
                return Err("vmess 节点缺少 UUID".to_string());
            }
            let alter_id = first_i64(target, &["alterId", "alter_id", "options.aid", "options.alterId"])
                .or_else(|| first_text(target, &["options.aid"]).and_then(|value| value.parse().ok()))
                .unwrap_or(0);
            let security = first_text(
                target,
                &["security", "cipher", "options.scy", "options.cipher"],
            )
            .unwrap_or_else(|| "auto".to_string());
            let mut user = Map::new();
            user.insert("id".to_string(), Value::String(id));
            user.insert("alterId".to_string(), json!(alter_id));
            if !security.trim().is_empty() {
                user.insert("security".to_string(), Value::String(security));
            }
            Ok(with_xray_stream_settings(
                target,
                json!({
                    "tag": "proxy",
                    "protocol": "vmess",
                    "settings": {
                        "vnext": [{
                            "address": target.host.clone(),
                            "port": target.port,
                            "users": [Value::Object(user)]
                        }]
                    }
                }),
            ))
        }
        "vless" => {
            let id = first_text(
                target,
                &["id", "username", "options.uuid", "options.id"],
            )
            .unwrap_or_else(|| target.username.clone());
            if id.trim().is_empty() {
                return Err("vless 节点缺少 UUID".to_string());
            }
            let mut user = Map::new();
            user.insert("id".to_string(), Value::String(id));
            user.insert(
                "encryption".to_string(),
                Value::String(
                    first_text(target, &["encryption", "query.encryption"])
                        .unwrap_or_else(|| "none".to_string()),
                ),
            );
            if let Some(flow) = first_text(target, &["flow", "query.flow", "options.flow"]) {
                if !flow.trim().is_empty() {
                    user.insert("flow".to_string(), Value::String(flow));
                }
            }
            Ok(with_xray_stream_settings(
                target,
                json!({
                    "tag": "proxy",
                    "protocol": "vless",
                    "settings": {
                        "vnext": [{
                            "address": target.host.clone(),
                            "port": target.port,
                            "users": [Value::Object(user)]
                        }]
                    }
                }),
            ))
        }
        "trojan" => {
            let password = first_text(
                target,
                &["password", "id", "username", "options.password"],
            )
            .unwrap_or_else(|| {
                if target.password.is_empty() {
                    target.username.clone()
                } else {
                    target.password.clone()
                }
            });
            if password.trim().is_empty() {
                return Err("trojan 节点缺少密码".to_string());
            }
            Ok(with_xray_stream_settings(
                target,
                json!({
                    "tag": "proxy",
                    "protocol": "trojan",
                    "settings": {
                        "servers": [{
                            "address": target.host.clone(),
                            "port": target.port,
                            "password": password
                        }]
                    }
                }),
            ))
        }
        "ss" => {
            let method = first_text(target, &["method", "cipher", "options.cipher"])
                .unwrap_or_else(|| target.username.clone());
            let password = first_text(target, &["password", "options.password"])
                .unwrap_or_else(|| target.password.clone());
            if method.trim().is_empty() || password.trim().is_empty() {
                return Err("shadowsocks 节点缺少加密方式或密码".to_string());
            }
            Ok(json!({
                "tag": "proxy",
                "protocol": "shadowsocks",
                "settings": {
                    "servers": [{
                        "address": target.host.clone(),
                        "port": target.port,
                        "method": method,
                        "password": password,
                        "udp": true
                    }]
                }
            }))
        }
        other => Err(format!("xray 暂不支持 {} 协议桥接", other)),
    }
}

fn with_xray_stream_settings(target: &GatewayOutboundTarget, mut outbound: Value) -> Value {
    let stream = build_xray_stream_settings(target);
    if !stream.is_empty() {
        if let Some(object) = outbound.as_object_mut() {
            object.insert("streamSettings".to_string(), Value::Object(stream));
        }
    }
    outbound
}

fn build_xray_stream_settings(target: &GatewayOutboundTarget) -> Map<String, Value> {
    let mut stream = Map::new();
    let network = first_text(
        target,
        &[
            "network",
            "type",
            "query.type",
            "options.network",
            "options.net",
            "options.type",
        ],
    )
    .unwrap_or_else(|| "tcp".to_string())
    .to_ascii_lowercase();
    let network = match network.as_str() {
        "h2" => "http".to_string(),
        other => other.to_string(),
    };
    if network != "tcp" {
        stream.insert("network".to_string(), Value::String(network.clone()));
    }

    let security = first_text(
        target,
        &[
            "security",
            "query.security",
            "options.tls",
            "options.security",
        ],
    )
    .unwrap_or_default()
    .to_ascii_lowercase();
    let tls_enabled = security == "tls"
        || first_bool(target, &["tls", "options.tls"]).unwrap_or(false);
    let reality_enabled = security == "reality";
    if reality_enabled {
        stream.insert("security".to_string(), Value::String("reality".to_string()));
        let mut reality = Map::new();
        insert_if_present(&mut reality, "serverName", first_server_name(target));
        insert_if_present(
            &mut reality,
            "fingerprint",
            first_text(target, &["fp", "query.fp", "options.client-fingerprint"]),
        );
        insert_if_present(
            &mut reality,
            "publicKey",
            first_text(target, &["pbk", "query.pbk", "options.reality-opts.public-key"]),
        );
        insert_if_present(
            &mut reality,
            "shortId",
            first_text(target, &["sid", "query.sid", "options.reality-opts.short-id"]),
        );
        insert_if_present(
            &mut reality,
            "spiderX",
            first_text(target, &["spx", "query.spx"]),
        );
        stream.insert("realitySettings".to_string(), Value::Object(reality));
    } else if tls_enabled {
        stream.insert("security".to_string(), Value::String("tls".to_string()));
        let mut tls = Map::new();
        insert_if_present(&mut tls, "serverName", first_server_name(target));
        if first_bool(target, &["allowInsecure", "skip-cert-verify", "options.skip-cert-verify"])
            .unwrap_or(false)
        {
            tls.insert("allowInsecure".to_string(), Value::Bool(true));
        }
        if let Some(alpn) = first_text(target, &["alpn", "query.alpn", "options.alpn"]) {
            let values = alpn
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| Value::String(value.to_string()))
                .collect::<Vec<_>>();
            if !values.is_empty() {
                tls.insert("alpn".to_string(), Value::Array(values));
            }
        }
        stream.insert("tlsSettings".to_string(), Value::Object(tls));
    }

    match network.as_str() {
        "ws" => {
            let mut ws = Map::new();
            insert_if_present(
                &mut ws,
                "path",
                first_text(
                    target,
                    &["path", "query.path", "options.path", "options.ws-opts.path"],
                ),
            );
            let host = first_text(
                target,
                &["host", "query.host", "options.ws-opts.headers.Host"],
            );
            if let Some(host) = host.filter(|value| !value.trim().is_empty()) {
                ws.insert("headers".to_string(), json!({ "Host": host }));
            }
            stream.insert("wsSettings".to_string(), Value::Object(ws));
        }
        "grpc" => {
            let mut grpc = Map::new();
            insert_if_present(
                &mut grpc,
                "serviceName",
                first_text(
                    target,
                    &[
                        "serviceName",
                        "query.serviceName",
                        "query.service_name",
                        "options.grpc-opts.grpc-service-name",
                    ],
                ),
            );
            stream.insert("grpcSettings".to_string(), Value::Object(grpc));
        }
        "http" => {
            let mut http = Map::new();
            if let Some(path) = first_text(target, &["path", "query.path", "options.h2-opts.path"]) {
                http.insert("path".to_string(), Value::Array(vec![Value::String(path)]));
            }
            if let Some(host) = first_text(target, &["host", "query.host", "options.h2-opts.host"]) {
                http.insert("host".to_string(), Value::Array(vec![Value::String(host)]));
            }
            stream.insert("httpSettings".to_string(), Value::Object(http));
        }
        _ => {}
    }

    stream
}

fn build_sing_box_config(target: &GatewayOutboundTarget, local_port: u16) -> Result<Value, String> {
    Ok(json!({
        "log": {
            "level": "warn"
        },
        "inbounds": [{
            "type": "socks",
            "tag": "socks-in",
            "listen": BRIDGE_BIND_HOST,
            "listen_port": local_port
        }],
        "outbounds": [
            build_sing_box_outbound(target)?
        ]
    }))
}

fn build_sing_box_outbound(target: &GatewayOutboundTarget) -> Result<Value, String> {
    match target.protocol.to_ascii_lowercase().as_str() {
        "hysteria2" => {
            let password = node_secret(target);
            if password.trim().is_empty() {
                return Err("hysteria2 节点缺少密码".to_string());
            }
            let mut outbound = base_sing_box_outbound("hysteria2", target);
            outbound.insert("password".to_string(), Value::String(password));
            if let Some(obfs_password) =
                first_text(target, &["obfs-password", "query.obfs-password", "options.obfs-password"])
            {
                outbound.insert(
                    "obfs".to_string(),
                    json!({
                        "type": "salamander",
                        "password": obfs_password
                    }),
                );
            }
            outbound.insert("tls".to_string(), Value::Object(build_sing_box_tls(target)));
            Ok(Value::Object(outbound))
        }
        "tuic" => {
            let uuid = first_text(target, &["uuid", "id", "username", "options.uuid"])
                .unwrap_or_else(|| target.username.clone());
            let password = first_text(target, &["password", "query.password", "options.password"])
                .unwrap_or_else(|| target.password.clone());
            if uuid.trim().is_empty() || password.trim().is_empty() {
                return Err("tuic 节点缺少 uuid 或 password".to_string());
            }
            let mut outbound = base_sing_box_outbound("tuic", target);
            outbound.insert("uuid".to_string(), Value::String(uuid));
            outbound.insert("password".to_string(), Value::String(password));
            insert_if_present(
                &mut outbound,
                "congestion_control",
                first_text(
                    target,
                    &["congestion-control", "query.congestion_control", "options.congestion-controller"],
                ),
            );
            outbound.insert("tls".to_string(), Value::Object(build_sing_box_tls(target)));
            Ok(Value::Object(outbound))
        }
        "anytls" => {
            let password = node_secret(target);
            if password.trim().is_empty() {
                return Err("anytls 节点缺少密码".to_string());
            }
            let mut outbound = base_sing_box_outbound("anytls", target);
            outbound.insert("password".to_string(), Value::String(password));
            outbound.insert("tls".to_string(), Value::Object(build_sing_box_tls(target)));
            Ok(Value::Object(outbound))
        }
        "hysteria" => {
            let auth = node_secret(target);
            if auth.trim().is_empty() {
                return Err("hysteria 节点缺少认证信息".to_string());
            }
            let mut outbound = base_sing_box_outbound("hysteria", target);
            outbound.insert("auth_str".to_string(), Value::String(auth));
            if let Some(up) = first_i64(target, &["up", "upmbps", "options.up"]) {
                outbound.insert("up_mbps".to_string(), json!(up));
            }
            if let Some(down) = first_i64(target, &["down", "downmbps", "options.down"]) {
                outbound.insert("down_mbps".to_string(), json!(down));
            }
            outbound.insert("tls".to_string(), Value::Object(build_sing_box_tls(target)));
            Ok(Value::Object(outbound))
        }
        other => Err(format!("sing-box 暂不支持 {} 协议桥接", other)),
    }
}

fn base_sing_box_outbound(protocol: &str, target: &GatewayOutboundTarget) -> Map<String, Value> {
    let mut outbound = Map::new();
    outbound.insert("type".to_string(), Value::String(protocol.to_string()));
    outbound.insert("tag".to_string(), Value::String("proxy".to_string()));
    outbound.insert("server".to_string(), Value::String(target.host.clone()));
    outbound.insert("server_port".to_string(), json!(target.port));
    outbound
}

fn build_sing_box_tls(target: &GatewayOutboundTarget) -> Map<String, Value> {
    let mut tls = Map::new();
    tls.insert("enabled".to_string(), Value::Bool(true));
    insert_if_present(&mut tls, "server_name", first_server_name(target));
    if first_bool(target, &["insecure", "skip-cert-verify", "options.skip-cert-verify"])
        .unwrap_or(false)
    {
        tls.insert("insecure".to_string(), Value::Bool(true));
    }
    if let Some(alpn) = first_text(target, &["alpn", "query.alpn", "options.alpn"]) {
        let values = alpn
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| Value::String(value.to_string()))
            .collect::<Vec<_>>();
        if !values.is_empty() {
            tls.insert("alpn".to_string(), Value::Array(values));
        }
    }
    tls
}

fn first_server_name(target: &GatewayOutboundTarget) -> Option<String> {
    first_text(
        target,
        &[
            "sni",
            "servername",
            "serverName",
            "peer",
            "query.sni",
            "query.peer",
            "options.sni",
            "options.servername",
            "options.server-name",
        ],
    )
}

fn node_secret(target: &GatewayOutboundTarget) -> String {
    first_text(
        target,
        &[
            "password",
            "id",
            "auth",
            "auth-str",
            "username",
            "options.password",
            "options.auth-str",
        ],
    )
    .unwrap_or_else(|| {
        if target.password.is_empty() {
            target.username.clone()
        } else {
            target.password.clone()
        }
    })
}

fn first_text(target: &GatewayOutboundTarget, paths: &[&str]) -> Option<String> {
    for path in paths {
        if let Some(value) = text_at(&target.standard_config, path) {
            if !value.trim().is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn first_i64(target: &GatewayOutboundTarget, paths: &[&str]) -> Option<i64> {
    for path in paths {
        if let Some(value) = value_at(&target.standard_config, path) {
            match value {
                Value::Number(number) => {
                    if let Some(value) = number.as_i64() {
                        return Some(value);
                    }
                }
                Value::String(text) => {
                    let normalized = text
                        .trim()
                        .trim_end_matches("Mbps")
                        .trim_end_matches("mbps")
                        .trim();
                    if let Ok(value) = normalized.parse::<i64>() {
                        return Some(value);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn first_bool(target: &GatewayOutboundTarget, paths: &[&str]) -> Option<bool> {
    for path in paths {
        if let Some(value) = value_at(&target.standard_config, path) {
            match value {
                Value::Bool(value) => return Some(*value),
                Value::String(text) => {
                    let normalized = text.trim().to_ascii_lowercase();
                    match normalized.as_str() {
                        "1" | "true" | "yes" | "tls" => return Some(true),
                        "0" | "false" | "no" | "none" => return Some(false),
                        _ => {}
                    }
                }
                Value::Number(number) => {
                    if let Some(value) = number.as_i64() {
                        return Some(value != 0);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn value_at<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for key in path.split('.') {
        current = current.get(key)?;
    }
    Some(current)
}

fn text_at(value: &Value, path: &str) -> Option<String> {
    match value_at(value, path)? {
        Value::String(text) => Some(text.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn insert_if_present(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        map.insert(key.to_string(), Value::String(value));
    }
}
