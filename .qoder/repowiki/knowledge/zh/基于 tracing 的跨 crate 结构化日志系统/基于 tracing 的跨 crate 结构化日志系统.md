---
kind: logging_system
name: 基于 tracing 的跨 crate 结构化日志系统
category: logging_system
scope:
    - '**'
source_files:
    - crates/aether-ui/src/logging.rs
    - crates/aether-win32/src/logging.rs
    - crates/aether-win32/src/window.rs
    - crates/aether-ui/Cargo.toml
    - crates/aether-win32/Cargo.toml
    - crates/aether-terminal/src/conpty.rs
    - crates/aether-terminal/src/terminal.rs
    - crates/aether-lsp/src/server.rs
---

## 1. 使用的框架与工具

仓库采用 **tracing** 生态作为统一的日志/追踪框架：
- `tracing`（0.1）——各 crate 通过 `tracing::info!` / `debug!` / `warn!` / `error!` / `trace!` 宏记录事件，并支持结构化字段（如 `error = %e, "..."`、`bytes = data.len(), "send_bytes: ..."`）。
- `tracing-subscriber`（0.3，启用 `fmt`、`env-filter`、`time` features）——负责订阅全局 subscriber，格式化输出。
- `tracing-appender`（0.2）+ `RollingFileAppender` + `Rotation::DAILY`——按日轮转写入文件。
- `time`（0.3，启用 `formatting`、`local-offset`）——使用本地时区 `OffsetTime` 格式化时间戳。

没有使用 `log` facade、`env_logger`、`simple_logger`、`slog`、`fern`、`log4rs` 等其它方案；整个工作区统一围绕 tracing 构建。

## 2. 核心文件与位置

| 文件 | 作用 |
|---|---|
| `crates/aether-ui/src/logging.rs` | UI crate 的日志初始化模块（`init_logging`、`flush_logs`、panic hook） |
| `crates/aether-win32/src/logging.rs` | Windows GUI 二进制入口侧的日志初始化模块（与 UI 版本几乎一致） |
| `crates/aether-win32/src/window.rs` | 在应用启动早期调用 `crate::logging::init_logging()`，失败时生成 `aether_init_logging_error_{pid}.txt` 错误报告 |
| `crates/aether-ui/Cargo.toml` | 声明 `tracing`、`tracing-subscriber`、`tracing-appender`、`time` 依赖 |
| `crates/aether-win32/Cargo.toml` | 同上，供最终可执行二进制使用 |

## 3. 架构与约定

### 初始化流程
每个 crate 暴露 `init_logging() -> io::Result<()>`，由上层（`aether-win32` 的 `window.rs`）在应用启动时调用。初始化步骤固定为：
1. 确定日志目录：`std::env::temp_dir().join("Aether").join("logs")`，并通过 `create_dir_all` 确保存在。
2. 创建 `RollingFileAppender(Rotation::DAILY, log_dir, "aether")`，产生 `aether-YYYY-MM-DD.log` 格式的文件名。
3. 构造本地时区的 `OffsetTime`，默认格式 `[year]-[month]-[day] [hour]:[minute]:[second]`，回退到仅日期格式。
4. 注册两个 `tracing_subscriber::fmt::layer`：
   - **文件层**：`with_ansi(false)`，包含 level、target、line_number、file。
   - **控制台层**：`with_ansi(true)`，同样包含 level、target、line_number、file。
5. 通过 `EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))` 设置默认级别为 `info`，允许通过 `RUST_LOG` 环境变量覆盖。
6. 使用 `tracing_subscriber::registry().with(...).try_init()` 安装全局 subscriber（避免重复初始化 panic）。
7. 安装自定义 panic hook：捕获 payload 和 location，以 `tracing::error!(panic.payload = ..., panic.location = ..., "应用程序发生 panic")` 记录后调用默认 hook。
8. 提供 `flush_logs()` 用于关键操作后强制 flush stdout。

### 日志输出结构
文件日志行包含：时间戳（本地时区）、级别、目标模块（target）、源文件名、行号。控制台日志额外保留 ANSI 颜色。

### 结构化字段约定
业务代码普遍使用命名字段而非纯字符串插值，例如：
- `tracing::warn!(error = %e, "ConPTY: AllocConsole 失败...")`
- `tracing::info!(cols, rows, "set_size: 首次同步尺寸")`
- `tracing::info!(bytes = n, count = read_count, preview = %preview, "终端读取线程读到数据")`
- `tracing::error!(error = %e, bytes = data.len(), "send_bytes: write_input 失败")`

这使日志可通过 `tracing` 的事件查询能力进行过滤与聚合。

### 日志级别策略
- 默认级别：`info`（通过 `EnvFilter::new("info")` 设定）。
- 可通过 `RUST_LOG` 环境变量覆盖（例如 `RUST_LOG=debug` 或按模块指定）。
- 各 crate 中 `trace!` / `debug!` 仅在需要详细诊断时使用（如 LSP 未处理请求、终端字节流调试）。

## 4. 约定与约束

- **日志目录固定**：所有日志写入 `%TEMP%/Aether/logs/`，避免 GUI 子系统下无 `%APPDATA%` 的问题（见 `get_log_dir` 注释）。
- **按日轮转**：使用 `Rotation::DAILY`，旧日志不会无限增长。
- **panic 安全**：自定义 panic hook 会在崩溃前用 `tracing::error!` 记录 panic 信息，并尝试 flush stdout。
- **幂等初始化**：使用 `try_init()` 而非 `init()`，避免重复初始化导致 panic。
- **双 sink**：同时输出到文件和控制台，便于开发期调试和生产期归档。
- **跨 crate 一致性**：`aether-ui` 与 `aether-win32` 中的 `logging.rs` 实现几乎完全相同，保证行为一致。
- **不引入 `log` 桥接**：各 crate 直接依赖 `tracing` 并使用其宏，未使用 `log` crate 做 facade 适配。

## 5. 依赖关系图

```
aether-win32 (bin)
  └── aether-ui (lib)
        ├── tracing (emit events)
        ├── tracing-subscriber (fmt layer, env filter, time)
        ├── tracing-appender (rolling file appender)
        └── time (local offset formatting)
```

其他 crate（`aether-terminal`、`aether-lsp`、`aether-ai-panel` 等）仅消费 `tracing` 宏，由顶层 `aether-win32` 完成 subscriber 初始化。
