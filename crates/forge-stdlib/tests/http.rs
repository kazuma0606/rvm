use forge_stdlib::http::{delete, get, patch, post, put, Response};
use std::collections::HashMap;

// ---- RequestBuilder ユニットテスト（ネットワーク不要） ----

#[test]
fn test_get_request_builder() {
    let req = get("https://example.com/users");
    assert_eq!(req.url, "https://example.com/users");
    assert_eq!(req.method, forge_stdlib::http::Method::Get);
}

#[test]
fn test_post_with_json_body() {
    let payload = serde_json::json!({ "name": "Alice" });
    let req = post("https://example.com/users").json(payload.clone());
    match req.body {
        Some(forge_stdlib::http::Body::Json(v)) => assert_eq!(v, payload),
        _ => panic!("expected Json body"),
    }
    assert_eq!(
        req.headers.get("Content-Type").map(|s| s.as_str()),
        Some("application/json")
    );
}

#[test]
fn test_form_body() {
    let mut form = HashMap::new();
    form.insert("name".to_string(), "Alice".to_string());
    form.insert("age".to_string(), "30".to_string());
    let req = post("https://example.com/upload").form(form.clone());
    match req.body {
        Some(forge_stdlib::http::Body::Form(f)) => assert_eq!(f, form),
        _ => panic!("expected Form body"),
    }
}

#[test]
fn test_header_chaining() {
    let req = get("https://example.com")
        .header("Authorization", "Bearer token123")
        .header("X-Custom", "value");
    assert_eq!(
        req.headers.get("Authorization").map(|s| s.as_str()),
        Some("Bearer token123")
    );
    assert_eq!(
        req.headers.get("X-Custom").map(|s| s.as_str()),
        Some("value")
    );
}

#[test]
fn test_query_params() {
    let mut params = HashMap::new();
    params.insert("page".to_string(), "1".to_string());
    params.insert("limit".to_string(), "20".to_string());
    let req = get("https://example.com/users").query(params.clone());
    assert_eq!(req.query, params);
}

#[test]
fn test_timeout_setting() {
    let req = get("https://example.com").timeout(5000);
    assert_eq!(req.timeout_ms, Some(5000));
}

#[test]
fn test_retry_setting() {
    let req = get("https://example.com").retry(3);
    assert_eq!(req.retry_count, 3);
}

#[test]
fn test_response_ok_flag() {
    let res_ok = make_response(200);
    assert!(res_ok.ok);
    assert_eq!(res_ok.status, 200);

    let res_err = make_response(400);
    assert!(!res_err.ok);
    assert_eq!(res_err.status, 400);

    let res_server_err = make_response(500);
    assert!(!res_server_err.ok);
}

// ---- Response メソッドテスト ----

#[test]
fn test_response_text() {
    let res = make_response_with_body(200, b"hello world");
    let text = res.text().expect("text() failed");
    assert_eq!(text, "hello world");
}

#[test]
fn test_response_json() {
    let body = br#"{"name":"Alice","age":30}"#;
    let res = make_response_with_body(200, body);
    let json = res.json().expect("json() failed");
    assert_eq!(json["name"], "Alice");
    assert_eq!(json["age"], 30);
}

// ---- mockito を使ったネットワークテスト ----

#[test]
fn test_send_get_mock() {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("GET", "/hello")
        .with_status(200)
        .with_header("content-type", "text/plain")
        .with_body("world")
        .create();

    let url = format!("{}/hello", server.url());
    let res = get(&url).send().expect("send failed");

    assert!(res.ok);
    assert_eq!(res.status, 200);
    assert_eq!(res.text().expect("text failed"), "world");
    mock.assert();
}

#[test]
fn test_send_post_json_mock() {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("POST", "/users")
        .match_header(
            "content-type",
            mockito::Matcher::Regex("application/json".to_string()),
        )
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":1}"#)
        .create();

    let url = format!("{}/users", server.url());
    let payload = serde_json::json!({ "name": "Alice" });
    let res = post(&url).json(payload).send().expect("send failed");

    assert!(res.ok);
    assert_eq!(res.status, 201);
    let json = res.json().expect("json failed");
    assert_eq!(json["id"], 1);
    mock.assert();
}

#[test]
fn test_retry_on_server_error() {
    let mut server = mockito::Server::new();
    // 最初の2回は 503、3回目は 200
    let mock_fail = server
        .mock("GET", "/flaky")
        .with_status(503)
        .expect(2)
        .create();
    let mock_ok = server
        .mock("GET", "/flaky")
        .with_status(200)
        .with_body("ok")
        .expect(1)
        .create();

    let url = format!("{}/flaky", server.url());
    let res = get(&url).retry(3).send().expect("send failed");

    assert!(res.ok);
    assert_eq!(res.status, 200);
    mock_fail.assert();
    mock_ok.assert();
}

// ---- ヘルパー ----

fn make_response(status: u16) -> Response {
    make_response_with_body(status, b"")
}

fn make_response_with_body(status: u16, body: &[u8]) -> Response {
    // Response は pub フィールドなので直接構築できる
    // ただし body_bytes は private なので forge_stdlib::http の pub コンストラクタを使う
    // → テスト用に from_parts を使う（または pub(crate) フィールドに変更）
    // ここでは mockito サーバー経由で作る代わりに、テスト用ヘルパーを http.rs に追加する
    use forge_stdlib::http::Response;
    Response::new_for_test(status, body.to_vec())
}
