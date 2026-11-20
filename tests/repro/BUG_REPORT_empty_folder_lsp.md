# Bug 复现报告：打开空文件夹后 LSP 服务进程异常终止

- 日期：2026-07-26
- 环境：Windows 25H2 / PowerShell 7 / debug 构建 `aether-app.exe`（`target\x86_64-pc-windows-msvc\debug`）
- 严重级别：中（服务进程静默死亡，主进程存活；语言服务功能全部失效且无任何 UI 提示）

## 一、现象概述

应用启动后通过文件浏览器（或 CLI 启动参数）打开一个**任意文件夹（包括空文件夹）**，
等待约 10~20 秒，随应用一同拉起的 LSP 服务进程 `rust-analyzer.exe` 会异常退出
（exit code = 1）。主进程不崩溃，但从此再无语言服务子进程，且不会重试、无任何用户提示。

## 二、复现步骤（已验证可 100% 复现）

### 方式 A：GUI 手工复现
1. 启动 `aether-app.exe`；
2. 通过菜单的"打开文件夹"选择一个空文件夹（首次打开需在"工作区信任"弹窗点"是"）；
3. 打开任务管理器，观察 `aether-app.exe` 的子进程；
4. 等待约 10~20 秒 → `rust-analyzer.exe` 子进程消失（异常终止）。

### 方式 B：脚本自动复现（本次验证使用）
```powershell
# 1. 准备空文件夹并预先加入信任列表（避免弹窗阻塞）
$empty = "$env:TEMP\aether_empty_repro"
New-Item -ItemType Directory -Force -Path $empty | Out-Null
Add-Content "$env:APPDATA\Aether\trusted_folders.txt" $empty.ToLower()

# 2. 运行监控脚本（启动应用 + 打开空文件夹 + 监控子进程 90 秒）
pwsh -NoProfile -File tests\repro\repro_empty_folder.ps1
```

### 实测监控输出（关键片段）
```text
== 主进程 PID: 15516 启动时间: 00:03:08.801
[00:03:11] 主进程存活 WS=205MB   子进程: [rust-analyzer.exe(48016)]
[00:03:22] 主进程存活 WS=204.7MB 子进程: [rust-analyzer.exe(48016)]
[00:03:25] !!! 子进程退出: rust-analyzer.exe(48016) cmd="rust-analyzer"
[00:03:25] 主进程存活 WS=203.4MB 子进程: []
...（此后 60+ 秒子进程列表始终为空，无重启/重试）
```
→ 服务进程在启动后约 **14~17 秒** 异常终止，主进程存活。

## 三、捕获到的确切错误信息

用 `tests\repro\repro_ra_stderr.py` 模拟应用的 LSP 握手（等价 initialize 参数），捕获 stderr：

```text
rust-analyzer PID=45900, rootUri=file:///C:/Users/songd/AppData/Local/Temp/aether_empty_repro
[stderr] error: Unknown binary 'rust-analyzer.exe' in official toolchain 'stable-x86_64-pc-windows-msvc'.

!!! rust-analyzer 在 11 秒后退出, exit code = 1
```

两次独立运行（不同 cwd）结果一致，退出耗时 11~14 秒。

## 四、根因分析

### 直接原因
本机 PATH 上的 `rust-analyzer.exe` 是 **rustup 的代理 shim**，而当前
stable 工具链**未安装 rust-analyzer 组件**。shim 会先花约 10 秒解析工具链清单，
然后打印上述错误并以 exit code 1 退出——这正是"等待几秒到一分钟后服务进程崩溃"的来源。

### 代码路径（应用侧放大因素）
1. `EditorState::open_folder()`（`crates/aether-win32/src/editor/files.rs` L349）
   对**任何**文件夹（无论是否为 Rust 项目、无论是否为空）都无条件调用 `init_lsp()`；
2. `init_lsp()`（`crates/aether-win32/src/editor/lsp.rs` L321-330）无条件按
   `default_server_config("rust")` 启动 `rust-analyzer`，启动失败被静默吞掉：
   `let _ = client_for_spawn.start_server("rust", config).await;`
3. `spawn_server()`（`crates/aether-lsp/src/transport.rs` L256）spawn 成功
   （shim 本身能启动），因此没有走"命令不存在"的错误分支；
4. 约 11~14 秒后 shim 报错退出 → `reader_loop` 收到 stdout EOF 退出
   （仅 `tracing::debug!` 级别记录，默认日志里不可见）→ pending 的
   `initialize` 请求收到 RecvError → `start_server` 返回 Err → 被 `let _ =` 丢弃；
5. 结果：服务进程死亡后**无重试、无状态上报、无 UI 提示**，
   `legacy_lsp_client` 仍持有已死服务器的句柄。

### 叠加风险（相同路径的次生问题）
- 即使 rust-analyzer 组件已正确安装，对**空文件夹/非 Rust 项目**启动它也无意义：
  空文件夹没有 `Cargo.toml`，rust-analyzer 会报 "Failed discovering workspace"；
- `Cargo.toml` 中 `[profile.release] panic = "abort"` + `strip = true`，
  release 构建下若后台线程发生 panic，整个主进程会无堆栈静默 abort
  （与 2026-07-26 日志中 23:40 运行中 → 23:49:43 无正常退出记录即全新启动的现象吻合，
  值得单独关注）。

