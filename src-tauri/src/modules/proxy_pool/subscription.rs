use reqwest::Url;
use std::time::Duration;

const MAX_SUBSCRIPTION_BYTES: u64 = 2 * 1024 * 1024;
const FETCH_TIMEOUT_SECONDS: u64 = 20;

#[derive(Debug, Clone)]
pub struct FetchedSubscription {
    pub url: String,
    pub content: String,
}

pub fn normalize_subscription_url(raw_url: &str) -> Result<String, String> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return Err("订阅 URL 不能为空".to_string());
    }

    let url = Url::parse(trimmed).map_err(|err| format!("订阅 URL 格式错误: {}", err))?;
    match url.scheme() {
        "http" | "https" => Ok(url.to_string()),
        _ => Err("订阅 URL 仅支持 http 或 https".to_string()),
    }
}

pub async fn fetch_subscription(raw_url: &str) -> Result<FetchedSubscription, String> {
    let url = normalize_subscription_url(raw_url)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(FETCH_TIMEOUT_SECONDS))
        .user_agent("ai-lemon-tools-proxy-subscription/1.0")
        .build()
        .map_err(|err| format!("初始化订阅 HTTP 客户端失败: {}", err))?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|err| format!("拉取订阅失败: {}", err))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("拉取订阅失败，HTTP 状态码: {}", status.as_u16()));
    }

    if response
        .content_length()
        .is_some_and(|length| length > MAX_SUBSCRIPTION_BYTES)
    {
        return Err("订阅内容不能超过 2 MB".to_string());
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|err| format!("读取订阅内容失败: {}", err))?;
    if bytes.len() as u64 > MAX_SUBSCRIPTION_BYTES {
        return Err("订阅内容不能超过 2 MB".to_string());
    }

    let content =
        String::from_utf8(bytes.to_vec()).map_err(|_| "订阅内容不是有效 UTF-8 文本".to_string())?;
    Ok(FetchedSubscription { url, content })
}
