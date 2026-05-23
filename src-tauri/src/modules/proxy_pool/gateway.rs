use super::bridge;
use super::store::{self, GatewayOutboundTarget};
use crate::modules::process;
use base64::{engine::general_purpose, Engine as _};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore};
use serde_json::json;
use std::net::IpAddr;
use std::sync::{Arc, OnceLock};
use tauri::Emitter;
use std::time::Duration;
use tokio::io::{copy_bidirectional, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{watch, Mutex as TokioMutex};
use tokio::time::{timeout, Instant};
use tokio_rustls::TlsConnector;
use url::Url;

const GATEWAY_BIND_HOST: &str = "127.0.0.1";
const HTTP_HEAD_READ_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_UPSTREAM_TIMEOUT: Duration = Duration::from_secs(20);
const GATEWAY_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);
const GATEWAY_BIND_RETRY_TIMEOUT: Duration = Duration::from_secs(5);
const GATEWAY_BIND_RETRY_INTERVAL: Duration = Duration::from_millis(100);
const GATEWAY_PORT_RELEASE_TIMEOUT: Duration = Duration::from_secs(5);
const GATEWAY_PORT_RELEASE_POLL_INTERVAL: Duration = Duration::from_millis(100);
const FAILOVER_PROMOTION_COOLDOWN: Duration = Duration::from_secs(2);
const MAX_HTTP_HEAD_BYTES: usize = 64 * 1024;
const PROXY_POOL_GATEWAY_FAILOVER_EVENT: &str = "proxy_pool://gateway_failover";

static GATEWAY_RUNTIME: OnceLock<TokioMutex<GatewayRuntime>> = OnceLock::new();
static HTTPS_PROXY_TLS_CONFIG: OnceLock<Result<Arc<ClientConfig>, String>> = OnceLock::new();
static FAILOVER_PROMOTION_STATE: OnceLock<TokioMutex<FailoverPromotionState>> = OnceLock::new();

trait GatewayIo: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T> GatewayIo for T where T: AsyncRead + AsyncWrite + Unpin + Send {}

type GatewayStream = Box<dyn GatewayIo>;

#[derive(Default)]
struct GatewayRuntime {
    task: Option<tokio::task::JoinHandle<()>>,
    shutdown_sender: Option<watch::Sender<bool>>,
    running_port: Option<u16>,
}

#[derive(Default)]
struct FailoverPromotionState {
    last_attempt_at: Option<Instant>,
}

#[derive(Debug, Clone)]
struct GatewayBindEndpoint {
    host: String,
    port: u16,
}

#[derive(Debug)]
struct HttpHead {
    head: Vec<u8>,
    leftover: Vec<u8>,
}

#[derive(Debug, Clone)]
struct HeaderLine {
    name: String,
    value: String,
}

#[derive(Debug, Clone)]
struct ProxyRequest {
    method: String,
    target: String,
    version: String,
    headers: Vec<HeaderLine>,
}

#[derive(Debug)]
struct HttpDestination {
    host: String,
    port: u16,
    origin_target: String,
    absolute_target: String,
}

fn gateway_runtime() -> &'static TokioMutex<GatewayRuntime> {
    GATEWAY_RUNTIME.get_or_init(|| TokioMutex::new(GatewayRuntime::default()))
}

fn failover_promotion_state() -> &'static TokioMutex<FailoverPromotionState> {
    FAILOVER_PROMOTION_STATE.get_or_init(|| TokioMutex::new(FailoverPromotionState::default()))
}

pub async fn restore_gateway_state() {
    if let Err(error) = sync_gateway_state().await {
        crate::modules::logger::log_warn(&format!(
            "[ProxyGateway] 启动时恢复内置代理网关失败: {}",
            error
        ));
    }
}

