use std::collections::HashMap;
use std::future::Future;
use std::io::Read;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRequest {
    pub method: String,
    pub path: String,
    pub query: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawResponse {
    pub status: i64,
    pub headers: HashMap<String, String>,
    pub body: String,
}

pub fn tcp_listen<F>(port: i64, handler: F) -> Result<(), String>
where
    F: Fn(RawRequest) -> RawResponse + Send + Sync + 'static,
{
    let port = u16::try_from(port).map_err(|_| format!("invalid port: {}", port))?;
    let handler = Arc::new(handler);
    let rt =
        tokio::runtime::Runtime::new().map_err(|err| format!("tokio runtime failed: {}", err))?;
    rt.block_on(async_tcp_listen(port, handler))
}

/// `tcp_listen_async` — async handler 版 (Forge: `fn(RawRequest) -> RawResponse!`)
///
/// ハンドラが `Future<Output = Result<RawResponse, String>>` を返す場合に使用する。
/// Forge コード側でハンドラ関数が `.await?` を含むと transpiler が自動的に async fn に
/// 昇格するため、このバリアントが必要になる。
pub fn tcp_listen_async<F, Fut>(port: i64, handler: F) -> Result<(), String>
where
    F: Fn(RawRequest) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<RawResponse, String>> + Send + 'static,
{
    let port = u16::try_from(port).map_err(|_| format!("invalid port: {}", port))?;
    let handler = Arc::new(handler);
    let rt =
        tokio::runtime::Runtime::new().map_err(|err| format!("tokio runtime failed: {}", err))?;
    rt.block_on(async_tcp_listen_fut(port, handler))
}

async fn async_tcp_listen_fut<F, Fut>(port: u16, handler: Arc<F>) -> Result<(), String>
where
    F: Fn(RawRequest) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<RawResponse, String>> + Send + 'static,
{
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .map_err(|err| format!("tcp bind failed: {}", err))?;

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let handler = Arc::clone(&handler);
                tokio::spawn(async move {
                    let _ = handle_connection_async(stream, handler).await;
                });
            }
            Err(err) => return Err(format!("tcp accept failed: {}", err)),
        }
    }
}

async fn handle_connection_async<F, Fut>(
    mut stream: tokio::net::TcpStream,
    handler: Arc<F>,
) -> Result<(), String>
where
    F: Fn(RawRequest) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<RawResponse, String>> + Send + 'static,
{
    let request = read_request_async(&mut stream).await?;
    let response = handler(request).await.unwrap_or_else(|err| RawResponse {
        status: 500,
        headers: HashMap::new(),
        body: format!("Internal Server Error: {}", err),
    });
    let payload = serialize_response(&response);
    stream
        .write_all(payload.as_bytes())
        .await
        .map_err(|err| format!("tcp write failed: {}", err))?;
    Ok(())
}

async fn read_request_async(stream: &mut tokio::net::TcpStream) -> Result<RawRequest, String> {
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0_u8; 1024];
    let header_end;

    loop {
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|err| format!("tcp read failed: {}", err))?;
        if read == 0 {
            return Err("connection closed before request headers".to_string());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(end) = find_header_end(&buffer) {
            header_end = end;
            break;
        }
    }

    let header_bytes = &buffer[..header_end];
    let header_text = std::str::from_utf8(header_bytes)
        .map_err(|err| format!("request headers are not utf-8: {}", err))?;
    let (method, path, query, headers) = parse_request_head(header_text)?;
    let content_length = headers
        .get("Content-Length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);

    let expected_len = header_end + 4 + content_length;
    while buffer.len() < expected_len {
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|err| format!("tcp read failed: {}", err))?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }

    if buffer.len() < expected_len {
        return Err("request body shorter than content-length".to_string());
    }

    let body_bytes = buffer[header_end + 4..expected_len].to_vec();
    let body = String::from_utf8(body_bytes)
        .map_err(|err| format!("request body is not utf-8: {}", err))?;

    Ok(RawRequest {
        method,
        path,
        query,
        headers,
        body,
    })
}

async fn async_tcp_listen<F>(port: u16, handler: Arc<F>) -> Result<(), String>
where
    F: Fn(RawRequest) -> RawResponse + Send + Sync + 'static,
{
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .map_err(|err| format!("tcp bind failed: {}", err))?;

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let handler = Arc::clone(&handler);
                tokio::spawn(async move {
                    let _ = handle_connection(stream, handler).await;
                });
            }
            Err(err) => return Err(format!("tcp accept failed: {}", err)),
        }
    }
}

async fn handle_connection<F>(
    mut stream: tokio::net::TcpStream,
    handler: Arc<F>,
) -> Result<(), String>
where
    F: Fn(RawRequest) -> RawResponse + Send + Sync + 'static,
{
    let request = read_request_async(&mut stream).await?;
    // Handler is sync — run in a blocking thread
    let response = tokio::task::spawn_blocking(move || handler(request))
        .await
        .map_err(|err| format!("handler panicked: {}", err))?;
    let payload = serialize_response(&response);
    stream
        .write_all(payload.as_bytes())
        .await
        .map_err(|err| format!("tcp write failed: {}", err))?;
    Ok(())
}

