//! Realtime 监听器实现（Supabase Realtime）

use std::time::Duration;
use tokio::sync::mpsc;

use supabase_realtime_rs::{
    PostgresChangeEvent, PostgresChangesFilter, PostgresChangesPayload,
    RealtimeClient, RealtimeClientOptions,
};

use super::error::{QueueError, Result};
use super::models::{Task, TaskStatus};
use super::supabase::SupabaseConfig;

/// Realtime 任务监听器
pub struct RealtimeTaskListener {
    config: SupabaseConfig,
    device_id: String,
    reconnect_interval: Duration,
    max_reconnect_attempts: usize,
}

impl RealtimeTaskListener {
    /// 创建新的 Realtime 监听器
    pub fn new(config: SupabaseConfig, device_id: String) -> Self {
        Self {
            config,
            device_id,
            reconnect_interval: Duration::from_secs(5),
            max_reconnect_attempts: 10,
        }
    }

    /// 设置重连间隔
    pub fn with_reconnect_interval(mut self, interval: Duration) -> Self {
        self.reconnect_interval = interval;
        self
    }

    /// 设置最大重连次数
    pub fn with_max_reconnect_attempts(mut self, attempts: usize) -> Self {
        self.max_reconnect_attempts = attempts;
        self
    }

    /// 启动监听（返回任务接收通道）
    pub async fn start(&self) -> Result<mpsc::Receiver<Task>> {
        let (tx, rx) = mpsc::channel(100);
        
        let config = self.config.clone();
        let device_id = self.device_id.clone();
        let reconnect_interval = self.reconnect_interval;
        let max_attempts = self.max_reconnect_attempts;
        
        tokio::spawn(async move {
            let mut attempt = 0;
            
            loop {
                match Self::connect_and_listen(&config, &device_id, tx.clone()).await {
                    Ok(_) => {
                        tracing::info!("Realtime 连接正常关闭");
                        break;
                    }
                    Err(e) => {
                        attempt += 1;
                        tracing::error!("Realtime 连接错误 (尝试 {}/{}): {}", attempt, max_attempts, e);
                        
                        if attempt >= max_attempts {
                            tracing::error!("达到最大重连次数，停止监听");
                            break;
                        }
                        
                        tokio::time::sleep(reconnect_interval).await;
                    }
                }
            }
        });
        
        Ok(rx)
    }

    /// 连接并监听 Supabase Realtime
    async fn connect_and_listen(
        config: &SupabaseConfig,
        device_id: &str,
        tx: mpsc::Sender<Task>,
    ) -> Result<()> {
        tracing::info!("连接到 Supabase Realtime: {}", config.realtime_url());
        
        // 创建 Realtime 客户端
        let client = RealtimeClient::new(
            &config.realtime_url(),
            RealtimeClientOptions {
                api_key: config.anon_key.clone(),
                ..Default::default()
            }
        ).map_err(|e| QueueError::NetworkError(format!("创建 Realtime 客户端失败: {}", e)))?;

        // 连接
        client.connect().await
            .map_err(|e| QueueError::NetworkError(format!("连接失败: {}", e)))?;

        tracing::info!("Realtime 连接成功");

        // 创建频道
        let channel = client.channel("agent_tasks_changes", Default::default()).await;

        // 监听 agent_tasks 表的 INSERT 事件
        let mut insert_rx = channel.on_postgres_changes(
            PostgresChangesFilter::new(PostgresChangeEvent::Insert, "public")
                .table("agent_tasks")
        ).await;

        // 监听 agent_tasks 表的 UPDATE 事件
        let mut update_rx = channel.on_postgres_changes(
            PostgresChangesFilter::new(PostgresChangeEvent::Update, "public")
                .table("agent_tasks")
        ).await;

        // 订阅频道
        channel.subscribe().await
            .map_err(|e| QueueError::NetworkError(format!("订阅失败: {}", e)))?;

        tracing::info!("已订阅 agent_tasks 表的变化");

        let device_id = device_id.to_string();
        
        // 处理 INSERT 事件
        let tx_insert = tx.clone();
        let device_id_insert = device_id.clone();
        tokio::spawn(async move {
            while let Some(payload) = insert_rx.recv().await {
                if let PostgresChangesPayload::Insert(insert) = payload {
                    tracing::info!("收到新任务 INSERT 事件");
                    
                    // 解析任务
                    if let Ok(task) = Self::parse_task_from_payload(&insert.new) {
                        // 只处理当前设备的任务
                        if task.device_id.as_ref().map(|d| d.to_string()) == Some(device_id_insert.clone()) {
                            if let Err(e) = tx_insert.send(task).await {
                                tracing::error!("发送任务失败: {}", e);
                            }
                        }
                    }
                }
            }
        });

        // 处理 UPDATE 事件
        let device_id_update = device_id.clone();
        tokio::spawn(async move {
            while let Some(payload) = update_rx.recv().await {
                if let PostgresChangesPayload::Update(update) = payload {
                    tracing::debug!("收到任务 UPDATE 事件");
                    
                    // 可以在这里处理任务更新（如取消）
                    if let Ok(task) = Self::parse_task_from_payload(&update.new) {
                        if task.device_id.as_ref().map(|d| d.to_string()) == Some(device_id_update.clone()) {
                            if task.status == TaskStatus::Cancelled {
                                tracing::info!("任务 {} 被取消", task.id);
                                // TODO: 发送取消信号
                            }
                        }
                    }
                }
            }
        });

        // 保持连接
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            
            // 检查连接状态
            // TODO: 实现连接状态检查
        }
    }

    /// 从 Postgres payload 解析任务
    fn parse_task_from_payload(payload: &std::collections::HashMap<String, serde_json::Value>) -> Result<Task> {
        let value = serde_json::to_value(payload)
            .map_err(|e| QueueError::SerializationError(format!("转换 payload 失败: {}", e)))?;
        serde_json::from_value(value)
            .map_err(|e| QueueError::SerializationError(format!("解析任务失败: {}", e)))
    }
}