pub async fn sync_gateway_state() -> Result<(), String> {
    let service_state = store::get_service_state()?;
    if !service_state.enabled {
        let _ = stop_gateway().await;
        bridge::stop_bridge().await;
        store::update_service_actual_port(None)?;
        return Ok(());
    }

    let preferred_port = service_state.preferred_port;
    let gateway_targets = load_gateway_targets().await?;
    let current_outbound = gateway_targets
        .first()
        .ok_or_else(|| "内置代理网关没有可用出口节点".to_string())?;
    if !bridge::is_bridge_protocol(&current_outbound.protocol) {
        bridge::stop_bridge().await;
    }
    let stale_task = {
        let mut runtime = gateway_runtime().lock().await;
        if runtime
            .task
            .as_ref()
            .is_some_and(|task| task.is_finished())
        {
            runtime.running_port = None;
            runtime.shutdown_sender = None;
            runtime.task.take()
        } else {
            None
        }
    };
    if let Some(task) = stale_task {
        let _ = task.await;
        store::update_service_actual_port(None)?;
    }

    let already_running = {
        let runtime = gateway_runtime().lock().await;
        runtime.running_port == Some(preferred_port) && runtime.task.is_some()
    };
    if already_running {
        return Ok(());
    }

    let _ = stop_gateway().await;
    store::update_service_actual_port(None)?;

    let listener = bind_gateway_listener(preferred_port)
        .await
        .map_err(|error| format_gateway_bind_error(preferred_port, &error))?;
    let (shutdown_sender, mut shutdown_receiver) = watch::channel(false);
    let port = preferred_port;

    let task = tokio::spawn(async move {
        crate::modules::logger::log_info(&format!(
            "[ProxyGateway] 内置代理网关已启动: http://{}:{}",
            GATEWAY_BIND_HOST, port
        ));

        loop {
            tokio::select! {
                changed = shutdown_receiver.changed() => {
                    if changed.is_ok() {
                        break;
                    }
                }
                accepted = listener.accept() => {
                    match accepted {
                        Ok((stream, addr)) => {
                            tokio::spawn(async move {
                                if let Err(error) = handle_client(stream).await {
                                    crate::modules::logger::log_warn(&format!(
                                        "[ProxyGateway] 请求处理失败 {}: {}",
                                        addr, error
                                    ));
                                }
                            });
                        }
                        Err(error) => {
                            crate::modules::logger::log_warn(&format!(
                                "[ProxyGateway] 接收连接失败: {}",
                                error
                            ));
                            break;
                        }
                    }
                }
            }
        }

        let should_clear = {
            let mut runtime = gateway_runtime().lock().await;
            if runtime.running_port == Some(port) {
                runtime.running_port = None;
                runtime.shutdown_sender = None;
                true
            } else {
                false
            }
        };
        if should_clear {
            let _ = store::update_service_actual_port(None);
        }
    });

    {
        let mut runtime = gateway_runtime().lock().await;
        runtime.running_port = Some(port);
        runtime.shutdown_sender = Some(shutdown_sender);
        runtime.task = Some(task);
    }
    store::update_service_actual_port(Some(port))?;
    Ok(())
}

pub async fn prepare_gateway_for_restart() -> Result<(), String> {
    let service_state = store::get_service_state()?;
    let endpoint = stop_gateway().await.or_else(|| {
        service_state.actual_port.map(|port| GatewayBindEndpoint {
            host: GATEWAY_BIND_HOST.to_string(),
            port,
        })
    });
    store::update_service_actual_port(None)?;
    bridge::cleanup_registered_bridge_processes("proxy-gateway-prepare-restart");

    let mut warnings = Vec::new();
    if let Some(endpoint) = endpoint {
        if let Err(err) = wait_for_gateway_port_release(&endpoint.host, endpoint.port).await {
            warnings.push(err);
            match process::kill_port_processes(endpoint.port) {
                Ok(killed) if killed > 0 => {
                    crate::modules::logger::log_warn(&format!(
                        "[ProxyGateway] 重启前清理内置代理网关端口 {}，已结束 {} 个残留进程",
                        endpoint.port, killed
                    ));
                    wait_for_gateway_port_release(&endpoint.host, endpoint.port).await?;
                }
                Ok(_) => {}
                Err(kill_error) => warnings.push(kill_error),
            }
        }
    }

    if warnings.is_empty() {
        Ok(())
    } else {
        let message = warnings.join("; ");
        crate::modules::logger::log_warn(&format!(
            "[ProxyGateway] 重启前关闭内置代理网关存在警告: {}",
            message
        ));
        Ok(())
    }
}

async fn stop_gateway() -> Option<GatewayBindEndpoint> {
    let (shutdown_sender, task, endpoint) = {
        let mut runtime = gateway_runtime().lock().await;
        let endpoint = runtime.running_port.map(|port| GatewayBindEndpoint {
            host: GATEWAY_BIND_HOST.to_string(),
            port,
        });
        runtime.running_port = None;
        (runtime.shutdown_sender.take(), runtime.task.take(), endpoint)
    };

    if let Some(sender) = shutdown_sender {
        let _ = sender.send(true);
    }

    if let Some(mut task) = task {
        tokio::select! {
            result = &mut task => {
                let _ = result;
            }
            _ = tokio::time::sleep(GATEWAY_SHUTDOWN_TIMEOUT) => {
                crate::modules::logger::log_warn("[ProxyGateway] 停止内置代理网关超时，已强制结束监听任务");
                task.abort();
                let _ = task.await;
            }
        }
    }
    bridge::stop_bridge().await;
    endpoint
}

