//! 真实的命令执行器

use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::error::{QueueError, Result};
use super::models::{Task, TaskLog, TaskResult, TaskStatus};
use super::persistence::AetherTaskStore;
use super::supabase::SupabaseRestClient;

/// 命令执行器
pub struct CommandExecutor {
    store: Arc<AetherTaskStore>,
    rest_client: Option<Arc<SupabaseRestClient>>,
}

impl CommandExecutor {
    /// 创建新的命令执行器
    pub fn new(store: Arc<AetherTaskStore>) -> Self {
        Self {
            store,
            rest_client: None,
        }
    }

    /// 设置 Supabase REST 客户端（用于实时同步）
    pub fn with_rest_client(mut self, client: Arc<SupabaseRestClient>) -> Self {
        self.rest_client = Some(client);
        self
    }

    /// 执行任务
    pub async fn execute(&self, task: Task) -> Result<TaskResult> {
        let start_time = chrono::Utc::now();
        let start_instant = std::time::Instant::now();

        tracing::info!("开始执行任务: {} ({})", task.title, task.id);

        // 记录开始日志
        let _ = self.store.append_log(&TaskLog::info(
            task.id,
            format!("开始执行: {}", task.command),
        ));

        // 同步状态到 Supabase
        if let Some(client) = &self.rest_client {
            let _ = client
                .update_task_status(
                    &task.id.to_string(),
                    "running",
                    Some(0),
                    Some(vec![format!("> 开始执行: {}", task.command)]),
                )
                .await;
        }

        // 执行命令
        let result = self.execute_command(&task).await;

        let completed_at = chrono::Utc::now();
        let duration = start_instant.elapsed();

        match result {
            Ok((exit_code, stdout, stderr)) => {
                let status = if exit_code == 0 {
                    TaskStatus::Completed
                } else {
                    TaskStatus::Failed
                };

                // 记录完成日志
                let _ = self.store.append_log(&TaskLog::info(
                    task.id,
                    format!("执行完成，退出码: {}", exit_code),
                ));

                // 同步到 Supabase
                if let Some(client) = &self.rest_client {
                    let logs = vec![format!("> 执行完成"), format!("> 退出码: {}", exit_code)];

                    let _ = client
                        .update_task_status(
                            &task.id.to_string(),
                            if exit_code == 0 { "done" } else { "failed" },
                            Some(100),
                            Some(logs),
                        )
                        .await;
                }

                Ok(TaskResult {
                    task_id: task.id,
                    status,
                    exit_code: Some(exit_code),
                    stdout,
                    stderr,
                    error: None,
                    started_at: start_time,
                    completed_at,
                    duration_ms: duration.as_millis() as u64,
                })
            }
            Err(e) => {
                // 记录错误日志
                let _ = self
                    .store
                    .append_log(&TaskLog::error(task.id, format!("执行失败: {}", e)));

                // 同步到 Supabase
                if let Some(client) = &self.rest_client {
                    let _ = client
                        .update_task_status(
                            &task.id.to_string(),
                            "failed",
                            Some(0),
                            Some(vec![format!("> 执行失败: {}", e)]),
                        )
                        .await;
                }

                Ok(TaskResult {
                    task_id: task.id,
                    status: TaskStatus::Failed,
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    error: Some(e.to_string()),
                    started_at: start_time,
                    completed_at,
                    duration_ms: duration.as_millis() as u64,
                })
            }
        }
    }

    /// 执行命令（内部方法）
    async fn execute_command(&self, task: &Task) -> Result<(i32, String, String)> {
        let stdout_lines: Vec<String> = Vec::new();
        let stderr_lines: Vec<String> = Vec::new();

        // 解析命令
        let (program, args) = Self::parse_command(&task.command)?;

        tracing::debug!("执行命令: {} {:?}", program, args);

        // 创建子进程
        let mut child = Command::new(program)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| QueueError::ExecutionError(format!("启动进程失败: {}", e)))?;

        // 读取 stdout
        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            let task_id = task.id;
            let store = self.store.clone();
            let rest_client = self.rest_client.clone();