pub fn parse_http_request(src: &str) -> Result<RawRequest, String> {
    read_http_request(&mut src.as_bytes())
}

pub fn render_http_response(response: &RawResponse) -> String {
    serialize_response(response)
}

fn read_http_request<R: Read>(reader: &mut R) -> Result<RawRequest, String> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    let header_end;

    loop {
        let read = reader
            .read(&mut chunk)
            .map_err(|err| format!("tcp read failed: {}", err))?;
        if read == 0 {
            return Err("connection closed before request headers".to_string());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(end) = find_header_end(&buffer) {
            header_end = end;
            break;
        }
    }

    let header_bytes = &buffer[..header_end];
    let header_text = std::str::from_utf8(header_bytes)
        .map_err(|err| format!("request headers are not utf-8: {}", err))?;
    let (method, path, query, headers) = parse_request_head(header_text)?;
    let content_length = headers
        .get("Content-Length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);

    let expected_len = header_end + 4 + content_length;
    while buffer.len() < expected_len {
        let read = reader
            .read(&mut chunk)
            .map_err(|err| format!("tcp read failed: {}", err))?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }

    if buffer.len() < expected_len {
        return Err("request body shorter than content-length".to_string());
    }

    let body_bytes = &buffer[header_end + 4..expected_len];
    let body = String::from_utf8(body_bytes.to_vec())
        .map_err(|err| format!("request body is not utf-8: {}", err))?;

    Ok(RawRequest {
        method,
        path,
        query,
        headers,
        body,
    })
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_request_head(
    head: &str,
) -> Result<
    (
        String,
        String,
        HashMap<String, String>,
        HashMap<String, String>,
    ),
    String,
> {
    let mut lines = head.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "missing request line".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| "missing request method".to_string())?
        .to_string();
    let target = parts
        .next()
        .ok_or_else(|| "missing request target".to_string())?;
    let version = parts
        .next()
        .ok_or_else(|| "missing http version".to_string())?;
    if parts.next().is_some() {
        return Err("invalid request line".to_string());
    }
    if version != "HTTP/1.1" {
        return Err(format!("unsupported http version: {}", version));
    }

    let (path, query) = split_path_and_query(target);
    let mut headers = HashMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(format!("invalid header: {}", line));
        };
        headers.insert(name.trim().to_string(), value.trim().to_string());
    }

    Ok((method, path, query, headers))
}

fn split_path_and_query(target: &str) -> (String, HashMap<String, String>) {
    let Some((path, query)) = target.split_once('?') else {
        return (target.to_string(), HashMap::new());
    };

    let mut query_map = HashMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.split_once('=') {
            Some((key, value)) => (key, value),
            None => (pair, ""),
        };
        query_map.insert(key.to_string(), value.to_string());
    }

    (path.to_string(), query_map)
}

fn serialize_response(response: &RawResponse) -> String {
    let mut headers = response.headers.clone();
    if !headers.contains_key("Content-Length") {
        headers.insert(
            "Content-Length".to_string(),
            response.body.len().to_string(),
        );
    }
    if !headers.contains_key("Content-Type") {
        headers.insert(
            "Content-Type".to_string(),
            "text/plain; charset=utf-8".to_string(),
        );
    }

    let mut lines = Vec::with_capacity(headers.len() + 1);
    lines.push(format!(
        "HTTP/1.1 {} {}",
        response.status,
        reason_phrase(response.status)
    ));

    let mut header_entries = headers.into_iter().collect::<Vec<_>>();
    header_entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, value) in header_entries {
        lines.push(format!("{}: {}", name, value));
    }

    format!("{}\r\n\r\n{}", lines.join("\r\n"), response.body)
}

fn reason_phrase(status: i64) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_request_line_headers_and_body() {
        let payload = concat!(
            "POST /items?id=42&lang=ja HTTP/1.1\r\n",
            "Host: localhost\r\n",
            "Content-Length: 5\r\n",
            "\r\n",
            "hello"
        );
        let request = parse_http_request(payload).expect("request should parse");
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/items");
        assert_eq!(request.query.get("id"), Some(&"42".to_string()));
        assert_eq!(request.query.get("lang"), Some(&"ja".to_string()));
        assert_eq!(request.headers.get("Host"), Some(&"localhost".to_string()));
        assert_eq!(request.body, "hello");
    }

    #[test]
    fn serializes_response_with_default_headers() {
        let response = RawResponse {
            status: 200,
            headers: HashMap::new(),
            body: "ok".to_string(),
        };
        let payload = render_http_response(&response);
        assert!(payload.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(payload.contains("Content-Length: 2\r\n"));
        assert!(payload.ends_with("\r\n\r\nok"));
    }
}
