//! 会话管理器（持久化存储和自动恢复）

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::auth::{AuthSession, SupabaseAuthClient};
use super::error::{QueueError, Result};

/// 会话管理器
pub struct SessionManager {
    auth_client: Arc<SupabaseAuthClient>,
    session_file: PathBuf,
    current_session: Arc<RwLock<Option<AuthSession>>>,
}

impl SessionManager {
    /// 创建新的会话管理器
    pub fn new(auth_client: Arc<SupabaseAuthClient>, session_file: PathBuf) -> Self {
        Self {
            auth_client,
            session_file,
            current_session: Arc::new(RwLock::new(None)),
        }
    }

    /// 登录并保存会话
    pub async fn login(&self, email: &str, password: &str) -> Result<AuthSession> {
        let session = self.auth_client.sign_in(email, password).await?;
        
        // 保存到内存
        {
            let mut sess = self.current_session.write().await;
            *sess = Some(session.clone());
        }
        
        // 保存到文件
        self.save_session(&session).await?;
        
        Ok(session)
    }

    /// 注册并保存会话
    pub async fn signup(&self, email: &str, password: &str, tenant_id: Option<String>) -> Result<AuthSession> {
        let session = self.auth_client.sign_up(email, password, tenant_id).await?;
        
        // 保存到内存
        {
            let mut sess = self.current_session.write().await;
            *sess = Some(session.clone());
        }
        
        // 保存到文件
        self.save_session(&session).await?;
        
        Ok(session)
    }

    /// 登出并清除会话
    pub async fn logout(&self) -> Result<()> {
        self.auth_client.sign_out().await?;
        
        // 清除内存
        {
            let mut sess = self.current_session.write().await;
            *sess = None;
        }
        
        // 删除文件
        if self.session_file.exists() {
            std::fs::remove_file(&self.session_file)?;
        }
        
        Ok(())
    }

    /// 从文件恢复会话
    pub async fn restore_session(&self) -> Result<Option<AuthSession>> {
        if !self.session_file.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&self.session_file)?;
        let session: AuthSession = serde_json::from_str(&content)
            .map_err(|e| QueueError::SerializationError(format!("解析会话文件失败: {}", e)))?;

        // 检查 Token 是否过期
        if let Some(expires_at) = session.expires_at {
            let now = chrono::Utc::now().timestamp();
            if now >= expires_at {
                tracing::info!("会话已过期，尝试刷新");
                
                // 尝试刷新 Token
                match self.auth_client.refresh_token().await {
                    Ok(new_session) => {
                        self.save_session(&new_session).await?;
                        
                        let mut sess = self.current_session.write().await;
                        *sess = Some(new_session.clone());
                        
                        return Ok(Some(new_session));
                    }
                    Err(e) => {
                        tracing::error!("刷新 Token 失败: {}", e);
                        // 删除过期会话
                        let _ = std::fs::remove_file(&self.session_file);
                        return Ok(None);
                    }
                }
            }
        }

        // 恢复到内存
        {
            let mut sess = self.current_session.write().await;
            *sess = Some(session.clone());
        }

        tracing::info!("会话已恢复: {}", session.user.email);
        Ok(Some(session))
    }

    /// 获取当前会话
    pub async fn get_session(&self) -> Option<AuthSession> {
        let sess = self.current_session.read().await;
        sess.clone()
    }

    /// 获取访问 Token
    pub async fn access_token(&self) -> Option<String> {
        let sess = self.current_session.read().await;
        sess.as_ref().map(|s| s.access_token.clone())
    }

    /// 检查是否已登录
    pub async fn is_authenticated(&self) -> bool {
        let sess = self.current_session.read().await;
        sess.is_some()
    }

    /// 保存会话到文件
    async fn save_session(&self, session: &AuthSession) -> Result<()> {
        // 确保目录存在
        if let Some(parent) = self.session_file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(session)?;
        std::fs::write(&self.session_file, content)?;
        
        tracing::debug!("会话已保存到: {}", self.session_file.display());
        Ok(())
    }

    /// 获取会话文件路径
    pub fn session_file_path(&self) -> &Path {
        &self.session_file
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_session_manager() -> (SessionManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let session_file = temp_dir.path().join("session.json");
        
        let config = crate::queue::supabase::SupabaseConfig::default();
        let auth_client = Arc::new(SupabaseAuthClient::new(config));
        let manager = SessionManager::new(auth_client, session_file);
        
        (manager, temp_dir)
    }

    #[tokio::test]
    async fn test_session_manager_creation() {
        let (manager, _temp) = create_test_session_manager().await;
        
        assert!(!manager.is_authenticated().await);
        assert!(manager.get_session().await.is_none());
    }

    #[tokio::test]
    async fn test_restore_nonexistent_session() {
        let (manager, _temp) = create_test_session_manager().await;
        
        let session = manager.restore_session().await.unwrap();
        assert!(session.is_none());
    }
}
