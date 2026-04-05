use std::collections::HashMap;

use forge_stdlib::net::{parse_http_request, render_http_response, RawResponse};

#[test]
fn parse_http_request_extracts_request_data() {
    let request = parse_http_request(concat!(
        "POST /users?id=10 HTTP/1.1\r\n",
        "Host: localhost\r\n",
        "Content-Length: 7\r\n",
        "\r\n",
        "payload"
    ))
    .expect("request should parse");

    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/users");
    assert_eq!(request.query.get("id"), Some(&"10".to_string()));
    assert_eq!(request.headers.get("Host"), Some(&"localhost".to_string()));
    assert_eq!(request.body, "payload");
}

#[test]
fn render_http_response_writes_status_headers_and_body() {
    let mut headers = HashMap::new();
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    let rendered = render_http_response(&RawResponse {
        status: 201,
        headers,
        body: "{\"ok\":true}".to_string(),
    });

    assert!(rendered.starts_with("HTTP/1.1 201 Created\r\n"));
    assert!(rendered.contains("Content-Type: application/json\r\n"));
    assert!(rendered.contains("Content-Length: 11\r\n"));
    assert!(rendered.ends_with("\r\n\r\n{\"ok\":true}"));
}
