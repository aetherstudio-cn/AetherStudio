//! 崩溃守卫：SEH 未处理异常过滤器 + minidump + 会话哨兵
//!
//! 背景（见 tests/BUG_REPORT_empty_folder_lsp.md 第四节）：release 构建配置为
//! `panic = "abort"` + `strip = true`，logging.rs 的 panic hook 只能覆盖 Rust 侧
//! panic；FFI/原生崩溃（Direct2D/DirectWrite 访问违例、C 库 abort、栈溢出）会
//! 绕过 panic hook 让进程无痕迹消失，日志上表现为"运行中 → 直接出现下次启动"。
//!
//! 本模块补齐三层可观测性：
//! 1. `SetUnhandledExceptionFilter`：捕获原生异常，写 minidump + 文本崩溃标记；
//! 2. 会话哨兵文件：启动时创建、正常退出时删除，下次启动检测到残留即在日志中
//!    报告"上次会话异常终止"（覆盖 panic abort、原生崩溃、强杀等所有消失路径）；
//! 3. 与现有 panic hook 互补：panic 由 hook 记日志，abort 后由哨兵兜底审计。

use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use windows::Win32::System::Diagnostics::Debug::{SetUnhandledExceptionFilter, EXCEPTION_POINTERS};

/// 崩溃产物目录（与日志同在 %TEMP%/Aether 下，便于一起排查/清理）
static CRASH_DIR: OnceLock<PathBuf> = OnceLock::new();

/// SEH 过滤器返回值：执行异常处理（终止进程）
const EXCEPTION_EXECUTE_HANDLER: i32 = 1;

fn crash_dir() -> PathBuf {
    std::env::temp_dir().join("Aether").join("crashes")
}

fn sentinel_path(dir: &Path) -> PathBuf {
    dir.join("session.sentinel")
}

/// 安装崩溃守卫。应在日志系统初始化后、窗口创建前调用。
pub fn install() {
    let dir = crash_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = CRASH_DIR.set(dir.clone());

    report_previous_session(&dir);

    // 写入本次会话哨兵（正常退出时由 mark_clean_exit 删除）
    let now = unix_now();
    let content = format!("pid={}\nstart_unix={}\n", std::process::id(), now);
    if let Err(e) = std::fs::write(sentinel_path(&dir), content) {
        tracing::warn!("崩溃守卫: 写入会话哨兵失败: {}", e);
    }

    // 注册 SEH 未处理异常过滤器（捕获 FFI/原生崩溃）
    unsafe {
        SetUnhandledExceptionFilter(Some(crash_filter));
    }
    tracing::info!("崩溃守卫已安装 crash_dir={}", dir.display());
}

/// 正常退出前调用：删除会话哨兵，标记本次会话干净退出。
pub fn mark_clean_exit() {
    let dir = CRASH_DIR.get().cloned().unwrap_or_else(crash_dir);
    let _ = std::fs::remove_file(sentinel_path(&dir));
    tracing::info!("会话正常退出，已清除哨兵");
}

/// 启动时检查上次会话是否正常退出；若哨兵残留，在日志中报告异常终止，
/// 并附带最近一次原生崩溃标记（若有）。
fn report_previous_session(dir: &Path) {
    let sentinel = sentinel_path(dir);
    if !sentinel.exists() {
        return;
    }
    let info = std::fs::read_to_string(&sentinel).unwrap_or_default();
    tracing::warn!(
        previous_session = %info.trim().replace('\n', ", "),
        "检测到上次会话未正常退出（哨兵残留）：进程可能 panic abort、原生崩溃或被强制终止"
    );
    if let Some(marker) = latest_crash_marker(dir) {
        let detail = std::fs::read_to_string(&marker).unwrap_or_default();
        tracing::warn!(
            crash_marker = %marker.display(),
            detail = %detail.trim().replace('\n', ", "),
            "最近一次原生崩溃记录"
        );
    }
}

/// 查找目录下修改时间最新的 crash_*.txt 标记
fn latest_crash_marker(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    entries
        .flatten()
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with("crash_") && name.ends_with(".txt")
        })
        .max_by_key(|e| {
            e.metadata()
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH)
        })
        .map(|e| e.path())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// SEH 未处理异常过滤器。
