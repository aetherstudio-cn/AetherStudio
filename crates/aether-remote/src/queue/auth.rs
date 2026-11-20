//! Supabase Auth 客户端

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::error::{QueueError, Result};
use super::supabase::SupabaseConfig;

/// 用户会话
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub expires_at: Option<i64>,
    pub token_type: String,
    pub user: User,
}

/// 用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub role: Option<String>,
    pub user_metadata: Option<serde_json::Value>,
    pub app_metadata: Option<serde_json::Value>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

impl User {
    /// 获取租户 ID
    pub fn tenant_id(&self) -> Option<String> {
        self.user_metadata
            .as_ref()?
            .get("tenant_id")?
            .as_str()
            .map(|s| s.to_string())
    }
}

/// 登录请求
#[derive(Debug, Serialize)]
struct LoginRequest {
    email: String,
    password: String,
}

/// 注册请求
#[derive(Debug, Serialize)]
struct SignupRequest {
    email: String,
    password: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

/// 刷新 Token 请求
#[derive(Debug, Serialize)]
struct RefreshTokenRequest {
    refresh_token: String,
}

/// Supabase Auth 客户端
pub struct SupabaseAuthClient {
    config: SupabaseConfig,
    client: reqwest::Client,
    session: Arc<RwLock<Option<AuthSession>>>,
}

impl SupabaseAuthClient {
    /// 创建新的 Auth 客户端
    pub fn new(config: SupabaseConfig) -> Self {
        let client = reqwest::Client::builder()
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            client,
            session: Arc::new(RwLock::new(None)),
        }
    }

    /// 用户注册
    pub async fn sign_up(
        &self,
        email: &str,
        password: &str,
        tenant_id: Option<String>,
    ) -> Result<AuthSession> {
        let url = format!("{}/auth/v1/signup", self.config.url);

        let mut data = serde_json::json!({});
        if let Some(tid) = tenant_id {
            data["tenant_id"] = serde_json::json!(tid);
        }

        let request = SignupRequest {
            email: email.to_string(),
            password: password.to_string(),
            data: Some(data),
        };

        let response = self
            .client
            .post(&url)
            .header("apikey", &self.config.anon_key)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| QueueError::NetworkError(format!("注册请求失败: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(QueueError::NetworkError(format!(
                "注册失败: {}",
                error_text
            )));
        }

        let session: AuthSession = response
            .json()
            .await
            .map_err(|e| QueueError::SerializationError(format!("解析响应失败: {}", e)))?;

        // 保存会话
        {
            let mut sess = self.session.write().await;
            *sess = Some(session.clone());
        }

        tracing::info!("用户注册成功: {}", email);
        Ok(session)
    }

    /// 用户登录
    pub async fn sign_in(&self, email: &str, password: &str) -> Result<AuthSession> {
        let url = format!("{}/auth/v1/token?grant_type=password", self.config.url);

        let request = LoginRequest {
            email: email.to_string(),
            password: password.to_string(),
        };

        let response = self
            .client
            .post(&url)
            .header("apikey", &self.config.anon_key)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| QueueError::NetworkError(format!("登录请求失败: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(QueueError::NetworkError(format!(
                "登录失败: {}",
                error_text
            )));
        }

        let session: AuthSession = response
            .json()
            .await
            .map_err(|e| QueueError::SerializationError(format!("解析响应失败: {}", e)))?;

        // 保存会话
        {
            let mut sess = self.session.write().await;
            *sess = Some(session.clone());
        }

        tracing::info!("用户登录成功: {}", email);
        Ok(session)
    }

    /// 刷新 Token
    pub async fn refresh_token(&self) -> Result<AuthSession> {
        let refresh_token = {
            let sess = self.session.read().await;
            sess.as_ref()
                .map(|s| s.refresh_token.clone())
                .ok_or_else(|| QueueError::Internal("没有会话".to_string()))?
        };

        let url = format!("{}/auth/v1/token?grant_type=refresh_token", self.config.url);

        let request = RefreshTokenRequest { refresh_token };

        let response = self
            .client
            .post(&url)
            .header("apikey", &self.config.anon_key)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| QueueError::NetworkError(format!("刷新 Token 失败: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(QueueError::NetworkError(format!(
                "刷新 Token 失败: {}",
                error_text
            )));
        }

        let session: AuthSession = response
            .json()
            .await
            .map_err(|e| QueueError::SerializationError(format!("解析响应失败: {}", e)))?;

        // 更新会话
        {
            let mut sess = self.session.write().await;
            *sess = Some(session.clone());
        }

        tracing::info!("Token 刷新成功");
        Ok(session)
    }

    /// 登出
    pub async fn sign_out(&self) -> Result<()> {
        let access_token = {
            let sess = self.session.read().await;
            sess.as_ref()
                .map(|s| s.access_token.clone())
                .ok_or_else(|| QueueError::Internal("没有会话".to_string()))?
        };

        let url = format!("{}/auth/v1/logout", self.config.url);

        let response = self
            .client
            .post(&url)
            .header("apikey", &self.config.anon_key)
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .map_err(|e| QueueError::NetworkError(format!("登出请求失败: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(QueueError::NetworkError(format!(
                "登出失败: {}",
                error_text
            )));
        }

        // 清除会话
        {
            let mut sess = self.session.write().await;
            *sess = None;
        }

        tracing::info!("用户已登出");
        Ok(())
    }

    /// 获取当前会话
    pub async fn get_session(&self) -> Option<AuthSession> {
        let sess = self.session.read().await;
        sess.clone()
    }

    /// 获取访问 Token
    pub async fn access_token(&self) -> Option<String> {
        let sess = self.session.read().await;
        sess.as_ref().map(|s| s.access_token.clone())
    }

    /// 获取当前用户
    pub async fn current_user(&self) -> Option<User> {
        let sess = self.session.read().await;
        sess.as_ref().map(|s| s.user.clone())
    }

    /// 检查是否已登录
    pub async fn is_authenticated(&self) -> bool {
        let sess = self.session.read().await;
        sess.is_some()
    }

    /// 检查 Token 是否过期
    pub async fn is_token_expired(&self) -> bool {
        let sess = self.session.read().await;

        if let Some(session) = sess.as_ref() {
            if let Some(expires_at) = session.expires_at {
                let now = chrono::Utc::now().timestamp();
                return now >= expires_at;
            }
        }

        false
    }

    /// 自动刷新 Token（如果过期）
    pub async fn ensure_valid_token(&self) -> Result<String> {
        if self.is_token_expired().await {
            tracing::info!("Token 已过期，自动刷新");
            self.refresh_token().await?;
        }

        self.access_token()
            .await
            .ok_or_else(|| QueueError::Internal("没有有效的 Token".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_auth_client_creation() {
        let config = SupabaseConfig::default();
        let client = SupabaseAuthClient::new(config);

        assert!(!client.is_authenticated().await);
        assert!(client.get_session().await.is_none());
    }

    #[test]
    fn test_user_tenant_id() {
        let user = User {
            id: "user-1".to_string(),
            email: "test@example.com".to_string(),
            role: Some("user".to_string()),
            user_metadata: Some(serde_json::json!({
                "tenant_id": "tenant-1"
            })),
            app_metadata: None,
            created_at: None,
            updated_at: None,
        };

        assert_eq!(user.tenant_id(), Some("tenant-1".to_string()));
    }
}
