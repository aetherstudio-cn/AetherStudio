---
kind: configuration_system
name: Aether Studio 用户配置系统（settings.json + DPAPI 密钥存储）
category: configuration_system
scope:
    - '**'
source_files:
    - crates/aether-shared/src/settings.rs
    - crates/aether-shared/src/lib.rs
    - crates/aether-win32/src/window/window_setup.rs
    - crates/aether-win32/src/editor/files.rs
    - crates/aether-ui/src/recent_projects.rs
    - crates/aether-ai-panel/src/ai_panel.rs
    - crates/aether-ai-panel/src/embedding.rs
    - crates/aether-cli/src/bin/clean_orphan_history.rs
---

## 1. 整体方案

Aether Studio 的用户配置采用 **JSON 文件 + 平台原生加密** 的混合方案，核心由 `crates/aether-shared/src/settings.rs` 中的 `AppSettings` 结构体统一管理。配置文件位于操作系统标准配置目录：通过 `dirs::config_dir()` 解析（Windows 下为 `%APPDATA%`），子目录固定为 `Aether/`，主配置文件为 `settings.json`，敏感信息（各 AI 模型的 API Key）单独以 Windows DPAPI 加密后写入 `api_key.enc`。

除 `AppSettings` 外，项目还维护了独立的轻量级持久化数据：
- 最近项目列表：`%APPDATA%/Aether/recent_projects.json`（`crates/aether-ui/src/recent_projects.rs` 中 `RecentProjectsManager` 自行实现 JSON 读写）
- 其他模块（AI 面板、embedding、CLI 清理脚本等）也直接使用 `dirs::config_dir().join("Aether")` 作为共享数据根目录。

## 2. 关键文件与包

- `crates/aether-shared/src/settings.rs`：定义所有配置结构体（`AppSettings`、`AiSettings`、`UiSettings`、`RemoteSettings`、`AutoSaveSettings`、`UpdateSettings`、`SshServerConfig`、`AiModelProfile`、`EditorMode`、`UpdatePolicy`）、序列化/反序列化、加载/保存、DPAPI 加解密、旧版单一 AI 配置迁移逻辑。
- `crates/aether-shared/src/lib.rs`：仅暴露 `launch` 和 `settings` 两个模块。
- `crates/aether-win32/src/window/window_setup.rs`：应用启动时调用 `AppSettings::load()` 并负责后续 `save()`。
- `crates/aether-win32/src/editor/files.rs`：通过 `AppSettings::persist_last_workspace()` 独立更新“最后打开的工作区”，避免与其他窗口内存副本冲突。
- `crates/aether-ui/src/recent_projects.rs`：独立管理最近项目列表。
- `crates/aether-ai-panel/src/*.rs`、`crates/aether-cli/src/bin/clean_orphan_history.rs`：使用同一 `Aether/` 目录存放对话日志、热点数据等运行时数据。

## 3. 架构与设计约定

### 3.1 配置结构分层
`AppSettings` 是顶层聚合结构，按领域拆分子结构：
- `ui`：主题、字体、侧边栏可见性、活动栏/菜单栏顺序、窗口位置尺寸、编辑器模式（`Developer` / `Agent`）、AI 标签页快照等。
- `remote`：SSH 服务器列表（密码不持久化，连接时输入）。
- `auto_save`：自动保存策略（防抖空闲 + 失焦 + 周期兜底）。
- `ai_models`：**多模型档案列表**，每个 `AiModelProfile` 内嵌一份 `AiSettings`（flatten 保持 JSON 形状不变）。
- `active_model_id`：当前激活模型 ID；`active_ai_settings()` 按优先级解析（激活 → 首个启用 → 回退旧 `ai` 字段）。
- `update`：自动更新策略（`AutoInstall` / `NotifyOnly` / `Disabled`）及抑制计时。

### 3.2 敏感信息隔离
- `AiSettings.api_key` 标注 `#[serde(skip_serializing)]`，**永不写入 `settings.json`**。
- 保存时将所有非空 `api_key` 收集为 `BTreeMap<model_id, key>`，用 Windows DPAPI (`CryptProtectData`) 加密后写入 `api_key.enc`；清空时删除该文件。
- 加载时先读 `api_key.enc`，尝试解密，支持新格式（JSON map）和旧格式（单个密钥字符串或裸文本），再注入到对应模型档案。
- `Debug` 实现中对 `api_key` 输出 `[REDACTED]`。

### 3.3 原子写入与损坏恢复
- `save_to()` 采用 **临时文件 + fsync + rename** 的原子写入模式（Windows 下 `share_mode(0)` 独占写，非 Windows 下设置 `0o600` 权限）。
- 解析失败时记录警告并将原文件重命名为 `.json.corrupt` 备份，然后回退到默认设置。
- `last_workspace` 字段通过独立的 `persist_last_workspace()` 方法读写，避免多个窗口内存副本互相覆盖。

### 3.4 向后兼容与迁移
- 旧版单一 `ai` 对象在首次加载时检测是否存在，若存在且 `ai_models` 为空，则自动迁移为一条 `id="legacy-ai"` 的模型档案，并设置 `active_model_id`。
- SSH `auth_type` 支持未知值降级为 `Fallback`（语义等同 Agent），保证旧配置可被读取。
- 迁移完成后再次保存不再写出旧的 `ai` 字段。

### 3.5 配置目录约定
- 统一通过 `dirs::config_dir()` 获取（Windows = `%APPDATA%`），子目录 `Aether/`。
- 当 `dirs::config_dir()` 不可用时回退到 `std::env::temp_dir()`。
- 部分 UI 代码（`dialogs.rs`）直接操作 `APPDATA` 环境变量进行临时目录切换，用于测试场景下的隔离。

## 4. 约定与约束

- **所有用户可编辑的配置必须通过 `AppSettings` 的 `save()` / `save_to()` 写入**，禁止直接 `fs::write(settings_path, ...)`，以保证原子性和密钥同步。
- **API Key 不得出现在 `settings.json` 的任何序列化输出中**（由 `skip_serializing` 和单元测试共同保证）。
- **新增配置字段必须提供 `Default` 实现并通过 `#[serde(default)]` 注解**，确保旧版本生成的配置文件仍可被新版本正常加载。
- **SSH 密码/passphrase 不持久化**，仅在连接时由用户输入。
- **工作区路径变更必须走 `persist_last_workspace()`**，不能直接修改内存中的 `ui.last_workspace` 后调用通用 `save()`，以避免多窗口并发覆盖。
- 运行期数据（对话日志、热点缓存、embedding 向量等）统一放在 `dirs::config_dir().join("Aether")` 下，与配置同域但不同文件。
- 自动保存策略遵循“组合触发”原则：防抖空闲 + 失焦立即保存 + 周期兜底，并对大文件智能降级（更长防抖、关闭周期保存）。