## 五、修复建议

1. **按需启动 LSP**：`init_lsp` 前检查工作区是否存在 `Cargo.toml`（或按打开文件的语言
   延迟启动），空文件夹/非 Rust 项目不拉起 rust-analyzer；
2. **启动前探活**：spawn 前先以 `rust-analyzer --version` 校验二进制真实可用
   （可识别 rustup shim 报错），失败时在状态栏提示"未找到 rust-analyzer"；
3. **失败可见化**：`start_server` 的 Err 不应被 `let _ =` 丢弃，至少通过
   `LspEvent::Log`/状态栏上报；`reader_loop` 退出（服务进程死亡）应升级为
   `tracing::warn!` 并推送事件；
4. **进程退出监听**：持有的 `child` 应有 wait 监听，异常退出时清理
   `legacy_lsp_client` 并按退避策略决定是否重试。

## 六、附件

- 监控脚本：`tests/repro/repro_empty_folder.ps1`
- stderr 捕获脚本：`tests/repro/repro_ra_stderr.py`

## 七、修复记录（2026-07-27）

修复建议 1~4 已全部实施并验证：

| 修改点 | 文件 | 内容 |
| --- | --- | --- |
| 按需启动 | `crates/aether-win32/src/editor/lsp.rs` | `init_lsp` 仅在根目录存在 `Cargo.toml` 时才拉起 rust-analyzer |
| 启动前探活 | `crates/aether-lsp/src/transport.rs` | 新增 `probe_server_command`：以 `--version` 验证二进制真实可用（30s 超时），识别 rustup shim 组件未安装场景；`init_lsp` 与 `lsp_notify_open` 两条启动路径均接入 |
| 失败可见化 | `crates/aether-win32/src/editor/lsp.rs`、`crates/aether-lsp/src/server.rs` | `start_server` 的 Err 不再被 `let _ =` 丢弃（tracing::warn）；`reader_loop` 退出由 debug 升级为 warn 并推送事件 |
| 进程退出监听 | `crates/aether-lsp/src/client.rs`、`crates/aether-win32/src/editor/lsp.rs` | 新增 `LspEvent::ServerExited` 事件与 `LspClient::remove_server`；UI 收到事件后状态栏提示"LSP 服务器已退出"，并移除死服务器句柄使后续可按需重启 |

### 验证结果

- `cargo test -p aether-lsp`：97 passed / 0 failed；
- 空文件夹场景：打开后 30+ 秒监控，**不再拉起 rust-analyzer**，主进程稳定；
- Rust 项目场景（存在 `Cargo.toml`、本机 rust-analyzer 为不可用 shim）：
  探活进程运行约 12 秒后识别失败，记录 warn 日志并优雅跳过，无死服务进程残留：
  ```text
  2026-07-27 00:30:38  WARN aether_win32::editor::lsp: LSP 探活失败，跳过启动
  rust-analyzer: LSP server probe failed (rust-analyzer): error: Unknown binary
  'rust-analyzer.exe' in official toolchain 'stable-x86_64-pc-windows-msvc'.
  ```

### 验证脚本勘误

原 `repro_empty_folder.ps1` 使用 `Start-Process -ArgumentList` 传递 JSON 参数时引号
会被吞掉，导致 CLI 参数解析失败、实际打开的是恢复的 `last_workspace`。验证修复时
改用 `& $exe --aether-launch-args $json` 直接调用（pwsh 自动补引号），并以
`settings.json` 中 `last_workspace` 是否变为目标路径确认参数生效。

## 八、叠加风险修复记录（2026-07-27）：崩溃可观测性

第四节"叠加风险"（`panic = "abort"` + `strip = true` 下进程无痕迹消失）已修复，
新增 `crates/aether-win32/src/crash_guard.rs` 崩溃守卫模块，补齐三层可观测性：

1. **SEH 未处理异常过滤器**（`SetUnhandledExceptionFilter`）：捕获 Rust panic hook
   覆盖不到的 FFI/原生崩溃（Direct2D/DirectWrite 访问违例、C 库 abort 等），
   崩溃时写文本标记（异常码+地址）与 minidump 到 `%TEMP%/Aether/crashes/`；
2. **会话哨兵文件**：启动时写入 `session.sentinel`、消息循环正常退出时删除；
   下次启动检测到残留即在日志中 WARN"检测到上次会话未正常退出"，并附最近一次
   原生崩溃标记详情——覆盖 panic abort、原生崩溃、强杀等所有消失路径；
3. 与 `logging.rs` 现有 panic hook 互补：panic 由 hook 记日志后 abort，
   哨兵在下次启动兜底审计。

### 验证结果

- 单元测试 3 项全过（含真实 `MiniDumpWriteDump` FFI 调用产出非空 dmp）；
- 端到端：启动 → 强杀（模拟崩溃）→ 再启动，日志出现
  `WARN 检测到上次会话未正常退出（哨兵残留）previous_session=pid=44136`；
  随后正常关闭，日志出现 `会话正常退出，已清除哨兵`，哨兵文件消失。


