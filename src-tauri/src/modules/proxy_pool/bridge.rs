use super::runtime;
use super::store::GatewayOutboundTarget;
use crate::modules::data_dir;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
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
const MIHOMO_RUNTIME: &str = "mihomo";
const MIHOMO_PROXY_NAME: &str = "selected-node";
const MIHOMO_GROUP_NAME: &str = "proxy";
const BRIDGE_READY_TIMEOUT: Duration = Duration::from_secs(8);
const BRIDGE_READY_INTERVAL: Duration = Duration::from_millis(120);
const MIHOMO_PROVIDER_READY_GRACE: Duration = Duration::from_millis(1800);

static BRIDGE_RUNTIME: OnceLock<TokioMutex<BridgeRuntime>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct BridgeEndpoint {
    pub host: String,
    pub port: u16,
}

pub struct TemporaryBridge {
    child: Option<Child>,
    endpoint: BridgeEndpoint,
    config_path: Option<PathBuf>,
    log_path: Option<PathBuf>,
}

impl TemporaryBridge {
    pub fn endpoint(&self) -> &BridgeEndpoint {
        &self.endpoint
    }

    pub fn log_snippet(&self) -> Option<String> {
        read_bridge_log_snippet(self.log_path.as_ref())
    }
}

impl Drop for TemporaryBridge {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
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
        if let Some(path) = self.config_path.take() {
            remove_bridge_file_and_empty_parent(path);
        }
        if let Some(path) = self.log_path.take() {
            remove_bridge_file_and_empty_parent(path);
        }
    }
}

#[derive(Default)]
struct BridgeRuntime {
    child: Option<Child>,
    key: String,
    endpoint: Option<BridgeEndpoint>,
    config_path: Option<PathBuf>,
    log_path: Option<PathBuf>,
}

enum BridgeRuntimeKind {
    Xray,
    SingBox,
    Mihomo,
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
        BridgeRuntimeKind::Mihomo => build_mihomo_config(target, port)?,
    };
    let config_path = write_bridge_config(target, runtime_name(&kind), &key, &config)?;
    let log_path = bridge_log_path(&config_path);
    let binary_path = resolve_runtime_binary(runtime_name(&kind)).await?;
    crate::modules::logger::log_info(&format!(
        "[ProxyBridge] 启动内置桥接: runtime={}, node={} ({}), listen=socks5://{}:{}, binary={}, config={}",
        runtime_name(&kind),
        target.name,
        target.protocol,
        endpoint.host,
        endpoint.port,
        binary_path.display(),
        config_path.display()
    ));
    let child = spawn_bridge_process(&binary_path, &config_path, &log_path, &kind)?;

    runtime_state.child = Some(child);
    runtime_state.key = key;
    runtime_state.endpoint = Some(endpoint.clone());
    runtime_state.config_path = Some(config_path);
    runtime_state.log_path = Some(log_path);

    if let Err(error) = wait_for_bridge_ready(&mut runtime_state, &endpoint).await {
        stop_bridge_locked(&mut runtime_state);
        return Err(error);
    }
    wait_for_runtime_provider_ready(&kind).await;

    crate::modules::logger::log_info(&format!(
        "[ProxyBridge] 内置桥接已就绪: runtime={}, node={} ({}), listen=socks5://{}:{}",
        runtime_name(&kind),
        target.name,
        target.protocol,
        endpoint.host,
        endpoint.port
    ));
    Ok(endpoint)
}

