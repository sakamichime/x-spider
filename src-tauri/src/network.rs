use std::collections::HashMap;
use reqwest::Method;
use serde_json::Value;

#[derive(Default, serde::Serialize)]
pub struct Response {
  status: u16,
  headers: HashMap<String, Vec<String>>,
  body: Value,
}

#[tauri::command]
pub async fn network_fetch(
  method: String,
  url: String,
  body: String,
  enable_proxy: bool,
  proxy_url: String,
  response_type: String,
  headers: HashMap<String, String>,
) -> Result<Response, String> {
  let map_reqwest_err = |err: reqwest::Error| err.to_string();
  // Convert method string into Method
  let method: Method = match method.to_uppercase().as_str() {
    "GET" => Ok(Method::GET),
    "POST" => Ok(Method::POST),
    "PATCH" => Ok(Method::PATCH),
    "PUT" => Ok(Method::PUT),
    "DELETE" => Ok(Method::DELETE),
    "HEAD" => Ok(Method::HEAD),
    _ => Err("Invalid method".to_string()),
  }?;

  // Build client
  let client = {
    let mut b = reqwest::Client::builder().danger_accept_invalid_certs(true);

    // Auto set proxy settings
    if enable_proxy {
      if proxy_url.len() == 0 {
        // Use system proxy, do nothing
      } else {
        // Use custom proxy url
        let proxy_http = reqwest::Proxy::http(proxy_url.clone()).or(Err("Failed to set proxy url".to_string()))?;
        let proxy_https = reqwest::Proxy::https(proxy_url.clone()).or(Err("Failed to set proxy url".to_string()))?;
        b = b.proxy(proxy_http).proxy(proxy_https);
      }
    } else {
      // No proxy
      b = b.no_proxy();
    }

    b.build().or(Err("Failed to build reqwest client".to_string()))
  }?;

  // Build request
  let request = {
    let mut req = client.request(method.clone(), url);
    for (k, v) in headers {
      req = req.header(k, v);
    }

    if !matches!(method.clone(), Method::GET) {
      req = req.body(body);
    }

    req
  };

  // Send request
  let response = request.send().await.map_err(map_reqwest_err)?;

  // Extract some info
  let status = response.status().as_u16();
  let resp_headers = {
    let reqwest_headers = response.headers();
    let mut h: HashMap<String, Vec<String>> = HashMap::with_capacity(reqwest_headers.len());

    for (k, v) in reqwest_headers {
      let v = v.to_str();
      if let Err(_) = v {
        continue;
      }

      let v = v.unwrap().to_string();
      h.entry(k.to_string()).and_modify(|arr: &mut Vec<String>| arr.push(v.clone()))
        .or_insert_with(|| vec![v]);
    }

    h
  };

  // Load response body
  let body: Value = {
    match response_type.as_str() {
      "json" => response.json().await.map_err(map_reqwest_err).map(|res| Value::Object(res)),
      "text" => response.text().await.map_err(map_reqwest_err).map(|res| Value::String(res)),
      "binary" => {
        let bytes = response.bytes().await.map_err(map_reqwest_err)?;
        serde_json::to_value(bytes.to_vec()).map_err(|err| err.to_string())
      },
      _ => Err("Unsupported response type".to_string())
    }
  }?;

  return Ok(Response {
    status,
    body,
    headers: resp_headers })
}

#[tauri::command]
pub async fn network_get_system_proxy_url() -> Result<HashMap<String, String>, ()> {
  use winreg::RegKey;
  use winreg::enums::HKEY_CURRENT_USER;

  let mut result: HashMap<String, String> = HashMap::new();

  let hkcu = RegKey::predef(HKEY_CURRENT_USER);
  let internet_settings = hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings");

  if let Ok(settings) = internet_settings {
    // 检查是否启用代理
    let proxy_enable: u32 = settings.get_value("ProxyEnable").unwrap_or(0);
    if proxy_enable == 0 {
      return Ok(result);
    }

    // 读取代理服务器地址
    let proxy_server: String = match settings.get_value("ProxyServer") {
      Ok(s) => s,
      Err(_) => return Ok(result),
    };

    // 解析代理服务器字符串
    // 格式可能是 "host:port" 或 "http=host:port;https=host:port"
    if proxy_server.contains('=') {
      // 多个代理配置，格式如 "http=127.0.0.1:8080;https=127.0.0.1:8080"
      for part in proxy_server.split(';') {
        if let Some((key, value)) = part.split_once('=') {
          result.insert(key.to_lowercase(), value.to_string());
        }
      }
    } else {
      // 单一代理配置，格式如 "127.0.0.1:8080"
      result.insert("http".to_string(), proxy_server.clone());
      result.insert("https".to_string(), proxy_server);
    }
  }

  Ok(result)
}
