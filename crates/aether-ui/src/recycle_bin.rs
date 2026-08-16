//! Windows 回收站操作：将文件/文件夹移入回收站（可从系统回收站恢复）。
//!
//! 使用 `SHFileOperationW`（`FO_DELETE` + `FOF_ALLOWUNDO`）实现，
//! 不永久删除文件，用户可通过 Windows 回收站恢复。

use std::path::Path;

/// 将指定路径的文件或文件夹移入 Windows 回收站。
///
/// 成功返回 `Ok(())`，失败返回错误信息字符串。
/// 注意：路径必须是绝对路径（Shell API 要求）。
pub fn move_to_recycle_bin(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    // SHFileOperationW 要求路径以双 NUL 结尾
    let wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .chain(std::iter::once(0))
        .collect();

    // SHFILEOPSTRUCTW 结构体（手动定义以避免 windows crate 中不稳定的 Shell API 绑定）
    #[repr(C)]
    #[allow(non_snake_case)]
    struct SHFILEOPSTRUCTW {
        hwnd: isize,
        wFunc: u32,
        pFrom: *const u16,
        pTo: *const u16,
        fFlags: u16,
        fAnyOperationsAborted: i32,
        hNameMappings: *mut std::ffi::c_void,
        lpszProgressTitle: *const u16,
    }

    const FO_DELETE: u32 = 0x0003;
    const FOF_ALLOWUNDO: u16 = 0x0040;
    const FOF_NOCONFIRMATION: u16 = 0x0010;
    const FOF_NOERRORUI: u16 = 0x0400;
    const FOF_SILENT: u16 = 0x0004;

    let op = SHFILEOPSTRUCTW {
        hwnd: 0,
        wFunc: FO_DELETE,
        pFrom: wide_path.as_ptr(),
        pTo: std::ptr::null(),
        fFlags: FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT,
        fAnyOperationsAborted: 0,
        hNameMappings: std::ptr::null_mut(),
        lpszProgressTitle: std::ptr::null(),
    };

    // SHFileOperationW 返回 0 表示成功
    let result = unsafe { SHFileOperationW(&op as *const _ as *const _) };
    if result == 0 {
        Ok(())
    } else {
        Err(format!("SHFileOperationW 失败，错误码: 0x{:08X}", result))
    }
}

#[link(name = "shell32")]
extern "system" {
    fn SHFileOperationW(lpFileOp: *const std::ffi::c_void) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_move_file_to_recycle_bin() {
        let dir = std::env::temp_dir().join("aether_recycle_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test_delete.txt");
        fs::write(&file, "hello").unwrap();
        assert!(file.exists());

        let result = move_to_recycle_bin(&file);
        assert!(result.is_ok(), "move_to_recycle_bin 失败: {:?}", result);
        assert!(!file.exists(), "文件应已从原位消失");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_move_dir_to_recycle_bin() {
        let dir = std::env::temp_dir().join("aether_recycle_dir_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("sub/file.txt"), "content").unwrap();
        assert!(dir.exists());

        let result = move_to_recycle_bin(&dir);
        assert!(result.is_ok(), "move_to_recycle_bin 失败: {:?}", result);
        assert!(!dir.exists(), "目录应已从原位消失");
    }
}
