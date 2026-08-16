//! ConPTY 集成烟雾测试
//!
//! 验证 `aether_win32::conpty::ConPtySession` 在真实 Windows 环境下：
//! 1. 能成功启动 `cmd.exe` 并保持运行
//! 2. 能读到子进程的初始输出（banner）
//! 3. 能接收用户输入并把回显文本写回管道
//!
//! 本测试取代了早期用于诊断 ConPTY 行为的 `test_conpty.cs` / `send_ctrl_grave.ps1`
//! 一次性脚本。相较 C# 版本，本测试直接调用生产代码（而非重复实现 Win32 绑定），
//! 可通过 `cargo test -p aether-win32 --test conpty_smoke` 反复运行。
//!
//! 注：ConPTY 与匿名管道两种后端在 cmd.exe 行为上略有差异（是否回显、
//! 是否包含 ANSI 控制序列），本测试对回显文本做子串匹配，两种后端均能通过。

use std::io::Read;
use std::sync::mpsc;
use std::time::Duration;

use aether_win32::conpty::{ConPtySession, PipeReader};

/// ConPTY/管道相关测试串行化：CreatePseudoConsole/CreatePipe 在并行运行时
/// 会互相干扰（同一进程内多对 ConPTY 同时存在时行为未定义），所以强制串行。
static CONPTY_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 优先用 `%COMSPEC%`，否则回退到常见路径。
fn cmd_path() -> String {
    std::env::var("COMSPEC")
        .ok()
        .filter(|p| std::path::Path::new(p).exists())
        .unwrap_or_else(|| r"C:\Windows\System32\cmd.exe".to_string())
}

/// 在后台线程里持续读取管道，累计字节直到满足停止条件、EOF 或超时。
///
/// 之所以需要后台线程 + 通道 + 超时：
/// - `PipeReader::read` 是阻塞 `ReadFile`，主线程直接调会卡死
///
/// 注意：不能用“等到 EOF 再返回全部字节”的模式：
/// - ConPTY 模式下输出管道在 `ClosePseudoConsole` 前不会关闭，而会话在
///   断言前仍存活，EOF 永远等不到；旧实现 `recv_timeout + unwrap_or_default`
///   会在超时时丢弃已收集的全部数据，导致明明有输出却断言 0 bytes
/// - 因此改为流式累计：有数据就累计，满足 stop 条件或超时即返回已累计内容
fn drain_until<F>(mut reader: PipeReader, timeout: Duration, stop: F) -> Vec<u8>
where
    F: Fn(&[u8]) -> bool,
{
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break, // EOF：管道关闭，子进程退出
                Ok(n) => {
                    // 主线程已返回时通道关闭，退出读循环
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    let mut collected = Vec::new();
    let deadline = std::time::Instant::now() + timeout;
    while let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) {
        match rx.recv_timeout(remaining) {
            Ok(chunk) => {
                collected.extend_from_slice(&chunk);
                if stop(&collected) {
                    break;
                }
            }
            Err(_) => break, // 超时或读线程退出：返回已累计内容
        }
    }
    collected
}

/// 烟雾测试 1：spawn 应成功，cmd.exe 至少 500ms 内不退出。
#[test]
fn conpty_smoke_spawns_cmd_and_stays_alive() {
    let _lock = CONPTY_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (session, _read_handle) =
        ConPtySession::spawn(&cmd_path(), None, 80, 24).expect("spawn 失败");
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        session.is_alive(),
        "cmd.exe 在 500ms 内退出，启动方式可能有误"
    );
    let backend = if session.is_pipe() { "pipe" } else { "ConPTY" };
    println!(
        "✓ conpty_smoke[1]: cmd.exe 启动并保持运行 (backend={})",
        backend
    );
}

/// 烟雾测试 2：spawn 后应能读到 cmd.exe 的初始 banner 输出。
#[test]
fn conpty_smoke_reads_initial_banner() {
    let _lock = CONPTY_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (session, read_handle) =
        ConPtySession::spawn(&cmd_path(), None, 80, 24).expect("spawn 失败");

    // CI 环境可能较慢，等待 cmd.exe 完全初始化并输出 banner
    std::thread::sleep(Duration::from_millis(500));

    // 写一个无害命令 + exit，确保管道最终会关闭，drain_to_eof 能返回
    session
        .write_input(b"ver\r\nexit\r\n")
        .expect("write_input 失败");

    // CI 环境给予更长的超时时间；一旦读到任何字节即可返回（不等 EOF）
    let bytes = drain_until(PipeReader::new(read_handle), Duration::from_secs(10), |c| {
        !c.is_empty()
    });
    assert!(
        !bytes.is_empty(),
        "应能读取到 cmd.exe 的输出，但读到 0 bytes (backend={})",
        if session.is_pipe() { "pipe" } else { "ConPTY" }
    );
    // 抓一段 ASCII 可读文本作概览，方便排错
    let preview: String = bytes
        .iter()
        .take(160)
        .copied()
        .filter(|b| (0x20..=0x7e).contains(b) || *b == b'\n' || *b == b'\r')
        .map(|b| b as char)
        .collect();
    println!(
        "✓ conpty_smoke[2]: 读取到 {} bytes 初始输出，前 160 可打印字符: {:?}",
        bytes.len(),
        preview
    );
}

/// 烟雾测试 3：写入 echo 命令后，回显的标记字符串必须出现在输出中。
#[test]
fn conpty_smoke_round_trip_echo() {
    let _lock = CONPTY_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (session, read_handle) =
        ConPtySession::spawn(&cmd_path(), None, 80, 24).expect("spawn 失败");

    // 用一个高熵且不可能自然出现在 banner 中的字符串作为标记
    let marker = "AETHER_CONPTY_PROBE_7E2F1A";
    let cmd = format!("echo {}\r\nexit\r\n", marker);
    session
        .write_input(cmd.as_bytes())
        .expect("write_input 失败");

    // 读到包含标记字符串即返回（不等 EOF）
    let bytes = drain_until(PipeReader::new(read_handle), Duration::from_secs(5), |c| {
        String::from_utf8_lossy(c).contains(marker)
    });
    let output = String::from_utf8_lossy(&bytes);
    assert!(
        output.contains(marker),
        "输出中应包含回显文本 '{}'，实际输出 ({} bytes, backend={}):\n{}",
        marker,
        bytes.len(),
        if session.is_pipe() { "pipe" } else { "ConPTY" },
        output
    );
    println!(
        "✓ conpty_smoke[3]: 回显 '{}' 出现在 {} bytes 输出中",
        marker,
        bytes.len()
    );
}
