//! Integration harness: spawns the real `represent-backend` binary with
//! `DEV_AUTH=1` against a temp data dir + stub dist/, polls `/status` until
//! up, and exposes a `reqwest` client. The child is killed on `Drop`.
//!
//! Tests are `#[ignore]` (they spawn a process + bind a port); run them with
//! `cargo test -p represent-e2e -- --ignored`.

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;

use tempfile::TempDir;

pub struct Stack {
    child: Child,
    pub base: String,
    pub client: reqwest::Client,
    pub data_dir: PathBuf,
    // Held so the temp dirs outlive the running binary.
    _data_tmp: TempDir,
    _static_tmp: TempDir,
}

impl Stack {
    pub async fn start() -> anyhow::Result<Self> {
        let data_tmp = tempfile::tempdir()?;
        let data_dir = data_tmp.path().to_path_buf();

        // Minimal static dir so the SPA fallback has something to serve.
        let static_tmp = tempfile::tempdir()?;
        std::fs::write(
            static_tmp.path().join("index.html"),
            "<html><body>represent</body></html>",
        )?;

        let port = free_port()?;
        let base = format!("http://127.0.0.1:{port}");

        let child = Command::new(bin_path())
            .env("DEV_AUTH", "1")
            .env("REPRESENT_BIND", format!("127.0.0.1:{port}"))
            .env("REPRESENT_DATA_DIR", &data_dir)
            .env("STATIC_DIR", static_tmp.path())
            .env("RUST_LOG", "warn")
            .spawn()?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()?;

        // Poll /status until the server is accepting.
        let mut up = false;
        for _ in 0..100 {
            if let Ok(r) = client.get(format!("{base}/status")).send().await {
                if r.status().is_success() {
                    up = true;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let stack = Stack {
            child,
            base,
            client,
            data_dir,
            _data_tmp: data_tmp,
            _static_tmp: static_tmp,
        };
        if !up {
            anyhow::bail!("backend did not come up within 10s");
        }
        Ok(stack)
    }

    pub async fn get(&self, route: &str) -> reqwest::Response {
        self.client
            .get(format!("{}{route}", self.base))
            .send()
            .await
            .expect("request failed")
    }

    pub async fn post_json(&self, route: &str, body: serde_json::Value) -> reqwest::Response {
        self.client
            .post(format!("{}{route}", self.base))
            .json(&body)
            .send()
            .await
            .expect("request failed")
    }

    pub async fn put_json(&self, route: &str, body: serde_json::Value) -> reqwest::Response {
        self.client
            .put(format!("{}{route}", self.base))
            .json(&body)
            .send()
            .await
            .expect("request failed")
    }

    pub async fn delete(&self, route: &str) -> reqwest::Response {
        self.client
            .delete(format!("{}{route}", self.base))
            .send()
            .await
            .expect("request failed")
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> anyhow::Result<u16> {
    let l = TcpListener::bind("127.0.0.1:0")?;
    Ok(l.local_addr()?.port())
}

/// The product binary sits next to the test binary's `deps/` dir.
fn bin_path() -> PathBuf {
    let mut p = std::env::current_exe().expect("current_exe");
    p.pop(); // remove test exe name
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("represent-backend")
}