pub async fn start_temporary_bridge(
    target: &GatewayOutboundTarget,
) -> Result<TemporaryBridge, String> {
    let kind = runtime_kind_for_protocol(&target.protocol)?;
    let port = reserve_local_port()?;
    let endpoint = BridgeEndpoint {
        host: BRIDGE_BIND_HOST.to_string(),
        port,
    };
    let config = match &kind {
        BridgeRuntimeKind::Xray => build_xray_config(target, port)?,
        BridgeRuntimeKind::SingBox => build_sing_box_config(target, port)?,
        BridgeRuntimeKind::Mihomo => build_mihomo_config(target, port)?,
    };
    let key = format!("{}-{}", bridge_key(target), port);
    let config_path = write_bridge_config(target, runtime_name(&kind), &key, &config)?;
    let log_path = bridge_log_path(&config_path);
    let binary_path = resolve_runtime_binary(runtime_name(&kind)).await?;
    crate::modules::logger::log_info(&format!(
        "[ProxyBridge] 启动临时桥接检测: runtime={}, node={} ({}), listen=socks5://{}:{}, binary={}, config={}",
        runtime_name(&kind),
        target.name,
        target.protocol,
        endpoint.host,
        endpoint.port,
        binary_path.display(),
        config_path.display()
    ));
    let child = spawn_bridge_process(&binary_path, &config_path, &log_path, &kind)?;
    let mut temporary = TemporaryBridge {
        child: Some(child),
        endpoint,
        config_path: Some(config_path),
        log_path: Some(log_path),
    };

    if let Err(error) = wait_for_temporary_bridge_ready(&mut temporary).await {
        drop(temporary);
        return Err(error);
    }
    wait_for_runtime_provider_ready(&kind).await;

    crate::modules::logger::log_info(&format!(
        "[ProxyBridge] 临时桥接检测已就绪: runtime={}, node={} ({}), listen=socks5://{}:{}",
        runtime_name(&kind),
        target.name,
        target.protocol,
        temporary.endpoint.host,
        temporary.endpoint.port
    ));
    Ok(temporary)
}

pub async fn stop_bridge() {
    let mut runtime_state = bridge_runtime().lock().await;
    stop_bridge_locked(&mut runtime_state);
}

pub async fn current_bridge_log_snippet() -> Option<String> {
    let runtime_state = bridge_runtime().lock().await;
    read_bridge_log_snippet(runtime_state.log_path.as_ref())
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
    if let Some(path) = runtime_state.config_path.take() {
        remove_bridge_file_and_empty_parent(path);
    }
    if let Some(path) = runtime_state.log_path.take() {
        remove_bridge_file_and_empty_parent(path);
    }
}

fn remove_bridge_file_and_empty_parent(path: PathBuf) {
    let parent = path.parent().map(PathBuf::from);
    let _ = fs::remove_file(&path);
    if let Some(parent) = parent {
        let _ = fs::remove_dir(parent);
    }
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
                return Err(format_bridge_exit_error(
                    status,
                    runtime_state.log_path.as_ref(),
                ));
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format_bridge_timeout_error(
                endpoint,
                runtime_state.log_path.as_ref(),
            ));
        }
        tokio::time::sleep(BRIDGE_READY_INTERVAL).await;
    }
}

