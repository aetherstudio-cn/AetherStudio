# AI面板系统

<cite>
**本文引用的文件**
- [aether-ai-panel/src/lib.rs](file://crates/aether-ai-panel/src/lib.rs)
- [aether-ai-panel/Cargo.toml](file://crates/aether-ai-panel/Cargo.toml)
- [aether-ai-panel/src/ai_panel.rs](file://crates/aether-ai-panel/src/ai_panel.rs)
- [aether-ai-panel/src/ai_agent.rs](file://crates/aether-ai-panel/src/ai_agent.rs)
- [aether-ai-panel/src/ai_context.rs](file://crates/aether-ai-panel/src/ai_context.rs)
- [aether-ai-panel/src/memory_store.rs](file://crates/aether-ai-panel/src/memory_store.rs)
- [aether-ai-panel/src/aether_db_store.rs](file://crates/aether-ai-panel/src/aether_db_store.rs)
- [aether-ai-panel/src/embedding.rs](file://crates/aether-ai-panel/src/embedding.rs)
- [aether-ai-panel/src/ai_warm_data.rs](file://crates/aether-ai-panel/src/ai_warm_data.rs)
- [aether-db/src/lib.rs](file://crates/aether-db/src/lib.rs)
- [aether-db/src/db.rs](file://crates/aether-db/src/db.rs)
- [aether-db/src/storage.rs](file://crates/aether-db/src/storage.rs)
- [aether-db/src/hnsw.rs](file://crates/aether-db/src/hnsw.rs)
- [aether-ai/src/lib.rs](file://crates/aether-ai/src/lib.rs)
- [aether-win32/src/ai_panel.rs](file://crates/aether-win32/src/ai_panel.rs)
- [README.md](file://README.md)
</cite>

## 更新摘要
**所做更改**
- 新增 AetherDB 持久化存储系统章节，详细说明自研嵌入式向量数据库的实现
- 更新记忆存储架构，从 MemoryStore + sqlite-vec 迁移到 AetherDbMemoryStore
- 增强向量检索功能，支持 HNSW 近似最近邻搜索和混合检索
- 更新依赖关系图，反映新的 AetherDB crate 集成
- 完善故障排查指南，包含 AetherDB 相关问题的诊断方法

## 目录
1. [简介](#简介)
2. [项目结构](#项目结构)
3. [核心组件](#核心组件)
4. [架构总览](#架构总览)
5. [详细组件分析](#详细组件分析)
6. [依赖关系分析](#依赖关系分析)
7. [性能考量](#性能考量)
8. [故障排查指南](#故障排查指南)
9. [结论](#结论)
10. [附录](#附录)

## 简介
AI面板系统是 Aether Studio（牧羊人编辑器）中负责"对话式AI助手"的核心模块，提供多会话管理、流式生成、历史记录与持久化、Agent工具协议解析、上下文附件收集、语义检索等能力。它通过 aether-ai 提供的 OpenAI 兼容接口与后端大模型交互，并通过 **全新的 AetherDB 持久化存储系统** 实现本地记忆与向量检索，结合嵌入模型完成语义搜索。UI层由 aether-win32 集成渲染与交互。

**更新** 已集成自研的 AetherDB 嵌入式向量数据库，替代原有的内存存储实现，提供更强大的持久化和语义搜索功能。

## 项目结构
- aether-ai-panel：AI面板业务逻辑与状态管理（会话、消息、流式、历史、Agent、嵌入、存储适配）。
- aether-ai：AI客户端封装（配置、安全校验、HTTP/SSE流式请求、错误处理、Token计数）。
- **aether-db：自研嵌入式向量数据库（纯 Rust，零 C 依赖）**。
- aether-win32：Windows原生UI层，重新导出并集成AI面板类型。
- 其他crate（core/render/shared等）提供编辑器基础能力与共享设置。

```mermaid
graph TB
UI["aether-win32<br/>窗口/事件/渲染"] --> Panel["aether-ai-panel<br/>会话/消息/流式/历史/Agent"]
Panel --> Client["aether-ai<br/>AiClient/安全校验/流式"]
Panel --> Store["AetherDbMemoryStore<br/>AetherDB + HNSW"]
Panel --> Embed["Embedding<br/>ONNX Runtime"]
Panel --> Core["aether-core<br/>工作区/词法/缓冲"]
Panel --> Shared["aether-shared<br/>设置/AiSettings"]
Store --> DB["AetherDB<br/>单文件持久化"]
DB --> Storage["Storage<br/>追加式写入"]
DB --> HNSW["HNSW<br/>向量索引"]
```

**图表来源**
- [aether-win32/src/ai_panel.rs:1-14](file://crates/aether-win32/src/ai_panel.rs#L1-L14)
- [aether-ai-panel/src/lib.rs:1-14](file://crates/aether-ai-panel/src/lib.rs#L1-L14)
- [aether-db/src/lib.rs:1-38](file://crates/aether-db/src/lib.rs#L1-L38)
- [aether-ai-panel/src/aether_db_store.rs:25-44](file://crates/aether-ai-panel/src/aether_db_store.rs#L25-L44)

章节来源
- [README.md:162-179](file://README.md#L162-L179)
- [aether-ai-panel/Cargo.toml:1-22](file://crates/aether-ai-panel/Cargo.toml#L1-L22)

## 核心组件
- AiPanel：活动会话的实时状态、输入、滚动、模式切换、附件、标签页、历史浮窗、Playbook、Agent流水线等。
- AiConversation：单条对话会话（消息、输入、光标、流状态、休眠/唤醒、归档判定）。
- AiStreamState：流式响应共享状态（partial/reasoning/done/error/truncated/start_time）。
- Agent工具协议：行锚定标记解析（文件块、命令、读取/列出目录、规划任务、精准编辑）。
- **AetherDbMemoryStore：基于自研 AetherDB 的记忆存储实现，支持会话/消息/Playbook条目CRUD、语义检索、混合检索、剪枝策略**。
- Embedding：ONNX Runtime嵌入模型（bge-small-zh-v1.5），全局单例懒加载，回退n-gram哈希。
- AiClient：OpenAI兼容聊天/补全/流式，严格HTTPS、私有IP/DNS重绑定防护、错误脱敏与安全展示。

**更新** 新增了 AetherDbMemoryStore 作为主要的存储实现，提供完整的持久化和向量检索能力。

章节来源
- [aether-ai-panel/src/ai_panel.rs:554-719](file://crates/aether-ai-panel/src/ai_panel.rs#L554-L719)
- [aether-ai-panel/src/ai_panel.rs:258-460](file://crates/aether-ai-panel/src/ai_panel.rs#L258-L460)
- [aether-ai-panel/src/ai_agent.rs:1-200](file://crates/aether-ai-panel/src/ai_agent.rs#L1-L200)
- [aether-ai-panel/src/aether_db_store.rs:77-443](file://crates/aether-ai-panel/src/aether_db_store.rs#L77-L443)
- [aether-ai-panel/src/embedding.rs:18-121](file://crates/aether-ai-panel/src/embedding.rs#L18-L121)
- [aether-ai/src/lib.rs:273-334](file://crates/aether-ai/src/lib.rs#L273-L334)

## 架构总览
AI面板采用分层设计：
- UI层（aether-win32）：窗口、事件、渲染，调用AI面板暴露的类型与方法。
- 业务层（aether-ai-panel）：会话管理、消息组织、流式聚合、历史持久化、Agent工具执行、嵌入检索。
- 服务层（aether-ai）：网络请求、SSE流式、安全校验、错误处理。
- **数据层（AetherDB + ONNX）**：自研嵌入式向量数据库，支持崩溃恢复、自动压缩、HNSW向量索引。

```mermaid
sequenceDiagram
participant UI as "UI(aether-win32)"
participant Panel as "AiPanel"
participant WarmData as "WarmDataStore"
participant Store as "AetherDbMemoryStore"
participant DB as "AetherDB"
participant Client as "AiClient"
UI->>Panel : 发送消息/选择上下文
Panel->>Client : chat_completion_stream(messages)
Client-->>Panel : SSE Token/Reasoning/Done/Error
Panel->>WarmData : 归档会话(异步)
WarmData->>Store : append_message(持久化)
Store->>DB : put(kind, key, payload, vector)
DB->>DB : HNSW索引更新
DB-->>Store : 成功/失败
Store-->>WarmData : 持久化结果
WarmData-->>Panel : 归档完成
Panel-->>UI : 更新视图/滚动/折叠思考块
```

**图表来源**
- [aether-ai/src/lib.rs:726-800](file://crates/aether-ai/src/lib.rs#L726-L800)
- [aether-ai-panel/src/ai_panel.rs:721-800](file://crates/aether-ai-panel/src/ai_panel.rs#L721-L800)
- [aether-ai-panel/src/ai_warm_data.rs:72-108](file://crates/aether-ai-panel/src/ai_warm_data.rs#L72-L108)
- [aether-ai-panel/src/aether_db_store.rs:132-151](file://crates/aether-ai-panel/src/aether_db_store.rs#L132-L151)
- [aether-db/src/db.rs:146-178](file://crates/aether-db/src/db.rs#L146-L178)

## 详细组件分析

### 会话与消息模型
- AiMessage：角色（用户/助手/系统/工具/待确认）、内容、深度思考内容与计时、折叠状态。
- AiConversation：会话元信息、消息列表、输入、光标、流状态、是否休眠、归档价值判断。
- AiStreamState：流式增量、结束标志、错误、截断原因、开始时间、首响应标记。

```mermaid
classDiagram
class AiMessage {
+role
+content
+reasoning
+reasoning_collapsed
+reasoning_ms
+reasoning_started_ms
+new(role, content)
+start_reasoning_timer()
+stop_reasoning_timer()
}
class AiConversation {
+id
+title
+created_at
+updated_at
+messages
+input
+caret_pos
+composition
+is_generating
+scroll_y
+content_height
+stick_to_bottom
+mode
+attachments
+stream_state
+should_stop
+used_bullet_ids
+hibernated
+hibernate_pending_at
+hibernated_msg_count
+new(id, title)
+is_archivable() bool
+drain_background() DrainEdge
}
class AiStreamState {
+partial
+reasoning
+done
+error
+truncated
+start_time
+received_first_response
}
AiConversation --> AiMessage : "包含"
AiConversation --> AiStreamState : "共享"
```

**图表来源**
- [aether-ai-panel/src/ai_panel.rs:102-168](file://crates/aether-ai-panel/src/ai_panel.rs#L102-L168)
- [aether-ai-panel/src/ai_panel.rs:170-201](file://crates/aether-ai-panel/src/ai_panel.rs#L170-L201)
- [aether-ai-panel/src/ai_panel.rs:258-460](file://crates/aether-ai-panel/src/ai_panel.rs#L258-L460)

章节来源
- [aether-ai-panel/src/ai_panel.rs:102-201](file://crates/aether-ai-panel/src/ai_panel.rs#L102-L201)
- [aether-ai-panel/src/ai_panel.rs:258-460](file://crates/aether-ai-panel/src/ai_panel.rs#L258-L460)

### 流式生成与后台轮询
- spawn_ai_stream：后台线程发起流式请求，将Token/Reasoning/Done/Truncated/Error写入共享状态。
- drain_background：非活动会话的后台轮询，将增量追加到消息，处理思考计时、错误中断、截断提示。

```mermaid
flowchart TD
Start(["后台流式开始"]) --> Init["初始化stream_state<br/>记录start_time"]
Init --> Loop{"接收事件"}
Loop --> |Token| AppendPartial["追加partial"]
Loop --> |Reasoning| AppendReasoning["追加reasoning<br/>启动思考计时"]
Loop --> |Done| MarkDone["标记done"]
Loop --> |Error| HandleErr["记录错误并标记done"]
Loop --> |Truncated| MarkTrunc["记录截断原因"]
AppendPartial --> UpdateMsg["追加到最后助手消息"]
AppendReasoning --> UpdateMsg
UpdateMsg --> Next["继续循环"]
MarkDone --> Next
HandleErr --> Next
MarkTrunc --> Next
Next --> End(["结束或继续"])
```

**图表来源**
- [aether-ai/src/lib.rs:726-800](file://crates/aether-ai/src/lib.rs#L726-L800)
- [aether-ai-panel/src/ai_panel.rs:721-800](file://crates/aether-ai-panel/src/ai_panel.rs#L721-L800)
- [aether-ai-panel/src/ai_panel.rs:364-460](file://crates/aether-ai-panel/src/ai_panel.rs#L364-L460)

章节来源
- [aether-ai/src/lib.rs:726-800](file://crates/aether-ai/src/lib.rs#L726-L800)
- [aether-ai-panel/src/ai_panel.rs:721-800](file://crates/aether-ai-panel/src/ai_panel.rs#L721-L800)
- [aether-ai-panel/src/ai_panel.rs:364-460](file://crates/aether-ai-panel/src/ai_panel.rs#L364-L460)

### Agent工具协议与解析
- 行锚定标记：文件块（AETHER_FILE/SEP/END_FILE）、运行命令（AETHER_RUN/END_RUN）、读取/列出（AETHER_READ/LIST）、规划（AETHER_PLAN/END_PLAN）、精准定位/编辑（AETHER_LOCATE/EDIT/END_EDIT）。
- parse_edits：从AI回复中解析出AiEdit列表（路径、search/replace、创建/删除判断）。
- has_agent_markers：快速检测是否包含工具标记，用于前置校验。

```mermaid
flowchart TD
Input["AI回复文本"] --> Scan["逐行扫描"]
Scan --> FoundHeader{"发现文件头标记?"}
FoundHeader --> |是| CollectSearch["收集search段至分隔行"]
CollectSearch --> FoundSep{"找到分隔行?"}
FoundSep --> |是| CollectReplace["收集replace段至结束标记"]
CollectReplace --> EmitEdit["输出AiEdit(path, search, replace)"]
FoundHeader --> |否| NextLine["下一行"]
EmitEdit --> NextLine
NextLine --> End["结束"]
```

**图表来源**
- [aether-ai-panel/src/ai_agent.rs:1-200](file://crates/aether-ai-panel/src/ai_agent.rs#L1-L200)

章节来源
- [aether-ai-panel/src/ai_agent.rs:1-200](file://crates/aether-ai-panel/src/ai_agent.rs#L1-L200)

### 上下文附件与提示构建
- AiContextAttachment：当前文件、选区、打开文件、诊断、文件树、自定义文本。
- wrap_code_block：将代码片段包装为带路径/语言标记的代码块。
- truncate_middle：限制长度并保留首尾，避免超长上下文。

章节来源
- [aether-ai-panel/src/ai_context.rs:1-117](file://crates/aether-ai-panel/src/ai_context.rs#L1-L117)

### AetherDB 持久化存储系统
**新增** AetherDB 是一个纯 Rust 实现的嵌入式向量数据库，具有以下特性：

- **单文件持久化**：4KB Header + 追加式数据段，段级 CRC32 校验，崩溃安全
- **HNSW 向量索引**：内存常驻，打开时全量重建；小规模自动退化暴力精确搜索
- **标量过滤**：kind 命名空间 + tag 二级索引（会话ID/分类等精确匹配）
- **向量相似度与标量条件联合检索**：knn + tag 过滤
- **空间回收**：删除/覆盖产生垃圾段，达阈值后后台 compaction（重写+原子替换）

```mermaid
classDiagram
class AetherDbMemoryStore {
+db : Mutex~AetherDb~
+embedding_dim : usize
+path : PathBuf
+open(dir, embedding_dim) Result
+path() Path
+is_empty() bool
+check_dim(embedding) Result
}
class AetherDb {
+storage : Storage
+dim : usize
+records : HashMap~(u8, String), Record~
+tag_index : HashMap~(u8, String), HashSet~String~~
+hnsw : HashMap~u8, Hnsw~
+put(kind, key, tag, payload, vector) Result
+delete(kind, key) Result
+scan(kind) Vec~Record~
+scan_by_tag(kind, tag) Vec~Record~
+knn(kind, query, k, tag_filter) Vec~KnnHit~
+flush() Result
+force_compact() Result
}
class MemoryStore {
<<interface>>
+upsert_conversation(conv) Result
+append_message(msg) Result
+search_messages(query_embedding, conv_id, k) Result
+hybrid_search_messages(text, embedding, conv_id, k) Result
+prune_bullets(config) Result
}
AetherDbMemoryStore --> MemoryStore : "实现"
AetherDbMemoryStore --> AetherDb : "使用"
```

**图表来源**
- [aether-ai-panel/src/aether_db_store.rs:25-66](file://crates/aether-ai-panel/src/aether_db_store.rs#L25-L66)
- [aether-db/src/db.rs:29-45](file://crates/aether-db/src/db.rs#L29-L45)
- [aether-ai-panel/src/memory_store.rs:77-201](file://crates/aether-ai-panel/src/memory_store.rs#L77-L201)

#### AetherDB 核心数据结构
- **Record**：一条存活记录，包含 key、tag、payload、vector
- **KnnHit**：KNN 命中结果，包含 key 和 distance
- **Segment**：原始段，包含 offset、size、kind、tombstone、tag、key、payload、vector

#### 存储层特性
- **追加式写入**：所有操作都是追加写，删除通过墓碑段标记
- **崩溃恢复**：启动时全量扫描重建内存索引，损坏段直接截断
- **自动压缩**：垃圾超过 1MB 且占比超 1/3 时触发 compaction
- **CRC32 校验**：每个段都有 CRC32 校验，确保数据完整性

章节来源
- [aether-ai-panel/src/aether_db_store.rs:1-675](file://crates/aether-ai-panel/src/aether_db_store.rs#L1-L675)
- [aether-db/src/db.rs:1-466](file://crates/aether-db/src/db.rs#L1-L466)
- [aether-db/src/storage.rs:1-404](file://crates/aether-db/src/storage.rs#L1-L404)

### 嵌入模型与向量检索
- **EmbeddingModel**：基于ONNX Runtime的文本编码，支持pooler_output或last_hidden_state均值池化，L2归一化。
- **全局单例**：init_embedding_model、try_init_default_model、embed_text（未初始化时回退n-gram哈希）。
- **默认模型目录**：%CONFIG%/Aether/models/bge-small-zh-v1.5。
- **HNSW 索引**：余弦距离度量，M=8、Mmax0=16、ef_construction=64，小规模自动退化暴力搜索。

**更新** 现在支持两种检索方式：纯向量检索和混合检索（关键词 + 向量 RRF 融合）。

章节来源
- [aether-ai-panel/src/embedding.rs:18-121](file://crates/aether-ai-panel/src/embedding.rs#L18-L121)
- [aether-ai-panel/src/embedding.rs:127-200](file://crates/aether-ai-panel/src/embedding.rs#L127-L200)
- [aether-db/src/hnsw.rs:1-337](file://crates/aether-db/src/hnsw.rs#L1-L337)

### AI客户端与安全
- AiConfig：提供商、API Key、Base URL、模型、采样参数、思考模式、频率/存在惩罚、停止序列、响应格式、用量统计、logprobs、用户标识。
- AiClient：连接超时/读空闲超时、禁用自动重定向、HTTPS强制、私有IP/DNS重绑定防护、云元数据黑名单、错误脱敏与安全展示。
- 流式接口：chat_completion_stream返回Receiver，事件包括Token/Reasoning/Done/Truncated/Error。

章节来源
- [aether-ai/src/lib.rs:167-271](file://crates/aether-ai/src/lib.rs#L167-L271)
- [aether-ai/src/lib.rs:273-334](file://crates/aether-ai/src/lib.rs#L273-L334)
- [aether-ai/src/lib.rs:408-548](file://crates/aether-ai/src/lib.rs#L408-L548)
- [aether-ai/src/lib.rs:726-800](file://crates/aether-ai/src/lib.rs#L726-L800)

## 依赖关系分析
- aether-ai-panel 依赖 aether-ai（流式请求）、aether-shared（设置）、aether-core（工作区/词法）、**aether-db（持久化存储）**。
- aether-win32 重新导出 aether-ai-panel 的类型以兼容上层调用。
- 存储层使用 **AetherDB（纯 Rust，零 C 依赖）**；嵌入层使用 ort + tokenizers。

```mermaid
graph LR
Win32["aether-win32"] --> Panel["aether-ai-panel"]
Panel --> AI["aether-ai"]
Panel --> Shared["aether-shared"]
Panel --> Core["aether-core"]
Panel --> AetherDB["aether-db"]
AetherDB --> Storage["Storage<br/>追加式写入"]
AetherDB --> HNSW["HNSW<br/>向量索引"]
Panel --> ORT["ort/tokenizers"]
```

**图表来源**
- [aether-win32/src/ai_panel.rs:1-14](file://crates/aether-win32/src/ai_panel.rs#L1-L14)
- [aether-ai-panel/Cargo.toml:6-22](file://crates/aether-ai-panel/Cargo.toml#L6-L22)
- [aether-db/src/lib.rs:17-38](file://crates/aether-db/src/lib.rs#L17-L38)

章节来源
- [aether-ai-panel/Cargo.toml:6-22](file://crates/aether-ai-panel/Cargo.toml#L6-L22)

## 性能考量
- 流式生成：后台线程+共享状态，UI轮询增量，避免阻塞主线程。
- 会话休眠：非活动会话卸载消息体至温数据层，降低内存占用。
- **向量检索：AetherDB HNSW 索引，默认维度512，支持混合检索（子串关键词 + 向量 RRF 融合）**。
- 网络优化：连接超时15s、读空闲300s，禁用自动重定向，限制响应体大小10MB。
- 错误脱敏：safe_display与sanitize_error防止敏感信息泄露。
- **存储优化：追加式写入、自动压缩、CRC32校验、崩溃恢复**。

**更新** 新增 AetherDB 的性能优势：纯 Rust 实现零外部依赖、单文件持久化、自动空间回收、HNSW 高效向量检索。

## 故障排查指南
- 网络连接失败：检查Base URL是否为HTTPS、API Key是否设置、DNS解析是否被拦截。
- API返回错误：查看safe_display描述，区分4xx/5xx并给出重试建议。
- 流式无响应：确认received_first_response与start_time，检查should_stop是否被设置。
- 嵌入模型未加载：检查默认模型目录是否存在model.onnx与tokenizer.json，否则回退n-gram。
- **历史为空：确认 AetherDB 文件是否存在，检查 append_message 是否成功，验证 workspace_hash 绑定**。
- **向量检索失败：检查向量维度是否匹配，确认 HNSW 索引是否正确构建，验证 tag 过滤条件**。
- **存储损坏：检查 AetherDB 文件头部 magic 是否匹配，查看 CRC32 校验是否通过，尝试 force_compact 修复**。

**更新** 新增 AetherDB 相关的故障排查方法，包括文件损坏检测、索引重建、压缩修复等。

章节来源
- [aether-ai/src/lib.rs:112-165](file://crates/aether-ai/src/lib.rs#L112-L165)
- [aether-ai/src/lib.rs:336-347](file://crates/aether-ai/src/lib.rs#L336-L347)
- [aether-ai-panel/src/ai_panel.rs:25-100](file://crates/aether-ai-panel/src/ai_panel.rs#L25-L100)
- [aether-ai-panel/src/embedding.rs:154-185](file://crates/aether-ai-panel/src/embedding.rs#L154-L185)
- [aether-db/src/storage.rs:208-246](file://crates/aether-db/src/storage.rs#L208-L246)

## 结论
AI面板系统通过清晰的分层与模块化设计，实现了高效的对话式AI助手功能。**新增的 AetherDB 持久化存储系统** 提供了强大的本地记忆、向量语义搜索和崩溃恢复能力。其流式生成、会话休眠、Agent工具协议、语义检索与安全校验共同构成了一个健壮且可扩展的AI集成方案。未来可进一步扩展云端同步、更多提供商支持与更精细的权限控制。

**更新** AetherDB 的引入显著提升了系统的可靠性和功能性，为大规模对话历史和智能检索提供了坚实基础。

## 附录
- 构建与运行：参考README中的构建命令与运行方式。
- 测试：单元测试、覆盖率、GUI冒烟测试脚本。
- 贡献：遵循CONTRIBUTING.md的流程与规范。
- **AetherDB 文档：了解自研向量数据库的详细设计和API**。

**更新** 新增 AetherDB 相关文档链接和使用指南。

章节来源
- [README.md:85-118](file://README.md#L85-L118)
- [README.md:127-159](file://README.md#L127-L159)
- [README.md:258-298](file://README.md#L258-L298)
- [README.md:300-333](file://README.md#L300-L333)