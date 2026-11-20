//! 设备管理器（注册、心跳、状态管理）

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use super::error::{QueueError, Result};
use super::supabase::{Device, SupabaseConfig, SupabaseRestClient};

/// 设备管理器
pub struct DeviceManager {
    rest_client: Arc<SupabaseRestClient>,
    device: Arc<RwLock<Option<Device>>>,
    heartbeat_interval: Duration,
}

impl DeviceManager {
    /// 创建新的设备管理器
    pub fn new(config: SupabaseConfig) -> Self {
        let rest_client = Arc::new(SupabaseRestClient::new(config));

        Self {
            rest_client,
            device: Arc::new(RwLock::new(None)),
            heartbeat_interval: Duration::from_secs(30),
        }
    }

    /// 设置心跳间隔
    pub fn with_heartbeat_interval(mut self, interval: Duration) -> Self {
        self.heartbeat_interval = interval;
        self
    }

    /// 注册设备
    pub async fn register(&self, device: Device) -> Result<String> {
        tracing::info!("注册设备: {}", device.device_name);

        let registered_device = self
            .rest_client
            .register_device(&device)
            .await
            .map_err(|e| QueueError::NetworkError(format!("设备注册失败: {}", e)))?;

        let device_id = registered_device
            .id
            .clone()
            .ok_or_else(|| QueueError::Internal("设备 ID 为空".to_string()))?;

        // 保存设备信息
        {
            let mut dev = self.device.write().await;
            *dev = Some(registered_device);
        }

        tracing::info!("设备注册成功: {}", device_id);
        Ok(device_id)
    }

    /// 启动心跳
    pub async fn start_heartbeat(&self) -> Result<()> {
        let device_id = {
            let dev = self.device.read().await;
            dev.as_ref()
                .and_then(|d| d.id.clone())
                .ok_or_else(|| QueueError::Internal("设备未注册".to_string()))?
        };

        let rest_client = self.rest_client.clone();
        let interval = self.heartbeat_interval;

        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);

            loop {
                interval_timer.tick().await;

                match rest_client.update_device_heartbeat(&device_id).await {
                    Ok(_) => {
                        tracing::debug!("心跳发送成功");
                    }
                    Err(e) => {
                        tracing::error!("心跳发送失败: {}", e);
                    }
                }
            }
        });

        tracing::info!("心跳已启动，间隔: {:?}", interval);
        Ok(())
    }

    /// 更新设备状态
    pub async fn update_status(&self, status: &str) -> Result<()> {
        let device_id = {
            let dev = self.device.read().await;
            dev.as_ref()
                .and_then(|d| d.id.clone())
                .ok_or_else(|| QueueError::Internal("设备未注册".to_string()))?
        };

        self.rest_client
            .update_device_status(&device_id, status)
            .await
            .map_err(|e| QueueError::NetworkError(format!("更新设备状态失败: {}", e)))?;

        tracing::info!("设备状态已更新: {}", status);
        Ok(())
    }

    /// 获取设备 ID
    pub async fn device_id(&self) -> Option<String> {
        let dev = self.device.read().await;
        dev.as_ref().and_then(|d| d.id.clone())
    }

    /// 获取设备信息
    pub async fn device(&self) -> Option<Device> {
        let dev = self.device.read().await;
        dev.clone()
    }

    /// 设备离线
    pub async fn go_offline(&self) -> Result<()> {
        self.update_status("offline").await
    }

    /// 设备上线
    pub async fn go_online(&self) -> Result<()> {
        self.update_status("online").await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_device_manager_creation() {
        let config = SupabaseConfig::default();
        let manager = DeviceManager::new(config);

        assert_eq!(manager.heartbeat_interval, Duration::from_secs(30));
        assert!(manager.device_id().await.is_none());
    }

    #[tokio::test]
    async fn test_device_manager_with_custom_interval() {
        let config = SupabaseConfig::default();
        let manager = DeviceManager::new(config).with_heartbeat_interval(Duration::from_secs(60));

        assert_eq!(manager.heartbeat_interval, Duration::from_secs(60));
    }
}