async fn wait_for_gateway_port_release(host: &str, port: u16) -> Result<(), String> {
    let deadline = Instant::now() + GATEWAY_PORT_RELEASE_TIMEOUT;
    loop {
        match TcpListener::bind((host, port)).await {
            Ok(listener) => {
                drop(listener);
                return Ok(());
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::AddrInUse && Instant::now() < deadline =>
            {
                tokio::time::sleep(GATEWAY_PORT_RELEASE_POLL_INTERVAL).await;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                let pids = process::find_pids_by_port(port).unwrap_or_default();
                let owner = if pids.is_empty() {
                    "未发现外部监听进程".to_string()
                } else {
                    format!("监听进程: {}", process::summarize_pid_list_for_log(&pids))
                };
                return Err(format!(
                    "内置代理网关端口 {} 停止后仍未释放: {}",
                    port, owner
                ));
            }
            Err(error) => {
                return Err(format!(
                    "检查内置代理网关端口 {} 是否释放失败: {}",
                    port, error
                ));
            }
        }
    }
}

async fn bind_gateway_listener(port: u16) -> Result<TcpListener, std::io::Error> {
    let deadline = Instant::now() + GATEWAY_BIND_RETRY_TIMEOUT;
    loop {
        match TcpListener::bind((GATEWAY_BIND_HOST, port)).await {
            Ok(listener) => return Ok(listener),
            Err(error)
                if error.kind() == std::io::ErrorKind::AddrInUse && Instant::now() < deadline =>
            {
                tokio::time::sleep(GATEWAY_BIND_RETRY_INTERVAL).await;
            }
            Err(error) => return Err(error),
        }
    }
}

async fn handle_client(mut client: TcpStream) -> Result<(), String> {
    match proxy_client(&mut client).await {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = write_error_response(&mut client, "502 Bad Gateway", &error).await;
            Err(error)
        }
    }
}

async fn proxy_client(client: &mut TcpStream) -> Result<(), String> {
    let initial = read_http_head(client).await?;
    let request = parse_proxy_request(&initial.head)?;
    let outbounds = load_gateway_targets().await?;

    if request.method.eq_ignore_ascii_case("CONNECT") {
        return proxy_connect_request(client, &request, &outbounds).await;
    }

    proxy_http_request(client, initial, &request, &outbounds).await
}

async fn proxy_connect_request(
    client: &mut TcpStream,
    request: &ProxyRequest,
    outbounds: &[GatewayOutboundTarget],
) -> Result<(), String> {
    let (host, port) = parse_authority(&request.target, 443)?;
    let (mut upstream, outbound) = open_tunnel_with_fallback(outbounds, &host, port).await?;
    persist_successful_failover_if_needed(outbounds, outbound).await;
    client
        .write_all(b"HTTP/1.1 200 Connection Established\r\nProxy-Agent: AI-Lemon-Tools\r\n\r\n")
        .await
        .map_err(|error| format!("写入 CONNECT 响应失败: {}", error))?;
    let _ = copy_bidirectional(client, &mut upstream)
        .await
        .map_err(|error| format!("CONNECT 隧道转发失败: {}", error))?;
    Ok(())
}

async fn proxy_http_request(
    client: &mut TcpStream,
    initial: HttpHead,
    request: &ProxyRequest,
    outbounds: &[GatewayOutboundTarget],
) -> Result<(), String> {
    let destination = resolve_http_destination(request)?;
    let (mut upstream, outbound) = open_http_upstream_with_fallback(outbounds, &destination).await?;
    let use_upstream_http_proxy = is_http_proxy_outbound(outbound);
    let request_target = if use_upstream_http_proxy {
        destination.absolute_target.as_str()
    } else {
        destination.origin_target.as_str()
    };
    let rewritten = build_forward_http_head(
        request,
        request_target,
        use_upstream_http_proxy.then_some(outbound),
    );
    upstream
        .write_all(&rewritten)
        .await
        .map_err(|error| format!("写入上游请求头失败: {}", error))?;
    if !initial.leftover.is_empty() {
        upstream
            .write_all(&initial.leftover)
            .await
            .map_err(|error| format!("写入上游请求体失败: {}", error))?;
    }

    persist_successful_failover_if_needed(outbounds, outbound).await;
    let _ = copy_bidirectional(client, &mut upstream)
        .await
        .map_err(|error| format!("HTTP 转发失败: {}", error))?;
    Ok(())
}

async fn open_tunnel_with_fallback<'a>(
    outbounds: &'a [GatewayOutboundTarget],
    destination_host: &str,
    destination_port: u16,
) -> Result<(GatewayStream, &'a GatewayOutboundTarget), String> {
    let mut errors = Vec::new();
    for outbound in outbounds {
        match open_tunnel(outbound, destination_host, destination_port).await {
            Ok(stream) => {
                if !errors.is_empty() {
                    crate::modules::logger::log_info(&format!(
                        "[ProxyGateway] 已切换到备用出口节点: {} ({})",
                        outbound.name, outbound.protocol
                    ));
                }
                return Ok((stream, outbound));
            }
            Err(error) => {
                crate::modules::logger::log_warn(&format!(
                    "[ProxyGateway] 出口节点不可用 {} ({}): {}",
                    outbound.name, outbound.protocol, error
                ));
                errors.push(format!("{} ({}): {}", outbound.name, outbound.protocol, error));
            }
        }
    }
    Err(format_outbound_errors("所有已选择的代理节点均不可用", errors))
}