/// 轮询任务监听器（降级方案）
pub struct PollingTaskListener {
    config: SupabaseConfig,
    device_id: String,
    poll_interval: Duration,
}

impl PollingTaskListener {
    /// 创建新的轮询监听器
    pub fn new(config: SupabaseConfig, device_id: String, poll_interval: Duration) -> Self {
        Self {
            config,
            device_id,
            poll_interval,
        }
    }

    /// 启动轮询
    pub async fn start(&self) -> Result<mpsc::Receiver<Task>> {
        let (tx, rx) = mpsc::channel(100);
        
        let config = self.config.clone();
        let device_id = self.device_id.clone();
        let poll_interval = self.poll_interval;
        
        tokio::spawn(async move {
            let rest_client = super::supabase::SupabaseRestClient::new(config);
            
            loop {
                match rest_client.get_pending_tasks(&device_id).await {
                    Ok(tasks_json) => {
                        for task_json in tasks_json {
                            if let Ok(task) = serde_json::from_value::<Task>(task_json) {
                                if let Err(e) = tx.send(task).await {
                                    tracing::error!("发送任务失败: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("轮询任务失败: {}", e);
                    }
                }
                
                tokio::time::sleep(poll_interval).await;
            }
        });
        
        Ok(rx)
    }
}

/// 混合监听器（Realtime + 轮询降级）
pub struct HybridTaskListener {
    realtime: RealtimeTaskListener,
    polling: PollingTaskListener,
}

impl HybridTaskListener {
    /// 创建混合监听器
    pub fn new(config: SupabaseConfig, device_id: String) -> Self {
        Self {
            realtime: RealtimeTaskListener::new(config.clone(), device_id.clone()),
            polling: PollingTaskListener::new(config, device_id, Duration::from_secs(30)),
        }
    }

    /// 启动监听（自动降级）
    pub async fn start(&self) -> Result<mpsc::Receiver<Task>> {
        let (tx, rx) = mpsc::channel(100);
        
        // 尝试 Realtime
        match self.realtime.start().await {
            Ok(mut realtime_rx) => {
                tracing::info!("使用 Realtime 模式");
                
                let tx_clone = tx.clone();
                tokio::spawn(async move {
                    while let Some(task) = realtime_rx.recv().await {
                        if let Err(e) = tx_clone.send(task).await {
                            tracing::error!("转发任务失败: {}", e);
                        }
                    }
                });
            }
            Err(e) => {
                tracing::warn!("Realtime 连接失败，降级到轮询模式: {}", e);
                
                match self.polling.start().await {
                    Ok(mut polling_rx) => {
                        tokio::spawn(async move {
                            while let Some(task) = polling_rx.recv().await {
                                if let Err(e) = tx.send(task).await {
                                    tracing::error!("转发任务失败: {}", e);
                                }
                            }
                        });
                    }
                    Err(e) => {
                        return Err(QueueError::Internal(format!(
                            "轮询模式也失败: {}",
                            e
                        )));
                    }
                }
            }
        }
        
        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_realtime_listener_creation() {
        let config = SupabaseConfig::default();
        let listener = RealtimeTaskListener::new(config, "device-1".to_string());
        assert_eq!(listener.device_id, "device-1");
        assert_eq!(listener.reconnect_interval, Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_polling_listener_creation() {
        let config = SupabaseConfig::default();
        let listener = PollingTaskListener::new(
            config,
            "device-1".to_string(),
            Duration::from_secs(10),
        );
        assert_eq!(listener.device_id, "device-1");
        assert_eq!(listener.poll_interval, Duration::from_secs(10));
    }

    #[tokio::test]
    async fn test_hybrid_listener_creation() {
        let config = SupabaseConfig::default();
        let listener = HybridTaskListener::new(config, "device-1".to_string());
        assert_eq!(listener.realtime.device_id, "device-1");
        assert_eq!(listener.polling.device_id, "device-1");
    }
}
