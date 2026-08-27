---
kind: external_dependency
name: 自研嵌入式向量数据库 AetherDB（.aedb）
slug: aether-db
category: external_dependency
category_hints:
    - framework_behavior
    - client_constraint
scope:
    - '**'
---

项目当前唯一的持久化存储是位于 `crates/aether-db` 的自研嵌入式向量数据库，替代了早期使用的 SQLite（rusqlite + sqlite-vec）。核心特征：
- 文件格式为单文件 `.aedb`（固定路径 `%APPDATA%\Aether\conversations\aether_memory.aedb`），4KB 头（magic/version/dim）+ 追加式数据段，段级 CRC32 校验，崩溃安全。
- 向量索引为自研 HNSW，按 kind 分域；≤2048 节点自动退化为暴力精确搜索，适合编辑器几千条消息的数据规模。
- 记录模型 `(kind, key) → {tag, payload, vector?}`，kind 为业务命名空间（会话/消息/playbook 条目等）。
- 垃圾回收通过墓碑删除 + compaction（写 .tmp → sync_all → rename → 重开句柄）实现。
- 唯一外部依赖是 `memmap2`（内存映射），零 C 依赖。

已知约束：无文件锁（编辑器与 CLI 同时打开同一文件会互相踩踏）；embedding 模型维度变更会导致数据库无法打开（需预留迁移策略）；HNSW 墓碑节点不回收、长会话大量删除后内存缓慢膨胀。热数据走 HotDataStore 的 mmap 日志（`conversations/hot/<id>.log`），温数据归档到 AetherDB，冷数据由 ACE Reflector 反思沉淀。