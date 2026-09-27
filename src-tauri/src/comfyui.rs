use futures_util::StreamExt;
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum ComfyError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Server error: {0}")]
    Server(String),
}

impl Serialize for ComfyError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemStats {
    pub system: SystemInfo,
    pub devices: Vec<DeviceInfo>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemInfo {
    pub os: String,
    pub ram_total: u64,
    pub ram_free: u64,
    pub comfyui_version: Option<String>,
    pub python_version: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeviceInfo {
    pub name: String,
    pub r#type: String,
    pub vram_total: u64,
    pub vram_free: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadResult {
    pub name: String,
    pub subfolder: String,
    pub r#type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PromptResponse {
    pub prompt_id: String,
    pub number: u32,
    #[serde(default)]
    pub node_errors: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryOutput {
    pub images: Option<Vec<ImageOutput>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ImageOutput {
    pub filename: String,
    pub subfolder: String,
    pub r#type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PromptHistory {
    pub prompt: serde_json::Value,
    pub outputs: HashMap<String, HistoryOutput>,
    pub status: Option<PromptStatus>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PromptStatus {
    pub status_str: String,
    pub completed: bool,
    #[serde(default)]
    pub messages: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HealthCheck {
    pub reachable: bool,
    pub gpu_name: Option<String>,
    pub vram_total_gb: Option<f64>,
    pub vram_free_gb: Option<f64>,
    pub comfyui_version: Option<String>,
    pub missing_nodes: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressUpdate {
    pub stage: String,
    pub value: u32,
    pub max: u32,
    pub percent: f32,
}

pub struct ComfyClient {
    base_url: String,
    client: reqwest::Client,
    client_id: String,
}

impl ComfyClient {
    pub fn new(base_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
            client_id: Uuid::new_v4().to_string(),
        }
    }

    pub async fn system_stats(&self) -> Result<SystemStats, ComfyError> {
        let url = format!("{}/system_stats", self.base_url);
        let resp = self.client.get(&url).send().await?;

        if !resp.status().is_success() {
            return Err(ComfyError::Server(format!(
                "system_stats failed: {}",
                resp.status()
            )));
        }

        Ok(resp.json().await?)
    }

    pub async fn object_info(&self) -> Result<HashMap<String, serde_json::Value>, ComfyError> {
        let url = format!("{}/object_info", self.base_url);
        let resp = self.client.get(&url).send().await?;

        if !resp.status().is_success() {
            return Err(ComfyError::Server(format!(
                "object_info failed: {}",
                resp.status()
            )));
        }

        Ok(resp.json().await?)
    }

    pub async fn check_nodes(&self, required: &[&str]) -> Result<Vec<String>, ComfyError> {
        let info = self.object_info().await?;
        let mut missing = Vec::new();

        for node in required {
            if !info.contains_key(*node) {
                missing.push(node.to_string());
            }
        }

        Ok(missing)
    }

    pub async fn health_check(&self, required_nodes: &[&str]) -> HealthCheck {
        let mut health = HealthCheck {
            reachable: false,
            gpu_name: None,
            vram_total_gb: None,
            vram_free_gb: None,
            comfyui_version: None,
            missing_nodes: Vec::new(),
            error: None,
        };

        match self.system_stats().await {
            Ok(stats) => {
                health.reachable = true;
                health.comfyui_version = stats.system.comfyui_version;

                if let Some(device) = stats.devices.first() {
                    health.gpu_name = Some(device.name.clone());
                    health.vram_total_gb =
                        Some((device.vram_total as f64) / (1024.0 * 1024.0 * 1024.0));
                    health.vram_free_gb =
                        Some((device.vram_free as f64) / (1024.0 * 1024.0 * 1024.0));
                }
            }
            Err(e) => {
                health.error = Some(format!("Cannot reach server: {}", e));
                return health;
            }
        }

        match self.check_nodes(required_nodes).await {
            Ok(missing) => {
                health.missing_nodes = missing;
            }
            Err(e) => {
                health.error = Some(format!("Cannot check nodes: {}", e));
            }
        }

        health
    }

    pub async fn upload_image(&self, image_data: &[u8], filename: &str) -> Result<UploadResult, ComfyError> {
        let url = format!("{}/upload/image", self.base_url);

        let part = Part::bytes(image_data.to_vec())
            .file_name(filename.to_string())
            .mime_str("image/png")
            .unwrap();

        let form = Form::new()
            .part("image", part)
            .text("type", "input")
            .text("overwrite", "true");

        let resp = self.client.post(&url).multipart(form).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ComfyError::Server(format!(
                "upload failed: {} - {}",
                status, text
            )));
        }

        Ok(resp.json().await?)
    }

    pub async fn queue_prompt(
        &self,
        prompt: serde_json::Value,
    ) -> Result<PromptResponse, ComfyError> {
        let url = format!("{}/prompt", self.base_url);

        let payload = serde_json::json!({
            "prompt": prompt,
            "client_id": self.client_id,
        });

        let resp = self.client.post(&url).json(&payload).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ComfyError::Server(format!(
                "queue failed: {} - {}",
                status, text
            )));
        }

        Ok(resp.json().await?)
    }

    pub async fn get_history(
        &self,
        prompt_id: &str,
    ) -> Result<Option<PromptHistory>, ComfyError> {
        let url = format!("{}/history/{}", self.base_url, prompt_id);
        let resp = self.client.get(&url).send().await?;

        if !resp.status().is_success() {
            return Err(ComfyError::Server(format!(
                "history failed: {}",
                resp.status()
            )));
        }

        let history: HashMap<String, PromptHistory> = resp.json().await?;
        Ok(history.get(prompt_id).cloned())
    }

    pub async fn get_image(&self, filename: &str, subfolder: &str, img_type: &str) -> Result<Vec<u8>, ComfyError> {
        let url = format!(
            "{}/view?filename={}&subfolder={}&type={}",
            self.base_url,
            urlencoding::encode(filename),
            urlencoding::encode(subfolder),
            urlencoding::encode(img_type)
        );

        let resp = self.client.get(&url).send().await?;

        if !resp.status().is_success() {
            return Err(ComfyError::Server(format!(
                "view failed: {}",
                resp.status()
            )));
        }

        Ok(resp.bytes().await?.to_vec())
    }

    pub fn ws_url(&self) -> String {
        let ws_base = self
            .base_url
            .replace("http://", "ws://")
            .replace("https://", "wss://");
        format!("{}/ws?clientId={}", ws_base, self.client_id)
    }

    pub async fn watch_progress<F>(
        &self,
        prompt_id: &str,
        mut callback: F,
    ) -> Result<(), ComfyError>
    where
        F: FnMut(ProgressUpdate),
    {
        let ws_url = self.ws_url();
        let (ws_stream, _) = connect_async(&ws_url).await?;
        let (mut _write, mut read) = ws_stream.split();

        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) {
                        let msg_type = data.get("type").and_then(|t| t.as_str());
                        let msg_data = data.get("data");

                        match (msg_type, msg_data) {
                            (Some("progress"), Some(d)) => {
                                let value = d.get("value").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                                let max = d.get("max").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
                                let percent = if max > 0 {
                                    (value as f32 / max as f32) * 100.0
                                } else {
                                    0.0
                                };
                                callback(ProgressUpdate {
                                    stage: "sampling".to_string(),
                                    value,
                                    max,
                                    percent,
                                });
                            }
                            (Some("executing"), Some(d)) => {
                                if let Some(current_prompt_id) = d.get("prompt_id").and_then(|p| p.as_str()) {
                                    if current_prompt_id == prompt_id {
                                        if d.get("node").is_none() {
                                            callback(ProgressUpdate {
                                                stage: "complete".to_string(),
                                                value: 100,
                                                max: 100,
                                                percent: 100.0,
                                            });
                                            return Ok(());
                                        } else {
                                            callback(ProgressUpdate {
                                                stage: "executing".to_string(),
                                                value: 0,
                                                max: 100,
                                                percent: 0.0,
                                            });
                                        }
                                    }
                                }
                            }
                            (Some("executed"), Some(d)) => {
                                if let Some(current_prompt_id) = d.get("prompt_id").and_then(|p| p.as_str()) {
                                    if current_prompt_id == prompt_id {
                                        callback(ProgressUpdate {
                                            stage: "complete".to_string(),
                                            value: 100,
                                            max: 100,
                                            percent: 100.0,
                                        });
                                        return Ok(());
                                    }
                                }
                            }
                            (Some("execution_error"), Some(d)) => {
                                if let Some(current_prompt_id) = d.get("prompt_id").and_then(|p| p.as_str()) {
                                    if current_prompt_id == prompt_id {
                                        let error_msg = d.get("exception_message")
                                            .and_then(|e| e.as_str())
                                            .unwrap_or("Unknown error");
                                        return Err(ComfyError::Server(error_msg.to_string()));
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(e) => return Err(ComfyError::WebSocket(e)),
                _ => {}
            }
        }

        Ok(())
    }
}
