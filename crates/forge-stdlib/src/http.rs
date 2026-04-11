use std::collections::HashMap;
use std::thread::sleep;
use std::time::Duration;

/// HTTP メソッド
#[derive(Debug, Clone, PartialEq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    fn as_str(&self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }
}

/// リクエストボディの種類
#[derive(Debug, Clone)]
pub enum Body {
    Json(serde_json::Value),
    Form(HashMap<String, String>),
    Raw(Vec<u8>),
}

/// HTTP リクエストビルダー
#[derive(Debug, Clone)]
pub struct RequestBuilder {
    pub method: Method,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub query: HashMap<String, String>,
    pub body: Option<Body>,
    pub timeout_ms: Option<u64>,
    pub retry_count: u32,
}

impl RequestBuilder {
    fn new(method: Method, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: HashMap::new(),
            query: HashMap::new(),
            body: None,
            timeout_ms: None,
            retry_count: 0,
        }
    }

    /// ヘッダーを追加する
    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// クエリパラメータを追加する
    pub fn query(mut self, params: HashMap<String, String>) -> Self {
        self.query.extend(params);
        self
    }

    /// JSON ボディを設定する（Content-Type: application/json 自動付与）
    pub fn json(mut self, value: serde_json::Value) -> Self {
        self.headers
            .insert("Content-Type".to_string(), "application/json".to_string());
        self.body = Some(Body::Json(value));
        self
    }

    /// フォームデータを設定する
    pub fn form(mut self, params: HashMap<String, String>) -> Self {
        self.body = Some(Body::Form(params));
        self
    }

    /// タイムアウトをミリ秒で設定する
    pub fn timeout(mut self, ms: u64) -> Self {
        self.timeout_ms = Some(ms);
        self
    }

    /// リトライ回数を設定する
    pub fn retry(mut self, n: u32) -> Self {
        self.retry_count = n;
        self
    }

    /// リクエストを送信する
    pub fn send(self) -> Result<Response, String> {
        let max_attempts = self.retry_count + 1;
        let mut last_err = String::new();

        for attempt in 0..max_attempts {
            if attempt > 0 {
                sleep(Duration::from_millis(100 * attempt as u64));
            }

            match self.send_once() {
                Ok(res) => {
                    // 5xx はリトライ対象
                    if res.status >= 500 && attempt + 1 < max_attempts {
                        last_err = format!("server error: {}", res.status);
                        continue;
                    }
                    return Ok(res);
                }
                Err(err) => {
                    last_err = err;
                    // ネットワークエラーはリトライ
                }
            }
        }

        Err(last_err)
    }

    fn send_once(&self) -> Result<Response, String> {
        let mut client_builder = reqwest::blocking::ClientBuilder::new();
        if let Some(ms) = self.timeout_ms {
            client_builder = client_builder.timeout(Duration::from_millis(ms));
        }
        let client = client_builder
            .build()
            .map_err(|e| format!("failed to build client: {}", e))?;

        let url = build_url(&self.url, &self.query)?;
        let mut req = match self.method {
            Method::Get => client.get(&url),
            Method::Post => client.post(&url),
            Method::Put => client.put(&url),
            Method::Patch => client.patch(&url),
            Method::Delete => client.delete(&url),
        };

        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }

        req = match &self.body {
            Some(Body::Json(v)) => req.json(v),
            Some(Body::Form(map)) => req.form(map),
            Some(Body::Raw(bytes)) => req.body(bytes.clone()),
            None => req,
        };

        let resp = req.send().map_err(|e| format!("request failed: {}", e))?;

        let status = resp.status().as_u16();
        let ok = status >= 200 && status < 300;

        let mut headers = HashMap::new();
        for (k, v) in resp.headers() {
            if let Ok(val) = v.to_str() {
                headers.insert(k.as_str().to_string(), val.to_string());
            }
        }

        let body_bytes = resp
            .bytes()
            .map_err(|e| format!("failed to read body: {}", e))?
            .to_vec();

        Ok(Response {
            status,
            ok,
            headers,
            body_bytes,
        })
    }
}

fn build_url(base: &str, query: &HashMap<String, String>) -> Result<String, String> {
    if query.is_empty() {
        return Ok(base.to_string());
    }
    let params: Vec<String> = query
        .iter()
        .map(|(k, v)| format!("{}={}", urlencoding_simple(k), urlencoding_simple(v)))
        .collect();
    let sep = if base.contains('?') { '&' } else { '?' };
    Ok(format!("{}{}{}", base, sep, params.join("&")))
}

/// 最小限の URL エンコード（スペース→%20・特殊文字）
fn urlencoding_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(c),
            _ => {
                for byte in c.to_string().as_bytes() {
                    out.push('%');
                    out.push_str(&format!("{:02X}", byte));
                }
            }
        }
    }
    out
}

/// HTTP レスポンス
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub ok: bool,
    pub headers: HashMap<String, String>,
    body_bytes: Vec<u8>,
}

impl Response {
    /// テスト用コンストラクタ
    #[doc(hidden)]
    pub fn new_for_test(status: u16, body_bytes: Vec<u8>) -> Self {
        Self {
            status,
            ok: status >= 200 && status < 300,
            headers: HashMap::new(),
            body_bytes,
        }
    }

    /// ボディを文字列として返す
    pub fn text(&self) -> Result<String, String> {
        String::from_utf8(self.body_bytes.clone())
            .map_err(|e| format!("body is not valid utf-8: {}", e))
    }

    /// ボディを JSON としてパースする
    pub fn json(&self) -> Result<serde_json::Value, String> {
        serde_json::from_slice(&self.body_bytes)
            .map_err(|e| format!("failed to parse body as json: {}", e))
    }

    /// ボディをバイト列として返す
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        Ok(self.body_bytes.clone())
    }
}

// ---- トップレベル関数 ----

/// GET リクエストを構築する
pub fn get(url: impl Into<String>) -> RequestBuilder {
    RequestBuilder::new(Method::Get, url)
}

/// POST リクエストを構築する
pub fn post(url: impl Into<String>) -> RequestBuilder {
    RequestBuilder::new(Method::Post, url)
}

/// PUT リクエストを構築する
pub fn put(url: impl Into<String>) -> RequestBuilder {
    RequestBuilder::new(Method::Put, url)
}

/// PATCH リクエストを構築する
pub fn patch(url: impl Into<String>) -> RequestBuilder {
    RequestBuilder::new(Method::Patch, url)
}

/// DELETE リクエストを構築する
pub fn delete(url: impl Into<String>) -> RequestBuilder {
    RequestBuilder::new(Method::Delete, url)
}