async fn open_http_upstream_with_fallback<'a>(
    outbounds: &'a [GatewayOutboundTarget],
    destination: &HttpDestination,
) -> Result<(GatewayStream, &'a GatewayOutboundTarget), String> {
    let mut errors = Vec::new();
    for outbound in outbounds {
        let result = match outbound.protocol.to_ascii_lowercase().as_str() {
            "http" => {
                bridge::stop_bridge().await;
                match guard_not_self_proxy(outbound) {
                    Ok(()) => connect_tcp(&outbound.host, outbound.port)
                        .await
                        .map(box_gateway_stream),
                    Err(error) => Err(error),
                }
            }
            "https" => {
                bridge::stop_bridge().await;
                match guard_not_self_proxy(outbound) {
                    Ok(()) => connect_https_proxy(outbound).await,
                    Err(error) => Err(error),
                }
            }
            _ => open_tunnel(outbound, &destination.host, destination.port).await,
        };

        match result {
            Ok(stream) => {
                if !errors.is_empty() {
                    crate::modules::logger::log_info(&format!(
                        "[ProxyGateway] 已切换到备用出口节点: {} ({})",
                        outbound.name, outbound.protocol
                    ));
                }
                return Ok((stream, outbound));
            }
            Err(error) => {
                crate::modules::logger::log_warn(&format!(
                    "[ProxyGateway] 出口节点不可用 {} ({}): {}",
                    outbound.name, outbound.protocol, error
                ));
                errors.push(format!("{} ({}): {}", outbound.name, outbound.protocol, error));
            }
        }
    }
    Err(format_outbound_errors("所有已选择的代理节点均不可用", errors))
}

async fn persist_successful_failover_if_needed(
    outbounds: &[GatewayOutboundTarget],
    successful_outbound: &GatewayOutboundTarget,
) {
    let Some(primary_outbound) = outbounds.first() else {
        return;
    };
    if primary_outbound.id == successful_outbound.id {
        return;
    }

    {
        let mut state = failover_promotion_state().lock().await;
        let now = Instant::now();
        if state
            .last_attempt_at
            .is_some_and(|last| now.duration_since(last) < FAILOVER_PROMOTION_COOLDOWN)
        {
            return;
        }
        state.last_attempt_at = Some(now);
    }

    let previous_current_node_id = primary_outbound.id.clone();
    let successful_node_id = successful_outbound.id.clone();
    let successful_name = successful_outbound.name.clone();
    match tokio::task::spawn_blocking(move || {
        store::promote_gateway_outbound_after_failover(
            &previous_current_node_id,
            &successful_node_id,
        )
        .map(|state| (previous_current_node_id, successful_node_id, state))
    })
    .await
    {
        Ok(Ok((from_node_id, to_node_id, Some(service_state)))) => {
            crate::modules::logger::log_info(&format!(
                "[ProxyGateway] 自动故障切换已持久化: {} -> {} ({})",
                from_node_id, to_node_id, successful_name
            ));
            if let Some(app) = crate::get_app_handle() {
                let _ = app.emit(
                    PROXY_POOL_GATEWAY_FAILOVER_EVENT,
                    json!({
                        "fromNodeId": from_node_id,
                        "toNodeId": to_node_id,
                        "serviceState": service_state,
                    }),
                );
            }
        }
        Ok(Ok((_from_node_id, _to_node_id, None))) => {}
        Ok(Err(error)) => {
            crate::modules::logger::log_warn(&format!(
                "[ProxyGateway] 持久化自动故障切换失败: {}",
                error
            ));
        }
        Err(error) => {
            crate::modules::logger::log_warn(&format!(
                "[ProxyGateway] 持久化自动故障切换任务失败: {}",
                error
            ));
        }
    }
}

fn format_outbound_errors(prefix: &str, errors: Vec<String>) -> String {
    match errors.len() {
        0 => prefix.to_string(),
        1 => errors.into_iter().next().unwrap_or_else(|| prefix.to_string()),
        _ => format!("{}: {}", prefix, errors.join("；")),
    }
}

async fn open_tunnel(
    outbound: &GatewayOutboundTarget,
    destination_host: &str,
    destination_port: u16,
) -> Result<GatewayStream, String> {
    match outbound.protocol.to_ascii_lowercase().as_str() {
        "direct" => {
            bridge::stop_bridge().await;
            connect_tcp(destination_host, destination_port)
                .await
                .map(box_gateway_stream)
        }
        "http" => {
            bridge::stop_bridge().await;
            guard_not_self_proxy(outbound)?;
            open_http_proxy_tunnel(outbound, destination_host, destination_port)
                .await
                .map(box_gateway_stream)
        }
        "socks5" => {
            bridge::stop_bridge().await;
            guard_not_self_proxy(outbound)?;
            open_socks5_tunnel(outbound, destination_host, destination_port)
                .await
                .map(box_gateway_stream)
        }
        "https" => {
            bridge::stop_bridge().await;
            guard_not_self_proxy(outbound)?;
            open_https_proxy_tunnel(outbound, destination_host, destination_port).await
        }
        "vmess" | "vless" | "trojan" | "ss" | "hysteria" | "hysteria2" | "tuic"
        | "anytls" => {
            let endpoint = bridge::ensure_bridge(outbound).await?;
            let bridge_outbound = GatewayOutboundTarget {
                id: format!("{}-bridge", outbound.id),
                name: format!("{} 桥接出口", outbound.name),
                protocol: "socks5".to_string(),
                host: endpoint.host,
                port: endpoint.port,
                username: String::new(),
                password: String::new(),
                gateway_port: outbound.gateway_port,
                standard_config: json!({}),
            };
            match open_socks5_tunnel(&bridge_outbound, destination_host, destination_port).await {
                Ok(stream) => Ok(box_gateway_stream(stream)),
                Err(error) => Err(append_bridge_log(error).await),
            }
        }
        other => Err(format!("内置代理网关暂不支持 {} 协议出口", other)),
    }
}

