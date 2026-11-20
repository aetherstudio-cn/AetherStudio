# 任务队列系统

基于 AetherDB 的生产级任务队列系统，支持优先级调度、并发控制、依赖管理、崩溃恢复、实时监控。

## 特性

- **持久化存储**: 使用 AetherDB 单文件存储，支持崩溃恢复
- **优先级队列**: 基于优先级的任务调度
- **并发控制**: 信号量限制的并发执行池
- **依赖管理**: 有向无环图（DAG）依赖关系
- **自动重试**: 指数退避重试策略
- **实时监控**: Prometheus 指标导出
- **Realtime 监听**: Supabase Realtime 事件订阅（支持降级到轮询）

## 快速开始

### 基本使用

```rust
use aether_remote::queue::{QueueConfig, Task, TaskQueueSystem};
use std::path::PathBuf;
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 创建配置
    let config = QueueConfig::new(
        PathBuf::from("data/tasks.aedb"),
        "device-1".to_string(),
    )
    .with_max_concurrent(4)
    .with_max_queue_size(1000);

    // 2. 创建任务队列系统
    let system = TaskQueueSystem::new(config).await?;

    // 3. 启动系统
    system.start().await?;

    // 4. 提交任务
    let task = Task::new(
        Uuid::new_v4(),  // tenant_id
        Uuid::new_v4(),  // user_id
        "我的任务".to_string(),
        "echo Hello".to_string(),
    );
    system.submit_task(task.clone()).await?;

    // 5. 查询状态
    if let Some(status) = system.get_task_status(task.id).await {
        println!("任务状态: {:?}", status);
    }

    // 6. 关闭系统
    system.shutdown().await?;

    Ok(())
}
```

### 优先级任务

```rust
let mut task = Task::new(
    tenant_id,
    user_id,
    "高优先级任务".to_string(),
    "important_command".to_string(),
);
task.priority = 10;  // 优先级越高越先执行
system.submit_task(task).await?;
```

### 依赖任务

```rust
let task1 = Task::new(/* ... */);
system.submit_task(task1.clone()).await?;

let mut task2 = Task::new(/* ... */);
task2.dependencies = vec![task1.id];  // task2 依赖 task1
system.submit_task(task2).await?;
```

### 取消任务

```rust
system.cancel_task(task_id).await?;
```

### 获取任务日志

```rust
let logs = system.get_task_logs(task_id).await;
for log in logs {
    println!("[{:?}] {}", log.level, log.message);
}
```

### 导出监控指标

```rust
let metrics = system.metrics();
let prometheus_output = metrics.export();
println!("{}", prometheus_output);
```

## 配置选项

```rust
let config = QueueConfig::new(db_path, device_id)
    .with_max_concurrent(8)              // 最大并发任务数
    .with_max_queue_size(2000)           // 队列最大容量
    .with_retention_days(60)             // 任务保留天数
    .with_polling_interval(Duration::from_secs(30))  // 轮询间隔
    .with_enable_metrics(true);          // 是否启用监控
```

## 架构

```
┌─────────────────────────────────────────┐
│         TaskQueueSystem                 │
│  ┌──────────────────────────────────┐   │
│  │   Realtime 监听器                 │   │
│  │   (Supabase Realtime / 轮询)      │   │
│  └──────────────────────────────────┘   │
│  ┌──────────────────────────────────┐   │
│  │   内存优先级队列                  │   │
│  │   (BinaryHeap)                   │   │
│  └──────────────────────────────────┘   │
│  ┌──────────────────────────────────┐   │
│  │   依赖图 (DAG)                    │   │
│  │   (petgraph)                     │   │
│  └──────────────────────────────────┘   │
│  ┌──────────────────────────────────┐   │
│  │   任务执行器                      │   │
│  │   (Semaphore 并发控制)            │   │
│  └──────────────────────────────────┘   │
│  ┌──────────────────────────────────┐   │
│  │   AetherDB 持久化层               │   │
│  │   (tasks.aedb)                   │   │
│  └──────────────────────────────────┘   │
└─────────────────────────────────────────┘
```

## 数据流

### 任务提交流程

```
用户提交任务
  -> 验证任务
  -> 保存到 AetherDB
  -> 加入依赖图
  -> 加入内存队列
  -> 更新监控指标
```

### 任务执行流程

```
调度器从依赖图获取就绪任务
  -> 从内存队列取出任务
  -> 更新状态为 Running
  -> 提交到执行器
  -> 执行任务
  -> 保存结果到 AetherDB
  -> 更新状态为 Completed
  -> 从依赖图移除
```

### 崩溃恢复流程

```
系统启动
  -> 打开 AetherDB
  -> 扫描 TASK_META kind
  -> 找出 Queued 和 Running 任务
  -> Running 任务标记为 Queued（重试）
  -> 重建内存队列和依赖图
  -> 恢复正常调度
```

## AetherDB 存储结构

任务队列使用以下 kind 命名空间：

- `TASK_META (10)`: 任务元数据
- `TASK_RESULT (11)`: 任务执行结果
- `TASK_LOG (12)`: 任务执行日志
- `QUEUE_STATE (13)`: 队列状态快照
- `DEAD_LETTER (14)`: 死信队列

## 性能指标

- **吞吐量**: 100+ 任务/秒
- **延迟**: 任务从提交到开始执行 < 100ms
- **并发**: 支持 4-8 个任务并发执行
- **恢复时间**: 崩溃后恢复 < 5 秒
- **内存占用**: 1000 个任务 < 50 MB

## 监控指标

系统导出以下 Prometheus 指标：

- `queue_tasks_submitted_total`: 提交的任务总数
- `queue_tasks_completed_total`: 完成的任务总数
- `queue_tasks_failed_total`: 失败的任务总数
- `queue_tasks_cancelled_total`: 取消的任务总数
- `queue_tasks_retried_total`: 重试的任务总数
- `queue_size`: 当前队列大小
- `queue_running_tasks`: 当前运行中任务数
- `queue_available_slots`: 当前可用执行槽位数
- `queue_execution_duration_seconds`: 任务执行时长分布
- `queue_wait_duration_seconds`: 任务队列等待时长分布

## 测试

运行单元测试：

```bash
cargo test -p aether-remote --lib queue
```

运行演示程序：

```bash
cargo run -p aether-remote --example task_queue_demo
```

## 后续优化方向

1. **分布式队列**: 支持多台电脑协同工作
2. **任务优先级动态调整**: 根据等待时间自动提升优先级
3. **资源感知调度**: 根据 CPU/内存使用率动态调整并发数
4. **任务模板**: 预定义常用任务模板，快速创建
5. **Web UI**: 可视化任务队列状态

## 许可证

与 AetherStudio 项目相同
