//! Supabase 配置和客户端

use serde::{Deserialize, Serialize};
use std::env;

/// Supabase 配置
#[derive(Debug, Clone)]
pub struct SupabaseConfig {
    pub url: String,
    pub anon_key: String,
    pub service_role_key: Option<String>,
}

impl Default for SupabaseConfig {
    fn default() -> Self {
        Self {
            url: env::var("SUPABASE_URL")
                .unwrap_or_else(|_| "https://gqrmqchgqiblpsvckioz.supabase.co".to_string()),
            anon_key: env::var("SUPABASE_ANON_KEY")
                .unwrap_or_else(|_| "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6Imdxcm1xY2hncWlibHBzdmNraW96Iiwicm9sZSI6ImFub24iLCJpYXQiOjE3ODgzMjkyMzEsImV4cCI6MjEwMzkwNTIzMX0.vC-wPFv25RW4kG1iaRoR0PSkA4Sz_FZEPhjPGGO1fuI".to_string()),
            service_role_key: env::var("SUPABASE_SERVICE_ROLE_KEY").ok(),
        }
    }
}

impl SupabaseConfig {
    /// 获取 Realtime WebSocket URL
    pub fn realtime_url(&self) -> String {
        self.url.replace("https://", "wss://") + "/realtime/v1"
    }

    /// 获取 REST API URL
    pub fn rest_url(&self) -> String {
        format!("{}/rest/v1", self.url)
    }
}

/// 设备信息（对应 devices 表）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: Option<String>,
    pub tenant_id: String,
    pub user_id: String,
    pub device_name: String,
    pub device_type: String,
    pub device_token: String,
    pub os_version: Option<String>,
    pub app_version: Option<String>,
    pub status: String,
    pub capabilities: serde_json::Value,
    pub last_seen_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

impl Device {
    /// 创建新设备
    pub fn new(
        tenant_id: String,
        user_id: String,
        device_name: String,
        device_type: String,
    ) -> Self {
        Self {
            id: None,
            tenant_id,
            user_id,
            device_name,
            device_type,
            device_token: uuid::Uuid::new_v4().to_string(),
            os_version: Some(std::env::consts::OS.to_string()),
            app_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            status: "online".to_string(),
            capabilities: serde_json::json!({
                "max_concurrent_tasks": 4,
                "supported_commands": ["shell", "code", "file"],
            }),
            last_seen_at: None,
            created_at: None,
            updated_at: None,
        }
    }
}

/// Supabase REST API 客户端
pub struct SupabaseRestClient {
    config: SupabaseConfig,
    client: reqwest::Client,
    access_token: Option<String>,
}

impl SupabaseRestClient {
    pub fn new(config: SupabaseConfig) -> Self {
        let client = reqwest::Client::builder()
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            client,
            access_token: None,
        }
    }

    /// 设置访问 Token
    pub fn with_access_token(mut self, token: String) -> Self {
        self.access_token = Some(token);
        self
    }

    /// 获取认证头
    fn auth_header(&self) -> String {
        self.access_token
            .as_ref()
            .unwrap_or(&self.config.anon_key)
            .clone()
    }

    /// 注册设备
    pub async fn register_device(&self, device: &Device) -> Result<Device, reqwest::Error> {
        let url = format!("{}/devices", self.config.rest_url());
        let token = self.auth_header();

        let response = self
            .client
            .post(&url)
            .header("apikey", &self.config.anon_key)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .header("Prefer", "return=representation")
            .json(device)
            .send()
            .await?;

        let devices: Vec<Device> = response.json().await?;
        Ok(devices.into_iter().next().unwrap())
    }

    /// 更新设备心跳
    pub async fn update_device_heartbeat(&self, device_id: &str) -> Result<(), reqwest::Error> {
        let url = format!("{}/devices?id=eq.{}", self.config.rest_url(), device_id);
        let token = self.auth_header();

        let now = chrono::Utc::now().to_rfc3339();
        let body = serde_json::json!({
            "last_seen_at": now,
            "status": "online",
        });

        self.client
            .patch(&url)
            .header("apikey", &self.config.anon_key)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        Ok(())
    }

    /// 更新设备状态
    pub async fn update_device_status(
        &self,
        device_id: &str,
        status: &str,
    ) -> Result<(), reqwest::Error> {
        let url = format!("{}/devices?id=eq.{}", self.config.rest_url(), device_id);
        let token = self.auth_header();

        let body = serde_json::json!({
            "status": status,
            "updated_at": chrono::Utc::now().to_rfc3339(),
        });

        self.client
            .patch(&url)
            .header("apikey", &self.config.anon_key)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        Ok(())
    }

    /// 更新任务状态
    pub async fn update_task_status(
        &self,
        task_id: &str,
        status: &str,
        progress: Option<i32>,
        logs: Option<Vec<String>>,
    ) -> Result<(), reqwest::Error> {
        let url = format!("{}/agent_tasks?id=eq.{}", self.config.rest_url(), task_id);
        let token = self.auth_header();

        let mut body = serde_json::json!({
            "status": status,
            "updated_at": chrono::Utc::now().to_rfc3339(),
        });

        if let Some(p) = progress {
            body["progress"] = serde_json::json!(p);
        }

        if let Some(l) = logs {
            body["logs"] = serde_json::json!(l);
        }

        self.client
            .patch(&url)
            .header("apikey", &self.config.anon_key)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        Ok(())
    }

    /// 获取待处理任务
    pub async fn get_pending_tasks(
        &self,
        device_id: &str,
    ) -> Result<Vec<serde_json::Value>, reqwest::Error> {
        let url = format!(
            "{}/agent_tasks?device_id=eq.{}&status=eq.queued&order=priority.desc,created_at.asc",
            self.config.rest_url(),
            device_id
        );
        let token = self.auth_header();

        let response = self
            .client
            .get(&url)
            .header("apikey", &self.config.anon_key)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await?;

        let tasks: Vec<serde_json::Value> = response.json().await?;
        Ok(tasks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supabase_config() {
        let config = SupabaseConfig::default();
        assert!(config.url.starts_with("https://"));
        assert!(config.realtime_url().starts_with("wss://"));
        assert!(config.rest_url().contains("/rest/v1"));
    }

    #[test]
    fn test_device_creation() {
        let device = Device::new(
            "tenant-1".to_string(),
            "user-1".to_string(),
            "My PC".to_string(),
            "windows".to_string(),
        );

        assert_eq!(device.status, "online");
        assert_eq!(device.device_type, "windows");
        assert!(!device.device_token.is_empty());
    }
}