fn box_gateway_stream<S>(stream: S) -> GatewayStream
where
    S: GatewayIo + 'static,
{
    Box::new(stream)
}

async fn append_bridge_log(error: String) -> String {
    match bridge::current_bridge_log_snippet().await {
        Some(output) => format!("{}; 内核输出: {}", error, output),
        None => error,
    }
}

fn guard_not_self_proxy(outbound: &GatewayOutboundTarget) -> Result<(), String> {
    if outbound.port == outbound.gateway_port && is_loopback_host(&outbound.host) {
        return Err("出口节点指向了内置代理网关自身，请更换出口节点或端口".to_string());
    }
    Ok(())
}

fn is_loopback_host(host: &str) -> bool {
    let host = host.trim().trim_matches('[').trim_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host == "127.0.0.1"
        || host == "::1"
        || host.starts_with("127.")
}

async fn open_http_proxy_tunnel(
    outbound: &GatewayOutboundTarget,
    destination_host: &str,
    destination_port: u16,
) -> Result<TcpStream, String> {
    let mut stream = connect_tcp(&outbound.host, outbound.port).await?;
    write_http_proxy_connect(
        &mut stream,
        outbound,
        destination_host,
        destination_port,
        "HTTP",
    )
    .await?;
    Ok(stream)
}

async fn open_https_proxy_tunnel(
    outbound: &GatewayOutboundTarget,
    destination_host: &str,
    destination_port: u16,
) -> Result<GatewayStream, String> {
    let mut stream = connect_https_proxy(outbound).await?;
    write_http_proxy_connect(
        &mut *stream,
        outbound,
        destination_host,
        destination_port,
        "HTTPS",
    )
    .await?;
    Ok(stream)
}