///
/// 崩溃上下文中堆/锁状态不可信，此处只做尽力而为的文件写入
/// （不走 tracing，避免 subscriber 内部锁死锁），随后放行进程终止。
unsafe extern "system" fn crash_filter(info: *const EXCEPTION_POINTERS) -> i32 {
    let dir = CRASH_DIR.get().cloned().unwrap_or_else(crash_dir);
    let ts = unix_now();
    let pid = std::process::id();

    // 1. 文本崩溃标记（异常码 + 异常地址，供下次启动时写入日志）
    let (code, addr) = extract_exception_info(info);
    let marker = dir.join(format!("crash_{}_{}.txt", ts, pid));
    let _ = std::fs::write(
        &marker,
        format!(
            "unix_time={}\npid={}\nexception_code=0x{:08X}\nexception_address=0x{:016X}\n",
            ts, pid, code, addr
        ),
    );

    // 2. minidump（可用 WinDbg/cv2pdb 配合构建产物分析调用栈）
    let dump = dir.join(format!("crash_{}_{}.dmp", ts, pid));
    write_minidump(&dump, info);

    EXCEPTION_EXECUTE_HANDLER
}

/// 从 EXCEPTION_POINTERS 提取异常码与异常地址（空指针安全）
unsafe fn extract_exception_info(info: *const EXCEPTION_POINTERS) -> (u32, usize) {
    if info.is_null() {
        return (0, 0);
    }
    let record = (*info).ExceptionRecord;
    if record.is_null() {
        return (0, 0);
    }
    (
        (*record).ExceptionCode.0 as u32,
        (*record).ExceptionAddress as usize,
    )
}

/// 写 minidump 到指定路径。info 为空时写不含异常上下文的进程快照。
unsafe fn write_minidump(path: &Path, info: *const EXCEPTION_POINTERS) {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_WRITE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_NONE,
    };
    use windows::Win32::System::Diagnostics::Debug::{
        MiniDumpNormal, MiniDumpWriteDump, MINIDUMP_EXCEPTION_INFORMATION,
    };
    use windows::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId,
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let Ok(hfile) = CreateFileW(
        PCWSTR(wide.as_ptr()),
        GENERIC_WRITE.0,
        FILE_SHARE_NONE,
        None,
        CREATE_ALWAYS,
        FILE_ATTRIBUTE_NORMAL,
        None,
    ) else {
        return;
    };

    let exception_info = MINIDUMP_EXCEPTION_INFORMATION {
        ThreadId: GetCurrentThreadId(),
        ExceptionPointers: info as *mut EXCEPTION_POINTERS,
        ClientPointers: false.into(),
    };
    let exception_param = if info.is_null() {
        None
    } else {
        Some(&exception_info as *const MINIDUMP_EXCEPTION_INFORMATION)
    };

    let _ = MiniDumpWriteDump(
        GetCurrentProcess(),
        GetCurrentProcessId(),
        hfile,
        MiniDumpNormal,
        exception_param,
        None,
        None,
    );
    let _ = CloseHandle(hfile);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 minidump FFI 管线可用：对当前进程写一份无异常上下文的快照
    #[test]
    fn test_write_minidump_produces_file() {
        let dir = std::env::temp_dir().join("Aether").join("crashes_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("test_{}.dmp", std::process::id()));
        unsafe {
            write_minidump(&path, std::ptr::null());
        }
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
        assert!(size > 0, "minidump 文件应非空，实际大小: {}", size);
    }

    /// 验证哨兵生命周期：写入 → 检测 → 清除
    #[test]
    fn test_sentinel_lifecycle() {
        let dir = std::env::temp_dir().join("Aether").join("sentinel_test");
        let _ = std::fs::create_dir_all(&dir);
        let sentinel = sentinel_path(&dir);

        std::fs::write(&sentinel, "pid=1\nstart_unix=0\n").unwrap();
        assert!(sentinel.exists());
        // report_previous_session 只记日志，不 panic 即可
        report_previous_session(&dir);

        let _ = std::fs::remove_file(&sentinel);
        assert!(!sentinel.exists());
        let _ = std::fs::remove_dir(&dir);
    }

    /// 验证崩溃标记检索：应返回最新的 crash_*.txt
    #[test]
    fn test_latest_crash_marker() {
        let dir = std::env::temp_dir().join("Aether").join("marker_test");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("crash_1_1.txt"), "old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join("crash_2_2.txt"), "new").unwrap();

        let latest = latest_crash_marker(&dir).expect("应找到崩溃标记");
        assert!(latest.ends_with("crash_2_2.txt"));

        let _ = std::fs::remove_file(dir.join("crash_1_1.txt"));
        let _ = std::fs::remove_file(dir.join("crash_2_2.txt"));
        let _ = std::fs::remove_dir(&dir);
    }
}
