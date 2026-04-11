use forge_transpiler::transpile;

#[test]
fn test_transpile_http_get() {
    let src = r#"
        use forge/http.{ get }

        fn fetch_user(url: string) -> unit {
            let res = get(url).send()?
            println(res)
        }
    "#;

    let output = transpile(src).expect("transpile http get");

    // use reqwest; が生成される
    assert!(
        output.contains("use reqwest;"),
        "expected 'use reqwest;' in output, got:\n{}",
        output
    );
    // reqwest::Client::new().get(url) が生成される
    assert!(
        output.contains("reqwest::Client::new().get("),
        "expected reqwest get call in output, got:\n{}",
        output
    );
    // .send().await が生成される
    assert!(
        output.contains(".send().await"),
        "expected '.send().await' in output, got:\n{}",
        output
    );
    // async fn が生成される
    assert!(
        output.contains("async fn"),
        "expected 'async fn' in output, got:\n{}",
        output
    );
}

#[test]
fn test_transpile_http_post_json() {
    let src = r#"
        use forge/http.{ post }

        fn create_user(url: string, payload: any) -> unit {
            let res = post(url).json(payload).send()?
            println(res)
        }
    "#;

    let output = transpile(src).expect("transpile http post json");

    // use reqwest; が生成される
    assert!(
        output.contains("use reqwest;"),
        "expected 'use reqwest;' in output, got:\n{}",
        output
    );
    // reqwest::Client::new().post(url) が生成される
    assert!(
        output.contains("reqwest::Client::new().post("),
        "expected reqwest post call in output, got:\n{}",
        output
    );
    // .json(&payload) が生成される（参照付き）
    assert!(
        output.contains(".json(&"),
        "expected '.json(&...)' in output, got:\n{}",
        output
    );
    // .send().await が生成される
    assert!(
        output.contains(".send().await"),
        "expected '.send().await' in output, got:\n{}",
        output
    );
    // async fn が生成される
    assert!(
        output.contains("async fn"),
        "expected 'async fn' in output, got:\n{}",
        output
    );
}