async fn write_http_proxy_connect<S>(
    stream: &mut S,
    outbound: &GatewayOutboundTarget,
    destination_host: &str,
    destination_port: u16,
    proxy_scheme: &str,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin + ?Sized,
{
    let authority = format_authority(destination_host, destination_port);
    let proxy_auth = proxy_authorization_header(outbound);
    let request = format!(
        "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\nProxy-Connection: Keep-Alive\r\n{proxy_auth}\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|error| format!("写入上游 {} 代理 CONNECT 失败: {}", proxy_scheme, error))?;
    let response = read_http_head(stream).await?;
    let status = parse_response_status(&response.head)?;
    if !(200..300).contains(&status) {
        return Err(format!(
            "上游 {} 代理 CONNECT 失败: HTTP {} ({})",
            proxy_scheme, status, outbound.name
        ));
    }
    Ok(())
}

async fn connect_https_proxy(outbound: &GatewayOutboundTarget) -> Result<GatewayStream, String> {
    let tcp = connect_tcp(&outbound.host, outbound.port)
        .await
        .map_err(|error| format!("连接上游 HTTPS 代理 TCP 失败: {}", error))?;
    let server_name = https_proxy_server_name(&outbound.host)?;
    let connector = TlsConnector::from(https_proxy_tls_config()?);
    let stream = timeout(CONNECT_UPSTREAM_TIMEOUT, connector.connect(server_name, tcp))
        .await
        .map_err(|_| {
            format!(
                "HTTPS 上游代理 TLS 握手超时: {}",
                format_authority(&outbound.host, outbound.port)
            )
        })?
        .map_err(|error| {
            format!(
                "HTTPS 上游代理 TLS 握手失败 {}: {}",
                format_authority(&outbound.host, outbound.port),
                error
            )
        })?;
    Ok(box_gateway_stream(stream))
}

fn https_proxy_server_name(host: &str) -> Result<ServerName<'static>, String> {
    let host = host.trim().trim_matches('[').trim_matches(']');
    if host.is_empty() {
        return Err("HTTPS 上游代理主机名为空，无法执行 TLS 握手".to_string());
    }
    ServerName::try_from(host.to_string()).map_err(|_| {
        format!(
            "HTTPS 上游代理主机名无法作为 TLS SNI 使用: {}，请使用证书匹配的域名",
            host
        )
    })
}

fn https_proxy_tls_config() -> Result<Arc<ClientConfig>, String> {
    HTTPS_PROXY_TLS_CONFIG
        .get_or_init(build_https_proxy_tls_config)
        .clone()
}

fn build_https_proxy_tls_config() -> Result<Arc<ClientConfig>, String> {
    let cert_result = rustls_native_certs::load_native_certs();
    for error in &cert_result.errors {
        crate::modules::logger::log_warn(&format!(
            "[ProxyGateway] 加载部分系统根证书失败: {}",
            error
        ));
    }
    let certs = cert_result.certs;
    if certs.is_empty() {
        return Err("系统根证书为空，无法验证 HTTPS 上游代理证书".to_string());
    }

    let mut root_store = RootCertStore::empty();
    let mut accepted = 0usize;
    let mut rejected = 0usize;
    for cert in certs {
        match root_store.add(cert) {
            Ok(()) => accepted += 1,
            Err(_) => rejected += 1,
        }
    }
    if accepted == 0 {
        return Err("系统根证书无法被 rustls 使用，无法验证 HTTPS 上游代理证书".to_string());
    }
    if rejected > 0 {
        crate::modules::logger::log_warn(&format!(
            "[ProxyGateway] 已跳过 {} 个无法解析的系统根证书",
            rejected
        ));
    }

    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    Ok(Arc::new(config))
}

async fn open_socks5_tunnel(
    outbound: &GatewayOutboundTarget,
    destination_host: &str,
    destination_port: u16,
) -> Result<TcpStream, String> {
    let mut stream = connect_tcp(&outbound.host, outbound.port).await?;
    if outbound.username.is_empty() {
        stream
            .write_all(&[0x05, 0x01, 0x00])
            .await
            .map_err(|error| format!("写入 SOCKS5 握手失败: {}", error))?;
    } else {
        stream
            .write_all(&[0x05, 0x02, 0x00, 0x02])
            .await
            .map_err(|error| format!("写入 SOCKS5 握手失败: {}", error))?;
    }

    let mut method_response = [0u8; 2];
    stream
        .read_exact(&mut method_response)
        .await
        .map_err(|error| format!("读取 SOCKS5 握手失败: {}", error))?;
    if method_response[0] != 0x05 {
        return Err("SOCKS5 代理返回了无效版本".to_string());
    }
    match method_response[1] {
        0x00 => {}
        0x02 => authenticate_socks5(outbound, &mut stream).await?,
        0xff => return Err("SOCKS5 代理没有可用认证方式".to_string()),
        method => return Err(format!("SOCKS5 代理选择了不支持的认证方式: {}", method)),
    }

    let mut request = Vec::with_capacity(8 + destination_host.len());
    request.extend_from_slice(&[0x05, 0x01, 0x00]);
    if let Ok(ip) = destination_host.parse::<IpAddr>() {
        match ip {
            IpAddr::V4(addr) => {
                request.push(0x01);
                request.extend_from_slice(&addr.octets());
            }
            IpAddr::V6(addr) => {
                request.push(0x04);
                request.extend_from_slice(&addr.octets());
            }
        }
    } else {
        let host_bytes = destination_host.as_bytes();
        if host_bytes.len() > u8::MAX as usize {
            return Err("SOCKS5 目标域名过长".to_string());
        }
        request.push(0x03);
        request.push(host_bytes.len() as u8);
        request.extend_from_slice(host_bytes);
    }
    request.extend_from_slice(&destination_port.to_be_bytes());
    stream
        .write_all(&request)
        .await
        .map_err(|error| format!("写入 SOCKS5 CONNECT 失败: {}", error))?;

    let mut response_head = [0u8; 4];
    stream
        .read_exact(&mut response_head)
        .await
        .map_err(|error| format!("读取 SOCKS5 CONNECT 响应失败: {}", error))?;
    if response_head[0] != 0x05 {
        return Err("SOCKS5 CONNECT 返回了无效版本".to_string());
    }
    if response_head[1] != 0x00 {
        return Err(format!(
            "SOCKS5 CONNECT 失败: {}",
            socks5_reply_message(response_head[1])
        ));
    }
    read_socks5_bound_address(&mut stream, response_head[3]).await?;
    Ok(stream)
}

async fn authenticate_socks5(
    outbound: &GatewayOutboundTarget,
    stream: &mut TcpStream,
) -> Result<(), String> {
    let username = outbound.username.as_bytes();
    let password = outbound.password.as_bytes();
    if username.len() > u8::MAX as usize || password.len() > u8::MAX as usize {
        return Err("SOCKS5 用户名或密码过长".to_string());
    }
    let mut request = Vec::with_capacity(3 + username.len() + password.len());
    request.push(0x01);
    request.push(username.len() as u8);
    request.extend_from_slice(username);
    request.push(password.len() as u8);
    request.extend_from_slice(password);
    stream
        .write_all(&request)
        .await
        .map_err(|error| format!("写入 SOCKS5 认证失败: {}", error))?;
    let mut response = [0u8; 2];
    stream
        .read_exact(&mut response)
        .await
        .map_err(|error| format!("读取 SOCKS5 认证响应失败: {}", error))?;
    if response != [0x01, 0x00] {
        return Err("SOCKS5 用户名密码认证失败".to_string());
    }
    Ok(())
}

async fn read_socks5_bound_address(stream: &mut TcpStream, atyp: u8) -> Result<(), String> {
    let address_len = match atyp {
        0x01 => 4,
        0x03 => {
            let mut len = [0u8; 1];
            stream
                .read_exact(&mut len)
                .await
                .map_err(|error| format!("读取 SOCKS5 绑定地址失败: {}", error))?;
            len[0] as usize
        }
        0x04 => 16,
        other => return Err(format!("SOCKS5 返回了未知地址类型: {}", other)),
    };
    let mut discard = vec![0u8; address_len + 2];
    stream
        .read_exact(&mut discard)
        .await
        .map_err(|error| format!("读取 SOCKS5 绑定地址失败: {}", error))?;
    Ok(())
}

async fn connect_tcp(host: &str, port: u16) -> Result<TcpStream, String> {
    if host.trim().is_empty() || port == 0 {
        return Err("代理网关目标地址或端口为空".to_string());
    }
    timeout(
        CONNECT_UPSTREAM_TIMEOUT,
        TcpStream::connect((host.trim(), port)),
    )
    .await
    .map_err(|_| format!("连接 {} 超时", format_authority(host, port)))?
    .map_err(|error| format!("连接 {} 失败: {}", format_authority(host, port), error))
}

async fn read_http_head<S>(stream: &mut S) -> Result<HttpHead, String>
where
    S: AsyncRead + Unpin + ?Sized,
{
    let mut buffer = Vec::with_capacity(4096);
    let mut chunk = [0u8; 2048];

    loop {
        let bytes_read = timeout(HTTP_HEAD_READ_TIMEOUT, stream.read(&mut chunk))
            .await
            .map_err(|_| "读取 HTTP 头超时".to_string())?
            .map_err(|error| format!("读取 HTTP 头失败: {}", error))?;
        if bytes_read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..bytes_read]);
        if let Some(end) = find_http_head_end(&buffer) {
            let leftover = buffer[end..].to_vec();
            buffer.truncate(end);
            return Ok(HttpHead {
                head: buffer,
                leftover,
            });
        }
        if buffer.len() > MAX_HTTP_HEAD_BYTES {
            return Err("HTTP 请求头过大".to_string());
        }
    }

    if buffer.is_empty() {
        return Err("HTTP 请求为空".to_string());
    }
    Ok(HttpHead {
        head: buffer,
        leftover: Vec::new(),
    })
}

