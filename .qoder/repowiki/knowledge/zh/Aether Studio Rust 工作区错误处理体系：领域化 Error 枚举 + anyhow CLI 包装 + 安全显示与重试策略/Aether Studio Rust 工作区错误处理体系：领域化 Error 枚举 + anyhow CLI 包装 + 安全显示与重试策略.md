---
kind: error_handling
name: Aether Studio Rust 工作区错误处理体系：领域化 Error 枚举 + anyhow CLI 包装 + 安全显示与重试策略
category: error_handling
scope:
    - '**'
source_files:
    - crates/aether-ai/src/lib.rs
    - crates/aether-db/src/storage.rs
    - crates/aether-remote/src/git.rs
    - crates/aether-render/src/vscode_theme.rs
    - crates/aether-lsp/src/types.rs
    - crates/aether-cli/src/main.rs
    - crates/aether-win32/src/main.rs
---

## 1. 整体方案

本仓库采用**按 crate 定义领域专用 `Error` 枚举**的模式，核心库（aether-ai、aether-db、aether-render、aether-remote）各自维护独立的错误类型并实现 `std::fmt::Display` 与 `std::error::Error`；CLI 层（aether-cli）使用 `anyhow::Result` 作为统一返回类型并在 `main` 中打印。仓库未引入 `thiserror`/`snaf0`/`eyre` 等第三方错误框架，也未发现全局 panic/recover 中间件。

## 2. 关键文件与错误类型

| 文件 | 错误类型 | 说明 |
|---|---|---|
| `crates/aether-ai/src/lib.rs` | `AiError`（enum: Http/Parse/Config/Api{code,message}） | AI 客户端错误，含 HTTP 状态码、解析失败、配置错误、API 响应错误 |
| `crates/aether-db/src/storage.rs` | `DbError(String)`（新类型包裹） | 存储层 I/O/CRC/mmap 错误，提供 `From<std::io::Error>` |
| `crates/aether-remote/src/git.rs` | `GitError`（enum: CloneFailed/PullFailed/.../GitNotInstalled） | 通过系统 git 二进制执行时的各类失败 |
| `crates/aether-render/src/vscode_theme.rs` | `ThemeError`（enum: Io/Parse/InvalidColor） | VS Code 主题 JSON 解析错误 |
| `crates/aether-lsp/src/types.rs` | `LspError`（struct: code/message/data） | LSP 协议层面的错误消息结构体（非 Rust Result 错误） |
| `crates/aether-cli/src/main.rs` | `anyhow::Result<()>` | CLI 入口统一错误类型，`main` 中 `eprintln!("aether: {:#}", e)` 后 `exit(1)` |

## 3. 架构与约定

### 3.1 领域错误枚举模式
每个业务 crate 定义自己的 `pub enum XxxError`，枚举变体表达语义化的失败原因（如 `AiError::Http`、`AiError::Api{code,message}`、`GitError::CloneFailed`），而非裸字符串。Display 实现输出人类可读的中文提示（如 GitError 的 `克隆失败:`、`系统未安装 git 或不在 PATH 中,请访问 ... 下载安装`）。

### 3.2 安全显示与敏感信息隔离
`AiError` 同时暴露两个展示通道：
- `Display`：包含完整（已截断至 200 字符）的 API 响应体，**仅供日志**，注释标注可能含 API Key 等敏感信息。
- `safe_display()`：返回对用户安全的描述，不含原始响应体，并按 HTTP 状态码映射为中文建议（400→格式错误、401→重新填写 API Key、402→充值、429→稍后重试等）。
- `is_retryable()` / `is_permanent()`：将 429/500/503 标记为可重试，400/401/402/403/404/422 标记为永久错误，配合上层指数退避重试策略。

### 3.3 错误传播路径
- **底层库**（aether-ai/db/render/remote）：函数返回 `Result<T, XxxError>`，调用方用 `?` 向上冒泡。
- **CLI 层**（aether-cli）：`run() -> anyhow::Result<()>`，使用 `bail!`、`Context`、`map_err` 包装底层错误，最终在 `main` 中以 `{:#}` 格式化输出。
- **GUI 主程序**（aether-win32）：`main.rs` 仅做单实例控制与 `run(args)` 调度，不直接处理业务错误；具体 UI 错误由各模块自行处理。

### 3.4 流式错误处理
AI 流式接口（`chat_completion_stream`）不返回 `Result` 中的错误，而是通过 `mpsc::Receiver<AiStreamEvent>` 推送 `Token`/`Reasoning`/`Truncated`/`Done`/`Error` 事件，错误走事件通道而非返回值。

### 3.5 数据完整性错误
`aether-db` 的 `decode_segment` 对损坏/截断段返回 `None`，`Storage::open` 扫描时遇到首个无效段即丢弃尾部数据（崩溃恢复策略），并用 CRC32 校验段完整性。

## 4. 约定与约束

- **禁止裸 `unwrap`/`expect` 在生产代码中**：搜索 `crates/aether-win32/src/**/*.rs` 未发现匹配结果，表明 GUI 主模块避免使用；但在测试代码和少量内部解析处仍有使用（如 `try_into().ok()?` 链）。
- **外部命令失败必须封装为领域错误**：`git.rs` 中 `run()` 子进程失败时构造 `GitError::*Failed(...)` 并附带 stdout/stderr 输出。
- **用户可见错误不得泄露敏感信息**：`AiError::Api` 的消息经 `truncate_error_message` 截断至 200 字符边界，并通过 `safe_display()` 向 UI 展示。
- **HTTP 错误按状态码分类**：`AiError::Api` 携带 `code: u16`，上层据此区分临时错误（429/500/503 自动重试）与永久错误（提示用户检查配置）。
- **CLI 统一错误出口**：`main` 中 `if let Err(e) = run() { eprintln!("aether: {:#}", e); std::process::exit(1); }`，所有 CLI 错误均通过此路径退出。
- **LSP 协议错误与 Rust 错误分离**：`LspError` 是 JSON 序列化结构体（code/message/data），用于 LSP 协议消息，不是 Rust `Result` 的错误类型。

## 5. 缺失与不一致

- 并非所有 crate 都定义了错误类型：`aether-core`、`aether-editor`、`aether-dap`、`aether-lsp` 的核心模块未发现自定义 `Error` 枚举，部分函数直接返回 `std::io::Result` 或 `Option`。
- `git.rs` 中存在 `.to_string()` 将 `GitError` 转为 `String` 再被 `Result` 接受的写法（`return Err(GitError::CloneFailed(...).to_string())`），与其他 crate 的强类型 `Result<T, XxxError>` 风格不一致。
- 未发现统一的错误日志记录中间件或全局 `panic` 钩子；GUI 窗口级崩溃防护见 `crash_guard.rs`（独立于业务错误处理）。
