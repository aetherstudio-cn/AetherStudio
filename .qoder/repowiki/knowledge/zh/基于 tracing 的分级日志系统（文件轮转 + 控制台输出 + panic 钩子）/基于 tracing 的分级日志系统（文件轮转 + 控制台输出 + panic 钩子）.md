---
kind: logging_system
name: 基于 tracing 的分级日志系统（文件轮转 + 控制台输出 + panic 钩子）
category: logging_system
scope:
    - '**'
source_files:
    - crates/aether-ui/src/logging.rs
    - crates/aether-win32/src/logging.rs
    - crates/aether-win32/src/window.rs
    - crates/aether-terminal/src/conpty.rs
    - crates/aether-terminal/src/terminal.rs
    - crates/aether-lsp/src/server.rs
    - crates/aether-ai-panel/src/ai_warm_data.rs
---

## 1. 使用的框架与工具

- **核心库**：`tracing` 0.1（各 crate 通过 `tracing = "0.1"` 引入，作为统一的日志 API）。
- **订阅器/格式化层**：`tracing-subscriber` 0.3，启用 `fmt`、`env-filter`、`time` 特性，提供结构化日志输出、环境变量过滤和本地时区时间戳。
- **文件输出**：`tracing-appender` 0.2 的 `RollingFileAppender`，按天（`Rotation::DAILY`）轮转写入日志文件。
- **时间格式**：`time` crate 的 `UtcOffset` + `format_description`，使用本地时区，格式为 `[year]-[month]-[day] [hour]:[minute]:[second]`。

## 2. 关键文件

- `crates/aether-ui/src/logging.rs`：UI 侧日志初始化入口，定义 `init_logging()` 和 `flush_logs()`。
- `crates/aether-win32/src/logging.rs`：Windows GUI 进程侧完全相同的日志初始化实现（两份代码几乎一致）。
- `crates/aether-win32/src/window.rs`：调用 `crate::logging::init_logging()` 完成实际启动时的日志系统注册。
- 各业务 crate 中直接使用 `tracing::info!` / `debug!` / `warn!` / `error!` / `trace!` 宏记录日志（如 `aether-terminal`、`aether-lsp`、`aether-ai-panel` 等）。

## 3. 架构与约定

### 3.1 初始化流程
1. 确定日志目录：`std::env::temp_dir().join("Aether").join("logs")`，即 `%TEMP%/Aether/logs/`。选择临时目录是为了避免 GUI 子系统下无 `%APPDATA%` 的问题。
2. 创建按日轮转的文件 appender：文件名前缀 `aether`，自动产出 `aether-YYYY-MM-DD.log`。
3. 构建两个 `tracing_subscriber::fmt::layer`：
   - **文件层**：关闭 ANSI、开启级别/目标/行号/文件；写入轮转文件。
   - **控制台层**：开启 ANSI、同样携带时间戳/级别/目标/行号/文件，用于调试。
4. 通过 `EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))` 设置默认日志级别为 `info`，并允许通过 `RUST_LOG` 环境变量覆盖。
5. 使用 `tracing_subscriber::registry().with(...).try_init()` 注册全局订阅者，避免重复初始化导致 panic。
6. 安装自定义 `panic` hook：捕获 panic payload 和位置信息，以 `tracing::error!` 形式记录到日志，再 flush stdout 后调用默认 hook。
7. 最后记录一条 `tracing::info!(log_dir = %..., "日志系统初始化完成")` 确认初始化成功。

### 3.2 日志字段约定
- 结构化字段采用命名参数形式，例如：
  - `error = %e, "..."`：记录错误对象。
  - `bytes = data.len(), "..."`：记录数值。
  - `panic.payload = %payload, panic.location = %location, "应用程序发生 panic"`：panic 钩子中的固定字段名。
  - `cols, rows, "set_size: 首次同步尺寸"`：终端尺寸变更。
- 所有日志均包含由 subscriber 自动注入的时间戳、级别、模块目标（target）、源文件和行号。

### 3.3 日志级别策略
- 默认级别：`info`。
- 可通过 `RUST_LOG` 环境变量调整（如 `RUST_LOG=debug` 或针对模块的过滤规则）。
- 各 crate 内部根据场景选择级别：正常流程用 `info`，调试细节用 `debug`/`trace`，异常/回退路径用 `warn`/`error`。

### 3.4 崩溃兜底
- 自定义 panic hook 确保在崩溃前将 panic 信息写入日志流，并通过 `std::io::Write::flush` 强制刷新 stdout，保证诊断信息可被捕获。
- 提供 `flush_logs()` 公共函数供关键操作后手动 flush。

## 4. 约束与规范

- **日志目录不可写时降级**：创建目录失败会 `eprintln!` 警告但不中断初始化，日志可能无法写入——这是已知的容错行为。
- **全局订阅者只初始化一次**：使用 `try_init()` 而非 `init()`，若已有订阅者则返回 `Err`，调用方需处理该错误（见 `window.rs` 中对错误的处理分支）。
- **GUI 进程与 UI crate 共享同一套初始化逻辑**：`aether-ui` 和 `aether-win32` 各自维护一份几乎相同的 `logging.rs`，说明当前没有跨 crate 共享的日志初始化库，属于重复实现。
- **日志输出不依赖 `log` crate**：项目统一使用 `tracing` 生态，未引入 `log` 或 `env_logger`，不存在桥接层。
- **日志内容不包含敏感数据**：从现有调用看，日志字段均为非敏感的业务上下文（错误码、字节数、终端尺寸、panic 位置等）。

## 5. 依赖分布

| Crate | 是否引入 tracing | 是否负责初始化 | 备注 |
|---|---:|---:|---|
| `aether-ui` | ✅ | ✅（定义 `init_logging`） | 同时依赖 `tracing-subscriber`、`tracing-appender` |
| `aether-win32` | ✅ | ✅（定义 `init_logging`，并在 `window.rs` 中调用） | 同上 |
| `aether-terminal` | ✅ | ❌ | 仅消费 `tracing::info!`/`warn!`/`error!` |
| `aether-lsp` | ✅ | ❌ | 仅消费 `tracing` |
| `aether-ai-panel` | ✅ | ❌ | 仅消费 `tracing` |
| 其他 crate | 部分引入 | 否 | 仅作为消费者 |
