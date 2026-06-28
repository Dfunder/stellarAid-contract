use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub url: String,
    pub secret: Option<String>,
    pub events: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    pub event: String,
    pub campaign_id: u64,
    pub donor: String,
    pub amount: String,
    pub tx_hash: String,
    pub timestamp: u64,
}

type WebhookStore = Arc<RwLock<HashMap<String, Vec<WebhookConfig>>>>;

#[derive(Clone)]
pub struct WebhookManager {
    client: Client,
    store: WebhookStore,
}

impl WebhookManager {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register(&self, campaign_id: u64, config: WebhookConfig) {
        let key = campaign_id.to_string();
        let mut store = self.store.write().await;
        store.entry(key).or_default().push(config);
        info!(campaign_id = campaign_id, "webhook registered");
    }

    pub async fn dispatch(&self, campaign_id: u64, payload: WebhookPayload) {
        let key = campaign_id.to_string();
        let configs = {
            let store = self.store.read().await;
            store.get(&key).cloned().unwrap_or_default()
        };

        for config in &configs {
            if !config.events.is_empty() && !config.events.contains(&payload.event) {
                continue;
            }

            let mut req = self.client.post(&config.url).json(&payload);
            if let Some(secret) = &config.secret {
                req = req.header("X-Webhook-Secret", secret);
            }

            match req.send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        info!(url = %config.url, event = %payload.event, "webhook delivered");
                    } else {
                        warn!(
                            url = %config.url,
                            status = %resp.status(),
                            "webhook delivery returned non-success"
                        );
                    }
                }
                Err(e) => {
                    error!(url = %config.url, error = %e, "webhook delivery failed");
                }
            }
        }
    }
}

impl Default for WebhookManager {
    fn default() -> Self {
        Self::new()
    }
}
