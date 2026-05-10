//! 集成测试：需要本地 sqlai-sidecar 在 :8081 端口运行。
//! 跑法：`docker compose up -d sidecar && cargo test -p sqlai-llm -- --ignored`

use sqlai_llm::sidecar::{SidecarConfig, SidecarEmbedder, SidecarMlClient};
use sqlai_llm::EmbeddingProvider;

fn cfg() -> SidecarConfig {
    SidecarConfig {
        base_url: std::env::var("SIDECAR_URL").unwrap_or_else(|_| "http://127.0.0.1:8081".into()),
        timeout_secs: 600, // 首次调用会下载 BGE-M3，预留 10 分钟
    }
}

#[ignore]
#[tokio::test]
async fn embed_real_sidecar_returns_1024_dim_vectors() {
    let e = SidecarEmbedder::new(cfg()).unwrap();
    let r = e.embed(&["你好".into(), "world".into()]).await.unwrap();
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].len(), 1024);
    assert_eq!(r[1].len(), 1024);
    assert_ne!(r[0], r[1]);
}

#[ignore]
#[tokio::test]
async fn ml_kmeans_real_sidecar() {
    let c = SidecarMlClient::new(cfg()).unwrap();
    let body = serde_json::json!({
        "task": "kmeans",
        "params": {"n_clusters": 2, "random_state": 0},
        "data": [
            [0.0, 0.0], [0.1, 0.0], [0.0, 0.1],
            [10.0, 10.0], [10.1, 10.0], [10.0, 10.1]
        ]
    });
    let r = c.run(&body).await.unwrap();
    assert_eq!(r["task"], "kmeans");
    let labels = r["result"]["labels"].as_array().unwrap();
    assert_eq!(labels.len(), 6);
}
