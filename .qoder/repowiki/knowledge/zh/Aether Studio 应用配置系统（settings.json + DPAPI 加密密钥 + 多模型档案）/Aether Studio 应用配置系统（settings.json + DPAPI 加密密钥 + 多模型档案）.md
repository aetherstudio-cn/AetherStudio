---
kind: configuration_system
name: Aether Studio 应用配置系统（settings.json + DPAPI 加密密钥 + 多模型档案）
category: configuration_system
scope:
    - '**'
source_files:
    - crates/aether-shared/src/settings.rs
    - crates/aether-win32/src/settings.rs
    - crates/aether-ai-panel/src/ai_panel.rs
    - crates/aether-ai-panel/src/embedding.rs
    - crates/aether-cli/src/bin/clean_orphan_history.rs
---

## 1. 总体方案

Aether Studio 使用基于 `serde`/`serde_json` 的纯 Rust 配置系统，核心持久化格式为 JSON，存放于用户配置目录下的 `Aether/settings.json`；敏感信息（各 AI 模型的 API Key）通过 Windows DPAPI 单独加密存储到 `Aether/api_key.enc`。配置加载遵循“默认值 → 磁盘 JSON → 迁移 → 解密密钥注入”的分层策略。

## 2. 关键文件与职责

- `crates/aether-shared/src/settings.rs`：定义全部配置结构体（`AppSettings`、`UiSettings`、`AiSettings`、`AiModelProfile`、`AutoSaveSettings`、`RemoteSettings`、`SshServerConfig`、`UpdateSettings`），实现 `load/save/load_from/save_to` 等 I/O 逻辑、DPAPI 加解密、旧版单一 AI 配置迁移、原子写入。
- `crates/aether-win32/src/settings.rs`：设置面板 UI 状态机（`SettingsPanel`、`ModelConfig`、`SettingsTab` 等），负责把 UI 编辑字段与 `AppSettings` 双向转换、模型列表管理、未保存变更检测、后台测试连接与模型列表拉取。
- `crates/aether-ai-panel/src/*.rs`、`crates/aether-cli/src/bin/clean_orphan_history.rs`、`crates/aether-ai-panel/src/embedding.rs`：其他模块通过 `dirs::config_dir()` 定位 Aether 配置目录，用于 AI 面板缓存、CLI 清理等。
- `.local-config/Aether/`：开发期本地配置目录（仓库内为空，运行时由 `dirs::config_dir()` 指向系统配置目录）。

## 3. 架构与设计约定

### 3.1 配置文件布局
- 位置：`dirs::config_dir().join("Aether")`，不存在时自动创建。
- 主配置：`settings.json`，包含 UI 主题、窗口位置、自动保存策略、SSH 服务器列表、AI 模型列表、更新策略等。
- 密钥文件：`api_key.enc`，仅保存各模型的 API Key（JSON map: `model_id -> key`），不进入 settings.json。
- 损坏备份：解析失败时将原文件重命名为 `settings.json.corrupt` 并回退默认配置。

### 3.2 分层加载顺序
1. 构造 `AppSettings::default()`（内置默认值）
2. 读取 `settings.json` 反序列化为 `AppSettings`
3. 读取并解密 `api_key.enc`，按 model id 注入对应 `AiModelProfile.settings.api_key`
4. 执行旧版迁移：若磁盘存在 `"ai"` 对象且无 `ai_models`，则生成一条 `id="legacy-ai"` 的模型档案并设为激活
5. 返回最终内存结构

### 3.3 多模型架构
- `AppSettings.ai_models: Vec<AiModelProfile>` 承载多个模型配置，每个模型独立 provider/base_url/model/api_key/参数。
- `active_model_id` 指定当前激活模型；`active_ai_settings()` 提供优先级访问：激活模型 → 首个启用模型 → 回退旧 `ai` 字段。
- 设置面板以“模型列表 + 内嵌表单”方式编辑，新建/编辑后通过 `sync_to_app_settings` 写回 `AppSettings`。

### 3.4 安全约定
- `ApiSettings.api_key` 标记 `#[serde(skip_serializing)]`，永不写入 `settings.json`。
- Windows 平台使用 `CryptProtectData` / `CryptUnprotectData`（`CRYPTPROTECT_LOCAL_MACHINE`）对 `api_key.enc` 进行 DPAPI 加解密。
- 非 Windows 平台降级为裸 UTF-8 字节（项目主要面向 Windows）。
- Debug 输出中 `api_key` 显示为 `[REDACTED]`。

### 3.5 原子写入
`save_to` 采用“临时文件 + fsync + rename”模式：
- 写入带进程 ID + 纳秒时间戳后缀的 `.tmp.*` 临时文件
- 调用 `sync_all()` 确保落盘
- 使用 `rename` 原子替换目标文件（Windows 下 `MoveFileEx`，POSIX 下 `rename`）
- 失败时清理临时文件，保证原文件不被破坏

### 3.6 工作区路径隔离持久化
`last_workspace` 字段通过独立的 `persist_last_workspace()` 读写，避免普通 `save()` 被各窗口并发覆写导致“最后打开文件夹”错乱。

## 4. 约束与规则

- **所有配置项必须可序列化**：结构体统一派生 `Serialize`/`Deserialize`，并使用 `#[serde(default)]` 保证向后兼容。
- **新增字段需有默认值**：通过 `Default` 实现或 `#[serde(default = ...)]` 提供，防止旧版本 JSON 无法解析。
- **敏感字段不得明文持久化**：`api_key` 必须走 DPAPI 加密文件；SSH 密码/passphrase 注释明确“不持久化”。
- **未知枚举值需兼容**：`SshAuthType` 使用 `#[serde(other)]` 将未知认证方式映射为 `Fallback`，行为等同 Agent。
- **UI 与持久化分离**：`SettingsPanel` 是纯 UI 状态，通过 `to_profile`/`from_profile`/`to_ai_settings`/`from_settings` 与共享层类型转换，禁止直接修改 `AppSettings` 内部字段。
- **未保存变更检测**：`SettingsPanel` 在打开时记录 `baseline_ai` 快照，通过 `is_dirty()` 比较当前编辑态与基准是否变化。
- **自动更新策略**：`UpdatePolicy` 枚举（`AutoInstall`/`NotifyOnly`/`Disabled`）配合 `suppress_days`、`last_check_ts`、`last_suppressed_ts` 控制检查频率。

## 5. 与其他模块的协作

- `aether-ai`：从 `AiSettings` 构造 `AiConfig`/`AiClient`，消费运行时的模型配置。
- `aether-ai-panel`：通过 `dirs::config_dir()` 定位配置目录，读取/写入 AI 面板缓存。
- `aether-cli`：`clean_orphan_history` 工具同样基于 `dirs::config_dir()` 清理历史数据。
- `aether-render`/`aether-ui`：通过 `AppSettings.ui` 获取主题、字体、窗口位置等 UI 偏好。

## 6. 总结

该配置系统以 `serde` JSON 为核心载体，通过“默认值 → 磁盘 → 迁移 → 密钥注入”的四层加载链实现健壮的配置管理；以 DPAPI 分离敏感密钥、以原子写入保障文件完整性、以多模型档案支持多供应商切换，并通过严格的 `skip_serializing`、`default`、`other` 等 serde 特性保证向后兼容与安全边界。