async fn wait_for_temporary_bridge_ready(temporary: &mut TemporaryBridge) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + BRIDGE_READY_TIMEOUT;
    loop {
        if endpoint_is_ready(&temporary.endpoint).await {
            return Ok(());
        }
        if let Some(child) = temporary.child.as_mut() {
            if let Some(status) = child
                .try_wait()
                .map_err(|err| format!("检查代理桥接进程失败: {}", err))?
            {
                return Err(format_bridge_exit_error(
                    status,
                    temporary.log_path.as_ref(),
                ));
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format_bridge_timeout_error(
                &temporary.endpoint,
                temporary.log_path.as_ref(),
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

async fn wait_for_runtime_provider_ready(kind: &BridgeRuntimeKind) {
    if matches!(kind, BridgeRuntimeKind::Mihomo) {
        tokio::time::sleep(MIHOMO_PROVIDER_READY_GRACE).await;
    }
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
    log_path: &PathBuf,
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
        BridgeRuntimeKind::Mihomo => {
            if let Some(parent) = config_path.parent() {
                command.arg("-d").arg(parent);
            }
            command.arg("-f").arg(config_path);
        }
    }
    if let Some(parent) = config_path.parent() {
        command.current_dir(parent);
    }
    let stdout = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(log_path)
        .map_err(|err| format!("创建代理桥接日志失败 {}: {}", log_path.display(), err))?;
    let stderr = stdout
        .try_clone()
        .map_err(|err| format!("准备代理桥接日志失败 {}: {}", log_path.display(), err))?;
    command.stdin(Stdio::null());
    command.stdout(Stdio::from(stdout));
    command.stderr(Stdio::from(stderr));
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    command
        .spawn()
        .map_err(|err| format!("启动代理桥接内核失败 {}: {}", binary_path.display(), err))
}

fn bridge_log_path(config_path: &PathBuf) -> PathBuf {
    config_path.with_extension("log")
}

fn format_bridge_exit_error(status: ExitStatus, log_path: Option<&PathBuf>) -> String {
    match read_bridge_log_snippet(log_path) {
        Some(output) => format!("代理桥接进程已退出: {}; 内核输出: {}", status, output),
        None => format!("代理桥接进程已退出: {}", status),
    }
}

fn format_bridge_timeout_error(endpoint: &BridgeEndpoint, log_path: Option<&PathBuf>) -> String {
    let base = format!(
        "代理桥接启动超时: socks5://{}:{}",
        endpoint.host, endpoint.port
    );
    match read_bridge_log_snippet(log_path) {
        Some(output) => format!("{}; 内核输出: {}", base, output),
        None => base,
    }
}

fn read_bridge_log_snippet(log_path: Option<&PathBuf>) -> Option<String> {
    const MAX_LOG_CHARS: usize = 1600;
    let path = log_path?;
    let mut file = fs::File::open(path).ok()?;
    let mut output = String::new();
    file.read_to_string(&mut output).ok()?;
    let output = output.trim();
    if output.is_empty() {
        return None;
    }
    let char_count = output.chars().count();
    if char_count <= MAX_LOG_CHARS {
        return Some(output.to_string());
    }
    let tail = output
        .chars()
        .skip(char_count.saturating_sub(MAX_LOG_CHARS))
        .collect::<String>();
    Some(format!("...{}", tail))
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
    let dir_name = format!(
        "{}-{}-{}",
        runtime_name,
        sanitize_file_part(&target.id),
        &key[..12]
    );
    let runtime_dir = dir.join(dir_name);
    fs::create_dir_all(&runtime_dir).map_err(|err| {
        format!(
            "创建代理桥接工作目录失败 {}: {}",
            runtime_dir.display(),
            err
        )
    })?;
    let path = runtime_dir.join("config.json");
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
        "vmess" | "vless" | "trojan" | "ss" | "hysteria" | "hysteria2" | "tuic"
        | "anytls" => Ok(BridgeRuntimeKind::Mihomo),
        other => Err(format!("{} 协议不需要或暂不支持内核桥接", other)),
    }
}

fn runtime_name(kind: &BridgeRuntimeKind) -> &'static str {
    match kind {
        BridgeRuntimeKind::Xray => XRAY_RUNTIME,
        BridgeRuntimeKind::SingBox => SING_BOX_RUNTIME,
        BridgeRuntimeKind::Mihomo => MIHOMO_RUNTIME,
    }
}

fn build_mihomo_config(target: &GatewayOutboundTarget, local_port: u16) -> Result<Value, String> {
    Ok(json!({
        "mixed-port": local_port,
        "bind-address": BRIDGE_BIND_HOST,
        "allow-lan": false,
        "mode": "rule",
        "log-level": "info",
        "ipv6": true,
        "unified-delay": true,
        "tcp-concurrent": true,
        "find-process-mode": "off",
        "profile": {
            "store-selected": false,
            "store-fake-ip": false
        },
        "proxies": [
            build_mihomo_proxy(target, MIHOMO_PROXY_NAME)?
        ],
        "proxy-groups": [
            {
                "name": MIHOMO_GROUP_NAME,
                "type": "select",
                "proxies": [
                    MIHOMO_PROXY_NAME
                ]
            }
        ],
        "rules": [
            format!("MATCH,{}", MIHOMO_GROUP_NAME)
        ]
    }))
}

fn build_mihomo_proxy(target: &GatewayOutboundTarget, proxy_name: &str) -> Result<Value, String> {
    if let Some(proxy) = build_mihomo_proxy_from_clash_options(target, proxy_name) {
        return Ok(Value::Object(proxy));
    }

    let protocol = target.protocol.to_ascii_lowercase();
    let mut proxy = Map::new();
    proxy.insert("name".to_string(), Value::String(proxy_name.to_string()));
    proxy.insert(
        "type".to_string(),
        Value::String(match protocol.as_str() {
            "shadowsocks" => "ss",
            other => other,
        }
        .to_string()),
    );
    proxy.insert("server".to_string(), Value::String(target.host.clone()));
    proxy.insert("port".to_string(), json!(target.port));
    proxy.insert("udp".to_string(), Value::Bool(true));

    match protocol.as_str() {
        "vless" => {
            let uuid = first_text(target, &["uuid", "id", "username", "options.uuid", "options.id"])
                .unwrap_or_else(|| target.username.clone());
            if uuid.trim().is_empty() {
                return Err("vless 节点缺少 UUID".to_string());
            }
            proxy.insert("uuid".to_string(), Value::String(uuid));
            proxy.insert(
                "encryption".to_string(),
                Value::String(
                    first_text(target, &["encryption", "query.encryption", "options.encryption"])
                        .unwrap_or_else(|| "none".to_string()),
                ),
            );
            insert_if_present(&mut proxy, "flow", first_text(target, &["flow", "query.flow", "options.flow"]));
            insert_if_present(
                &mut proxy,
                "packet-encoding",
                first_text(
                    target,
                    &[
                        "packet-encoding",
                        "query.packetEncoding",
                        "query.packet-encoding",
                        "options.packet-encoding",
                    ],
                ),
            );
            insert_mihomo_tls_options(&mut proxy, target, false);
            insert_mihomo_transport_options(&mut proxy, target);
        }
        "vmess" => {
            let uuid = first_text(target, &["uuid", "id", "username", "options.uuid", "options.id"])
                .unwrap_or_else(|| target.username.clone());
            if uuid.trim().is_empty() {
                return Err("vmess 节点缺少 UUID".to_string());
            }
            proxy.insert("uuid".to_string(), Value::String(uuid));
            proxy.insert(
                "alterId".to_string(),
                json!(
                    first_i64(target, &["alterId", "alter_id", "options.aid", "options.alterId"])
                        .or_else(|| first_text(target, &["options.aid"]).and_then(|value| value.parse().ok()))
                        .unwrap_or(0)
                ),
            );
            proxy.insert(
                "cipher".to_string(),
                Value::String(
                    first_text(target, &["cipher", "security", "options.cipher", "options.scy"])
                        .unwrap_or_else(|| "auto".to_string()),
                ),
            );
            insert_mihomo_tls_options(&mut proxy, target, false);
            insert_mihomo_transport_options(&mut proxy, target);
        }
        "trojan" => {
            let password = first_text(target, &["password", "id", "username", "options.password"])
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
            proxy.insert("password".to_string(), Value::String(password));
            insert_mihomo_tls_options(&mut proxy, target, true);
            insert_mihomo_transport_options(&mut proxy, target);
        }
        "ss" | "shadowsocks" => {
            let cipher = first_text(target, &["cipher", "method", "options.cipher"])
                .unwrap_or_else(|| target.username.clone());
            let password = first_text(target, &["password", "options.password"])
                .unwrap_or_else(|| target.password.clone());
            if cipher.trim().is_empty() || password.trim().is_empty() {
                return Err("shadowsocks 节点缺少加密方式或密码".to_string());
            }
            proxy.insert("cipher".to_string(), Value::String(cipher));
            proxy.insert("password".to_string(), Value::String(password));
            insert_if_present(&mut proxy, "plugin", first_text(target, &["plugin", "query.plugin", "options.plugin"]));
            if let Some(plugin_opts) = value_at(&target.standard_config, "options.plugin-opts") {
                proxy.insert("plugin-opts".to_string(), plugin_opts.clone());
            }
        }
        "hysteria2" => {
            let password = node_secret(target);
            if password.trim().is_empty() {
                return Err("hysteria2 节点缺少密码".to_string());
            }
            proxy.insert("password".to_string(), Value::String(password));
            insert_mihomo_tls_options(&mut proxy, target, true);
            insert_if_present(&mut proxy, "obfs", first_text(target, &["obfs", "query.obfs", "options.obfs"]));
            insert_if_present(
                &mut proxy,
                "obfs-password",
                first_text(target, &["obfs-password", "query.obfs-password", "options.obfs-password"]),
            );
        }
        "hysteria" => {
            let auth = node_secret(target);
            if auth.trim().is_empty() {
                return Err("hysteria 节点缺少认证信息".to_string());
            }
            proxy.insert("auth-str".to_string(), Value::String(auth));
            insert_if_present(
                &mut proxy,
                "protocol",
                first_text(target, &["protocol", "query.protocol", "options.protocol"]),
            );
            insert_if_present(&mut proxy, "obfs", first_text(target, &["obfs", "query.obfs", "options.obfs"]));
            insert_if_present(
                &mut proxy,
                "obfs-password",
                first_text(target, &["obfs-password", "query.obfs-password", "options.obfs-password"]),
            );
            if let Some(up) = first_i64(target, &["up", "upmbps", "options.up"]) {
                proxy.insert("up".to_string(), json!(up));
            }
            if let Some(down) = first_i64(target, &["down", "downmbps", "options.down"]) {
                proxy.insert("down".to_string(), json!(down));
            }
            insert_mihomo_tls_options(&mut proxy, target, true);
        }
        "tuic" => {
            let uuid = first_text(target, &["uuid", "id", "username", "options.uuid"])
                .unwrap_or_else(|| target.username.clone());
            let password = first_text(target, &["password", "query.password", "options.password"])
                .unwrap_or_else(|| target.password.clone());
            if uuid.trim().is_empty() || password.trim().is_empty() {
                return Err("tuic 节点缺少 uuid 或 password".to_string());
            }
            proxy.insert("uuid".to_string(), Value::String(uuid));
            proxy.insert("password".to_string(), Value::String(password));
            insert_if_present(
                &mut proxy,
                "congestion-controller",
                first_text(
                    target,
                    &[
                        "congestion-controller",
                        "congestion-control",
                        "query.congestion_control",
                        "options.congestion-controller",
                    ],
                ),
            );
            insert_if_present(
                &mut proxy,
                "udp-relay-mode",
                first_text(target, &["udp-relay-mode", "query.udp_relay_mode", "options.udp-relay-mode"]),
            );
            insert_mihomo_tls_options(&mut proxy, target, true);
        }
        "anytls" => {
            let password = node_secret(target);
            if password.trim().is_empty() {
                return Err("anytls 节点缺少密码".to_string());
            }
            proxy.insert("password".to_string(), Value::String(password));
            insert_mihomo_tls_options(&mut proxy, target, true);
        }
        other => return Err(format!("mihomo 暂不支持 {} 协议桥接", other)),
    }

    Ok(Value::Object(proxy))
}

fn build_mihomo_proxy_from_clash_options(
    target: &GatewayOutboundTarget,
    proxy_name: &str,
) -> Option<Map<String, Value>> {
    let options = value_at(&target.standard_config, "options")?.as_object()?;
    if !options.contains_key("type") || !options.contains_key("server") {
        return None;
    }

    let mut proxy = options.clone();
    proxy.insert("name".to_string(), Value::String(proxy_name.to_string()));
    proxy.insert("server".to_string(), Value::String(target.host.clone()));
    proxy.insert("port".to_string(), json!(target.port));
    if let Some(proxy_type) = text_at(&Value::Object(proxy.clone()), "type") {
        if proxy_type.eq_ignore_ascii_case("shadowsocks") {
            proxy.insert("type".to_string(), Value::String("ss".to_string()));
        }
    } else {
        proxy.insert("type".to_string(), Value::String(target.protocol.clone()));
    }
    Some(proxy)
}

fn insert_mihomo_tls_options(
    proxy: &mut Map<String, Value>,
    target: &GatewayOutboundTarget,
    default_tls: bool,
) {
    let security = first_text(
        target,
        &[
            "security",
            "query.security",
            "options.security",
            "options.tls",
        ],
    )
    .unwrap_or_default()
    .to_ascii_lowercase();
    let reality_enabled =
        security == "reality" || value_at(&target.standard_config, "options.reality-opts").is_some();
    let tls_enabled = reality_enabled
        || security == "tls"
        || first_bool(target, &["tls", "options.tls"]).unwrap_or(false)
        || default_tls;
    if !tls_enabled {
        return;
    }

    proxy.insert("tls".to_string(), Value::Bool(true));
    insert_if_present(proxy, "servername", first_server_name(target));
    if first_bool(
        target,
        &[
            "allowInsecure",
            "insecure",
            "skip-cert-verify",
            "query.allowInsecure",
            "query.allow_insecure",
            "query.insecure",
            "query.skip-cert-verify",
            "query.skip_cert_verify",
            "options.skip-cert-verify",
        ],
    )
    .unwrap_or(false)
    {
        proxy.insert("skip-cert-verify".to_string(), Value::Bool(true));
    }
    if let Some(fingerprint) =
        first_text(target, &["fp", "query.fp", "options.client-fingerprint"])
            .filter(|value| !value.trim().is_empty())
    {
        proxy.insert("client-fingerprint".to_string(), Value::String(fingerprint));
    }
    if let Some(alpn) = csv_list_value(first_text(target, &["alpn", "query.alpn", "options.alpn"])) {
        proxy.insert("alpn".to_string(), alpn);
    }
    if reality_enabled {
        let mut reality_opts = Map::new();
        insert_if_present(
            &mut reality_opts,
            "public-key",
            first_text(target, &["pbk", "query.pbk", "options.reality-opts.public-key"]),
        );
        insert_if_present(
            &mut reality_opts,
            "short-id",
            first_text(target, &["sid", "query.sid", "options.reality-opts.short-id"]),
        );
        if !reality_opts.is_empty() {
            proxy.insert("reality-opts".to_string(), Value::Object(reality_opts));
        }
    }
}

fn insert_mihomo_transport_options(proxy: &mut Map<String, Value>, target: &GatewayOutboundTarget) {
    let network = first_text(
        target,
        &[
            "query.type",
            "network",
            "options.network",
            "options.net",
        ],
    )
    .unwrap_or_else(|| "tcp".to_string())
    .to_ascii_lowercase();
    let network = match network.as_str() {
        "http" => "h2".to_string(),
        other => other.to_string(),
    };
    if network == "tcp" || network.is_empty() {
        return;
    }
    proxy.insert("network".to_string(), Value::String(network.clone()));

    match network.as_str() {
        "ws" => {
            let mut ws = Map::new();
            insert_if_present(
                &mut ws,
                "path",
                first_text(
                    target,
                    &["query.path", "path", "options.path", "options.ws-opts.path"],
                ),
            );
            ws.insert("headers".to_string(), json!({ "Host": first_ws_host(target) }));
            proxy.insert("ws-opts".to_string(), Value::Object(ws));
        }
        "grpc" => {
            let mut grpc = Map::new();
            insert_if_present(
                &mut grpc,
                "grpc-service-name",
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
            proxy.insert("grpc-opts".to_string(), Value::Object(grpc));
        }
        "h2" => {
            let mut h2 = Map::new();
            insert_if_present(
                &mut h2,
                "path",
                first_text(target, &["query.path", "path", "options.h2-opts.path"]),
            );
            if let Some(host) = first_text(target, &["query.host", "options.h2-opts.host", "host"])
                .filter(|value| !value.trim().is_empty())
            {
                h2.insert("host".to_string(), Value::Array(vec![Value::String(host)]));
            }
            proxy.insert("h2-opts".to_string(), Value::Object(h2));
        }
        _ => {}
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
            "query.type",
            "options.network",
            "options.net",
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
            ws.insert("headers".to_string(), json!({ "Host": first_ws_host(target) }));
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
            if let Some(host) = first_text(target, &["query.host", "options.h2-opts.host", "host"]) {
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
        "vless" => {
            let uuid = first_text(target, &["uuid", "id", "username", "options.uuid", "options.id"])
                .unwrap_or_else(|| target.username.clone());
            if uuid.trim().is_empty() {
                return Err("vless 节点缺少 UUID".to_string());
            }
            let mut outbound = base_sing_box_outbound("vless", target);
            outbound.insert("uuid".to_string(), Value::String(uuid));
            if let Some(flow) = first_text(target, &["flow", "query.flow", "options.flow"]) {
                outbound.insert("flow".to_string(), Value::String(flow));
            }
            insert_sing_box_v2ray_tls_and_transport(&mut outbound, target, false);
            Ok(Value::Object(outbound))
        }
        "vmess" => {
            let uuid = first_text(target, &["uuid", "id", "username", "options.uuid", "options.id"])
                .unwrap_or_else(|| target.username.clone());
            if uuid.trim().is_empty() {
                return Err("vmess 节点缺少 UUID".to_string());
            }
            let mut outbound = base_sing_box_outbound("vmess", target);
            outbound.insert("uuid".to_string(), Value::String(uuid));
            outbound.insert(
                "security".to_string(),
                Value::String(
                    first_text(target, &["security", "cipher", "options.cipher", "options.scy"])
                        .unwrap_or_else(|| "auto".to_string()),
                ),
            );
            if let Some(alter_id) =
                first_i64(target, &["alterId", "alter_id", "options.aid", "options.alterId"])
                    .or_else(|| first_text(target, &["options.aid"]).and_then(|value| value.parse().ok()))
            {
                outbound.insert("alter_id".to_string(), json!(alter_id));
            }
            insert_sing_box_v2ray_tls_and_transport(&mut outbound, target, false);
            Ok(Value::Object(outbound))
        }
        "trojan" => {
            let password = first_text(target, &["password", "id", "username", "options.password"])
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
            let mut outbound = base_sing_box_outbound("trojan", target);
            outbound.insert("password".to_string(), Value::String(password));
            insert_sing_box_v2ray_tls_and_transport(&mut outbound, target, true);
            Ok(Value::Object(outbound))
        }
        "ss" => {
            let method = first_text(target, &["method", "cipher", "options.cipher"])
                .unwrap_or_else(|| target.username.clone());
            let password = first_text(target, &["password", "options.password"])
                .unwrap_or_else(|| target.password.clone());
            if method.trim().is_empty() || password.trim().is_empty() {
                return Err("shadowsocks 节点缺少加密方式或密码".to_string());
            }
            let mut outbound = base_sing_box_outbound("shadowsocks", target);
            outbound.insert("method".to_string(), Value::String(method));
            outbound.insert("password".to_string(), Value::String(password));
            Ok(Value::Object(outbound))
        }
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
            "query.servername",
            "query.serverName",
            "options.sni",
            "options.servername",
            "options.serverName",
            "options.server-name",
            "options.peer",
        ],
    )
}

fn first_ws_host(target: &GatewayOutboundTarget) -> String {
    first_text(
        target,
        &[
            "query.host",
            "options.ws-opts.headers.Host",
            "options.ws-opts.headers.host",
            "headers.Host",
            "headers.host",
        ],
    )
    .unwrap_or_else(|| target.host.clone())
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

fn insert_sing_box_v2ray_tls_and_transport(
    outbound: &mut Map<String, Value>,
    target: &GatewayOutboundTarget,
    default_tls: bool,
) {
    if let Some(tls) = build_sing_box_v2ray_tls(target, default_tls) {
        outbound.insert("tls".to_string(), Value::Object(tls));
    }
    if let Some(transport) = build_sing_box_v2ray_transport(target) {
        outbound.insert("transport".to_string(), transport);
    }
}

fn build_sing_box_v2ray_tls(
    target: &GatewayOutboundTarget,
    default_enabled: bool,
) -> Option<Map<String, Value>> {
    let security = first_text(
        target,
        &[
            "security",
            "query.security",
            "options.security",
            "options.tls",
        ],
    )
    .unwrap_or_default()
    .to_ascii_lowercase();
    let reality_enabled =
        security == "reality" || value_at(&target.standard_config, "options.reality-opts").is_some();
    let tls_enabled = reality_enabled
        || security == "tls"
        || first_bool(target, &["tls", "options.tls"]).unwrap_or(false)
        || default_enabled;
    if !tls_enabled {
        return None;
    }

    let mut tls = Map::new();
    tls.insert("enabled".to_string(), Value::Bool(true));
    insert_if_present(&mut tls, "server_name", first_server_name(target));
    if first_bool(target, &["allowInsecure", "skip-cert-verify", "options.skip-cert-verify"])
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
    if let Some(fingerprint) =
        first_text(target, &["fp", "query.fp", "options.client-fingerprint"])
            .filter(|value| !value.trim().is_empty())
    {
        tls.insert(
            "utls".to_string(),
            json!({
                "enabled": true,
                "fingerprint": fingerprint
            }),
        );
    }
    if reality_enabled {
        let mut reality = Map::new();
        reality.insert("enabled".to_string(), Value::Bool(true));
        insert_if_present(
            &mut reality,
            "public_key",
            first_text(target, &["pbk", "query.pbk", "options.reality-opts.public-key"]),
        );
        insert_if_present(
            &mut reality,
            "short_id",
            first_text(target, &["sid", "query.sid", "options.reality-opts.short-id"]),
        );
        tls.insert("reality".to_string(), Value::Object(reality));
    }

    Some(tls)
}

fn build_sing_box_v2ray_transport(target: &GatewayOutboundTarget) -> Option<Value> {
    let network = first_text(
        target,
        &[
            "query.type",
            "network",
            "options.network",
            "options.net",
        ],
    )
    .unwrap_or_else(|| "tcp".to_string())
    .to_ascii_lowercase();
    let network = match network.as_str() {
        "h2" => "http".to_string(),
        other => other.to_string(),
    };

    match network.as_str() {
        "ws" => {
            let mut transport = Map::new();
            transport.insert("type".to_string(), Value::String("ws".to_string()));
            insert_if_present(
                &mut transport,
                "path",
                first_text(
                    target,
                    &["query.path", "path", "options.path", "options.ws-opts.path"],
                ),
            );
            transport.insert("headers".to_string(), json!({ "Host": first_ws_host(target) }));
            Some(Value::Object(transport))
        }
        "grpc" => {
            let mut transport = Map::new();
            transport.insert("type".to_string(), Value::String("grpc".to_string()));
            insert_if_present(
                &mut transport,
                "service_name",
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
            Some(Value::Object(transport))
        }
        "http" => {
            let mut transport = Map::new();
            transport.insert("type".to_string(), Value::String("http".to_string()));
            insert_if_present(
                &mut transport,
                "path",
                first_text(target, &["query.path", "path", "options.h2-opts.path"]),
            );
            if let Some(host) = first_text(target, &["query.host", "options.h2-opts.host", "host"])
                .filter(|value| !value.trim().is_empty())
            {
                transport.insert("host".to_string(), Value::Array(vec![Value::String(host)]));
            }
            Some(Value::Object(transport))
        }
        "tcp" | "" => None,
        _ => None,
    }
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

fn csv_list_value(value: Option<String>) -> Option<Value> {
    let values = value?
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| Value::String(value.to_string()))
        .collect::<Vec<_>>();
    if values.is_empty() {
        None
    } else {
        Some(Value::Array(values))
    }
}

fn insert_if_present(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        map.insert(key.to_string(), Value::String(value));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_mihomo_config, build_mihomo_proxy, GatewayOutboundTarget, MIHOMO_GROUP_NAME,
        MIHOMO_PROXY_NAME,
    };
    use serde_json::{json, Value};

    fn target(protocol: &str, standard_config: Value) -> GatewayOutboundTarget {
        GatewayOutboundTarget {
            id: "node-1".to_string(),
            name: "Test Node".to_string(),
            protocol: protocol.to_string(),
            host: "proxy.example.com".to_string(),
            port: 443,
            username: "11111111-1111-4111-8111-111111111111".to_string(),
            password: String::new(),
            gateway_port: 7897,
            standard_config,
        }
    }

    #[test]
    fn mihomo_config_routes_traffic_through_proxy_group() {
        let config = build_mihomo_config(
            &target(
                "vless",
                json!({
                    "options": {
                        "name": "Subscription Node",
                        "type": "vless",
                        "server": "proxy.example.com",
                        "port": 443,
                        "uuid": "11111111-1111-4111-8111-111111111111",
                        "tls": true,
                        "servername": "edge.example.com"
                    }
                }),
            ),
            19001,
        )
        .expect("mihomo config");

        assert_eq!(config["mixed-port"], json!(19001));
        assert_eq!(config["proxies"][0]["name"], json!(MIHOMO_PROXY_NAME));
        assert_eq!(config["proxy-groups"][0]["name"], json!(MIHOMO_GROUP_NAME));
        assert_eq!(
            config["proxy-groups"][0]["proxies"][0],
            json!(MIHOMO_PROXY_NAME)
        );
        assert_eq!(
            config["rules"][0],
            json!(format!("MATCH,{}", MIHOMO_GROUP_NAME))
        );
    }

    #[test]
    fn mihomo_share_link_config_reads_servername_query() {
        let node = target(
            "vless",
            json!({
                "id": "11111111-1111-4111-8111-111111111111",
                "query": {
                    "security": "tls",
                    "servername": "edge.example.com"
                }
            }),
        );

        let proxy = build_mihomo_proxy(&node, MIHOMO_PROXY_NAME).expect("mihomo proxy");
        assert_eq!(proxy["name"], json!(MIHOMO_PROXY_NAME));
        assert_eq!(proxy["servername"], json!("edge.example.com"));
    }

    #[test]
    fn mihomo_ws_host_does_not_become_tls_servername() {
        let node = target(
            "vless",
            json!({
                "id": "11111111-1111-4111-8111-111111111111",
                "query": {
                    "security": "tls",
                    "type": "ws",
                    "host": "ws.example.com",
                    "path": "/proxy"
                }
            }),
        );

        let proxy = build_mihomo_proxy(&node, MIHOMO_PROXY_NAME).expect("mihomo proxy");
        assert_eq!(proxy["tls"], json!(true));
        assert!(proxy.get("servername").is_none());
        assert_eq!(proxy["ws-opts"]["headers"]["Host"], json!("ws.example.com"));
        assert_eq!(proxy["ws-opts"]["path"], json!("/proxy"));
    }

    #[test]
    fn mihomo_ws_host_falls_back_to_server_host() {
        let node = target(
            "vless",
            json!({
                "id": "11111111-1111-4111-8111-111111111111",
                "query": {
                    "security": "tls",
                    "type": "ws",
                    "path": "/proxy"
                }
            }),
        );

        let proxy = build_mihomo_proxy(&node, MIHOMO_PROXY_NAME).expect("mihomo proxy");
        assert_eq!(
            proxy["ws-opts"]["headers"]["Host"],
            json!("proxy.example.com")
        );
    }
}