fn find_http_head_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

fn parse_proxy_request(head: &[u8]) -> Result<ProxyRequest, String> {
    let text = String::from_utf8_lossy(head);
    let mut lines = text.split("\r\n");
    let request_line = lines.next().ok_or_else(|| "HTTP 请求行为空".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| "HTTP 请求缺少 method".to_string())?
        .to_string();
    let target = parts
        .next()
        .ok_or_else(|| "HTTP 请求缺少 target".to_string())?
        .to_string();
    let version = parts.next().unwrap_or("HTTP/1.1").to_string();
    let mut headers = Vec::new();

    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push(HeaderLine {
                name: name.trim().to_string(),
                value: value.trim().to_string(),
            });
        }
    }

    Ok(ProxyRequest {
        method,
        target,
        version,
        headers,
    })
}

fn parse_response_status(head: &[u8]) -> Result<u16, String> {
    let text = String::from_utf8_lossy(head);
    let status_line = text
        .split("\r\n")
        .next()
        .ok_or_else(|| "上游代理响应为空".to_string())?;
    status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| "上游代理响应缺少状态码".to_string())?
        .parse::<u16>()
        .map_err(|error| format!("上游代理状态码无效: {}", error))
}

fn resolve_http_destination(request: &ProxyRequest) -> Result<HttpDestination, String> {
    if request.target.starts_with("http://") || request.target.starts_with("https://") {
        let url = Url::parse(&request.target).map_err(|error| format!("代理目标 URL 无效: {}", error))?;
        if url.scheme() == "https" {
            return Err("HTTPS 请求需要使用 CONNECT 方法建立隧道".to_string());
        }
        let host = url
            .host_str()
            .ok_or_else(|| "代理目标缺少 Host".to_string())?
            .to_string();
        let port = url
            .port_or_known_default()
            .ok_or_else(|| "代理目标缺少端口".to_string())?;
        let mut origin_target = url.path().to_string();
        if origin_target.is_empty() {
            origin_target.push('/');
        }
        if let Some(query) = url.query() {
            origin_target.push('?');
            origin_target.push_str(query);
        }
        return Ok(HttpDestination {
            host,
            port,
            origin_target,
            absolute_target: request.target.clone(),
        });
    }

    let host_header = header_value(&request.headers, "host")
        .ok_or_else(|| "HTTP 代理请求缺少 Host 头".to_string())?;
    let (host, port) = parse_authority(&host_header, 80)?;
    let origin_target = if request.target.is_empty() {
        "/".to_string()
    } else {
        request.target.clone()
    };
    let absolute_target = format!(
        "http://{}{}",
        format_authority(&host, port),
        if origin_target.starts_with('/') {
            origin_target.clone()
        } else {
            format!("/{}", origin_target)
        }
    );
    Ok(HttpDestination {
        host,
        port,
        origin_target,
        absolute_target,
    })
}

