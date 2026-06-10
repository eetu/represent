//! End-to-end API tests against the spawned binary. `#[ignore]` by default —
//! run with `cargo test -p represent-e2e -- --ignored`.

use represent_e2e::Stack;
use serde_json::json;

#[tokio::test]
#[ignore]
async fn status_is_unauthenticated_and_reports_store() {
    let s = Stack::start().await.unwrap();
    let r = s.get("/status").await;
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.unwrap();
    assert_eq!(body["service"], "represent");
    assert_eq!(body["db_healthy"], true);
    assert_eq!(body["project_count"], 0);
    // No OIDC configured in the harness.
    assert!(body["oidc_healthy"].is_null());
}

#[tokio::test]
#[ignore]
async fn dev_login_isolates_users() {
    let s = Stack::start().await.unwrap();

    // Alice logs in (dev session cookie) and creates a project.
    let r = s.get("/auth/login?username=alice&next=/").await;
    assert!(r.status().is_success()); // followed the redirect to the SPA shell
    s.post_json("/api/projects", json!({ "name": "alices" })).await;
    let body: serde_json::Value = s.get("/api/me").await.json().await.unwrap();
    assert_eq!(body["email"], "alice@local");

    // Bob logs in on the same client — the cookie swaps, Alice's data is gone.
    s.get("/auth/login?username=bob&next=/").await;
    let body: serde_json::Value = s.get("/api/projects").await.json().await.unwrap();
    assert_eq!(body["projects"].as_array().unwrap().len(), 0);
    // Same name in Bob's namespace is fine.
    let r = s.post_json("/api/projects", json!({ "name": "alices" })).await;
    assert_eq!(r.status(), reqwest::StatusCode::CREATED);

    // Logout drops the session; DEV_AUTH then falls back to the synthetic
    // dev profile (still authenticated — bypass is the point of dev mode).
    let r = s.post_json("/auth/logout", json!({})).await;
    assert_eq!(r.status(), reqwest::StatusCode::NO_CONTENT);
    let body: serde_json::Value = s.get("/api/me").await.json().await.unwrap();
    assert_eq!(body["email"], "dev@localhost");
}

#[tokio::test]
#[ignore]
async fn project_and_file_crud_roundtrip() {
    let s = Stack::start().await.unwrap();

    let r = s.post_json("/api/projects", json!({ "name": "demo" })).await;
    assert_eq!(r.status(), reqwest::StatusCode::CREATED);
    // Duplicate → 409.
    let r = s.post_json("/api/projects", json!({ "name": "demo" })).await;
    assert_eq!(r.status(), reqwest::StatusCode::CONFLICT);

    let r = s
        .put_json(
            "/api/projects/demo/files/01-intro.md",
            json!({ "content": "---\ntimer: 90\n---\n# intro\n" }),
        )
        .await;
    assert_eq!(r.status(), reqwest::StatusCode::NO_CONTENT);

    let body: serde_json::Value = s.get("/api/projects/demo/files").await.json().await.unwrap();
    assert_eq!(body["files"][0]["name"], "01-intro.md");

    let body: serde_json::Value = s
        .get("/api/projects/demo/files/01-intro.md")
        .await
        .json()
        .await
        .unwrap();
    assert!(body["content"].as_str().unwrap().contains("timer: 90"));

    let body: serde_json::Value = s.get("/api/projects").await.json().await.unwrap();
    assert_eq!(body["projects"][0]["file_count"], 1);

    let r = s.delete("/api/projects/demo/files/01-intro.md").await;
    assert_eq!(r.status(), reqwest::StatusCode::NO_CONTENT);
    let r = s.delete("/api/projects/demo").await;
    assert_eq!(r.status(), reqwest::StatusCode::NO_CONTENT);
}

#[tokio::test]
#[ignore]
async fn reorder_persists_without_renaming() {
    let s = Stack::start().await.unwrap();
    s.post_json("/api/projects", json!({ "name": "demo" })).await;
    s.put_json("/api/projects/demo/files/alpha.md", json!({ "content": "a" }))
        .await;
    s.put_json("/api/projects/demo/files/beta.md", json!({ "content": "b" }))
        .await;

    let r = s
        .post_json(
            "/api/projects/demo/reorder",
            json!({ "files": ["beta.md", "alpha.md"] }),
        )
        .await;
    assert!(r.status().is_success());
    let body: serde_json::Value = r.json().await.unwrap();
    // Names untouched; order is metadata.
    assert_eq!(body["files"][0]["name"], "beta.md");
    assert_eq!(body["files"][1]["name"], "alpha.md");

    // The order survives a fresh listing.
    let body: serde_json::Value = s.get("/api/projects/demo/files").await.json().await.unwrap();
    assert_eq!(body["files"][0]["name"], "beta.md");

    // Not a permutation → 400.
    let r = s
        .post_json("/api/projects/demo/reorder", json!({ "files": ["beta.md"] }))
        .await;
    assert_eq!(r.status(), reqwest::StatusCode::BAD_REQUEST);

    // Rename keeps the order slot; clashing target → 409.
    let r = s
        .post_json(
            "/api/projects/demo/files/beta.md/rename",
            json!({ "to": "intro.md" }),
        )
        .await;
    assert_eq!(r.status(), reqwest::StatusCode::NO_CONTENT);
    let body: serde_json::Value = s.get("/api/projects/demo/files").await.json().await.unwrap();
    assert_eq!(body["files"][0]["name"], "intro.md");
    let r = s
        .post_json(
            "/api/projects/demo/files/intro.md/rename",
            json!({ "to": "alpha.md" }),
        )
        .await;
    assert_eq!(r.status(), reqwest::StatusCode::CONFLICT);
}

#[tokio::test]
#[ignore]
async fn bundle_downloads_zip() {
    let s = Stack::start().await.unwrap();
    s.post_json("/api/projects", json!({ "name": "demo" })).await;
    s.put_json("/api/projects/demo/files/a.md", json!({ "content": "# a" }))
        .await;

    let r = s.get("/api/projects/demo/bundle").await;
    assert!(r.status().is_success());
    assert_eq!(
        r.headers()["content-type"].to_str().unwrap(),
        "application/zip"
    );
    assert!(r.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .contains("demo.zip"));
    let bytes = r.bytes().await.unwrap();
    assert_eq!(&bytes[..2], b"PK");
}

#[tokio::test]
#[ignore]
async fn traversal_names_are_rejected() {
    let s = Stack::start().await.unwrap();
    // Encoded `..%2f..%2fetc` decodes to a slash-bearing segment — 400, and
    // nothing escapes the data dir.
    let r = s
        .put_json(
            "/api/projects/demo/files/..%2F..%2Fpwned.md",
            json!({ "content": "x" }),
        )
        .await;
    assert!(r.status().is_client_error());
    let r = s.post_json("/api/projects", json!({ "name": ".hidden" })).await;
    assert_eq!(r.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[ignore]
async fn spa_fallback_serves_shell_with_200() {
    let s = Stack::start().await.unwrap();
    let r = s.get("/p/demo/f/01-intro.md").await;
    assert!(r.status().is_success());
    assert!(r.text().await.unwrap().contains("represent"));
}