            tokio::spawn(async move {
                let mut line_count = 0;
                while let Ok(Some(line)) = lines.next_line().await {
                    line_count += 1;
                    tracing::debug!("stdout: {}", line);

                    // 记录日志
                    let _ = store.append_log(&TaskLog::info(task_id, line.clone()));

                    // 每 10 行同步一次到 Supabase
                    if line_count % 10 == 0 {
                        if let Some(client) = &rest_client {
                            let progress = (line_count / 10).min(95) as i32;
                            let _ = client
                                .update_task_status(
                                    &task_id.to_string(),
                                    "running",
                                    Some(progress),
                                    None,
                                )
                                .await;
                        }
                    }
                }
            });
        }

        // 读取 stderr
        if let Some(stderr) = child.stderr.take() {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();

            let task_id = task.id;
            let store = self.store.clone();

            tokio::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::warn!("stderr: {}", line);
                    let _ = store.append_log(&TaskLog::error(task_id, line.clone()));
                }
            });
        }

        // 等待进程完成（带超时）
        let timeout = tokio::time::Duration::from_secs(task.timeout_seconds);
        let status = match tokio::time::timeout(timeout, child.wait()).await {
            Ok(Ok(status)) => status,
            Ok(Err(e)) => {
                return Err(QueueError::ExecutionError(format!("等待进程失败: {}", e)));
            }
            Err(_) => {
                // 超时，杀死进程
                let _ = child.kill().await;
                return Err(QueueError::Timeout(format!(
                    "任务执行超时（{} 秒）",
                    task.timeout_seconds
                )));
            }
        };

        let exit_code = status.code().unwrap_or(-1);

        Ok((exit_code, stdout_lines.join("\n"), stderr_lines.join("\n")))
    }

    /// 解析命令字符串
    fn parse_command(command: &str) -> Result<(String, Vec<String>)> {
        let parts: Vec<String> = shell_words::split(command)
            .map_err(|e| QueueError::InvalidTask(format!("解析命令失败: {}", e)))?;

        if parts.is_empty() {
            return Err(QueueError::InvalidTask("命令为空".to_string()));
        }

        let program = parts[0].clone();
        let args = parts[1..].to_vec();

        Ok((program, args))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use uuid::Uuid;

    async fn create_test_executor() -> (CommandExecutor, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_executor.aedb");
        let store = Arc::new(AetherTaskStore::open(&db_path).unwrap());
        let executor = CommandExecutor::new(store);
        (executor, temp_dir)
    }

    #[tokio::test]
    async fn test_parse_command() {
        let (program, args) = CommandExecutor::parse_command("echo hello world").unwrap();
        assert_eq!(program, "echo");
        assert_eq!(args, vec!["hello", "world"]);
    }

    #[tokio::test]
    async fn test_parse_command_with_quotes() {
        let (program, args) = CommandExecutor::parse_command("echo \"hello world\"").unwrap();
        assert_eq!(program, "echo");
        assert_eq!(args, vec!["hello world"]);
    }

    #[tokio::test]
    async fn test_parse_empty_command() {
        let result = CommandExecutor::parse_command("");
        assert!(result.is_err());
    }

    #[tokio::test]
    #[cfg(target_os = "windows")]
    async fn test_execute_simple_command() {
        let (executor, _temp) = create_test_executor().await;

        let mut task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Test Echo".to_string(),
            "cmd /c echo Hello".to_string(),
        );
        task.timeout_seconds = 10;

        let result = executor.execute(task).await.unwrap();
        assert_eq!(result.status, TaskStatus::Completed);
        assert_eq!(result.exit_code, Some(0));
    }

    #[tokio::test]
    #[cfg(not(target_os = "windows"))]
    async fn test_execute_simple_command() {
        let (executor, _temp) = create_test_executor().await;

        let mut task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Test Echo".to_string(),
            "echo Hello".to_string(),
        );
        task.timeout_seconds = 10;

        let result = executor.execute(task).await.unwrap();
        assert_eq!(result.status, TaskStatus::Completed);
        assert_eq!(result.exit_code, Some(0));
    }

    #[tokio::test]
    async fn test_execute_failing_command() {
        let (executor, _temp) = create_test_executor().await;

        let mut task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Test Fail".to_string(),
            "nonexistent_command_12345".to_string(),
        );
        task.timeout_seconds = 5;

        let result = executor.execute(task).await.unwrap();
        assert_eq!(result.status, TaskStatus::Failed);
    }
}