fn build_forward_http_head(
    request: &ProxyRequest,
    forward_target: &str,
    upstream_http_proxy: Option<&GatewayOutboundTarget>,
) -> Vec<u8> {
    let mut output = format!(
        "{} {} {}\r\n",
        request.method, forward_target, request.version
    );
    let mut has_host = false;
    for header in &request.headers {
        if header.name.eq_ignore_ascii_case("host") {
            has_host = true;
        }
        if should_drop_proxy_header(&header.name) {
            continue;
        }
        output.push_str(&header.name);
        output.push_str(": ");
        output.push_str(&header.value);
        output.push_str("\r\n");
    }
    if !has_host {
        if let Ok(destination) = resolve_http_destination(request) {
            output.push_str("Host: ");
            output.push_str(&format_authority(&destination.host, destination.port));
            output.push_str("\r\n");
        }
    }
    if let Some(outbound) = upstream_http_proxy {
        output.push_str(&proxy_authorization_header(outbound));
    }
    output.push_str("\r\n");
    output.into_bytes()
}

fn is_http_proxy_outbound(outbound: &GatewayOutboundTarget) -> bool {
    outbound.protocol.eq_ignore_ascii_case("http")
        || outbound.protocol.eq_ignore_ascii_case("https")
}

fn should_drop_proxy_header(name: &str) -> bool {
    name.eq_ignore_ascii_case("proxy-connection")
        || name.eq_ignore_ascii_case("proxy-authorization")
}

fn header_value(headers: &[HeaderLine], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.clone())
}

fn parse_authority(raw: &str, default_port: u16) -> Result<(String, u16), String> {
    let authority = raw.trim();
    if authority.is_empty() {
        return Err("目标地址为空".to_string());
    }
    if let Some(rest) = authority.strip_prefix('[') {
        let Some(end) = rest.find(']') else {
            return Err("IPv6 目标地址格式错误".to_string());
        };
        let host = rest[..end].to_string();
        let port = rest[end + 1..]
            .strip_prefix(':')
            .map(|value| parse_port(value, default_port))
            .transpose()?
            .unwrap_or(default_port);
        return Ok((host, port));
    }
    if let Some((host, port)) = authority.rsplit_once(':') {
        if !host.contains(':') {
            return Ok((host.to_string(), parse_port(port, default_port)?));
        }
    }
    Ok((authority.to_string(), default_port))
}

fn parse_port(raw: &str, default_port: u16) -> Result<u16, String> {
    if raw.trim().is_empty() {
        return Ok(default_port);
    }
    raw.trim()
        .parse::<u16>()
        .map_err(|_| format!("端口格式错误: {}", raw))
        .and_then(|port| {
            if port == 0 {
                Err("端口必须在 1-65535 之间".to_string())
            } else {
                Ok(port)
            }
        })
}

fn format_authority(host: &str, port: u16) -> String {
    let host = host.trim();
    if host.contains(':') && !host.starts_with('[') {
        format!("[{}]:{}", host, port)
    } else {
        format!("{}:{}", host, port)
    }
}

fn proxy_authorization_header(outbound: &GatewayOutboundTarget) -> String {
    if outbound.username.is_empty() {
        return String::new();
    }
    let token = general_purpose::STANDARD.encode(format!(
        "{}:{}",
        outbound.username, outbound.password
    ));
    format!("Proxy-Authorization: Basic {token}\r\n")
}

async fn load_gateway_targets() -> Result<Vec<GatewayOutboundTarget>, String> {
    tokio::task::spawn_blocking(store::load_gateway_outbound_candidates)
        .await
        .map_err(|error| format!("读取内置代理网关出口失败: {}", error))?
}

async fn write_error_response(
    stream: &mut TcpStream,
    status: &str,
    body: &str,
) -> Result<(), String> {
    let body = body.as_bytes();
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(header.as_bytes())
        .await
        .map_err(|error| format!("写入错误响应失败: {}", error))?;
    stream
        .write_all(body)
        .await
        .map_err(|error| format!("写入错误响应失败: {}", error))
}

fn format_gateway_bind_error(port: u16, error: &std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::AddrInUse {
        return format!(
            "内置代理网关端口 {} 已被占用，请修改网关端口或关闭占用程序",
            port
        );
    }
    format!("启动内置代理网关失败: {}", error)
}

fn socks5_reply_message(code: u8) -> &'static str {
    match code {
        0x01 => "一般 SOCKS 服务器失败",
        0x02 => "规则不允许连接",
        0x03 => "网络不可达",
        0x04 => "主机不可达",
        0x05 => "连接被拒绝",
        0x06 => "TTL 过期",
        0x07 => "命令不支持",
        0x08 => "地址类型不支持",
        _ => "未知错误",
    }
}
