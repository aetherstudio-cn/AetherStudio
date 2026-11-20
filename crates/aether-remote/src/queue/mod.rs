//! 任务队列模块
//!
//! 提供生产级任务队列功能，包括：
//! - AetherDB 持久化存储
//! - 内存优先级队列
//! - 并发任务执行器
//! - 依赖图管理
//! - 自动重试策略
//! - Realtime 监听
//! - 监控指标
//! - 设备注册和心跳
//! - 实时状态同步
//! - 用户认证和会话管理

pub mod auth;
pub mod command_executor;
pub mod config;
pub mod dependency;
pub mod device;
pub mod error;
pub mod executor;
pub mod kind;
pub mod metrics;
pub mod models;
pub mod persistence;
pub mod priority_queue;
pub mod realtime;
pub mod retry;
pub mod session;
pub mod supabase;
pub mod system;

// 重新导出常用类型
pub use auth::{AuthSession, SupabaseAuthClient, User};
pub use command_executor::CommandExecutor;
pub use config::QueueConfig;
pub use device::DeviceManager;
pub use error::{QueueError, Result};
pub use models::{QueueState, Task, TaskLog, TaskResult, TaskStatus};
pub use persistence::AetherTaskStore;
pub use session::SessionManager;
pub use supabase::{Device, SupabaseConfig, SupabaseRestClient};
pub use system::TaskQueueSystem;
