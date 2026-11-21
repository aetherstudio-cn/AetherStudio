//! 模拟用户输入（SendInput）：供 Agent 自动化测试与 repro 脚本驱动 UI。
//!
//! 脚本行格式（每行一个动作，`#` 开头为注释）：
//! - `type <文本>`：以 KEYEVENTF_UNICODE 逐 UTF-16 单元输入文本（支持中文）
//! - `key <ctrl+shift+a>`：组合键/单键（修饰键 ctrl/shift/alt/win + 主键名）
//! - `click <x,y>`：屏幕物理坐标左键单击（绝对坐标归一化 65535，DPI 感知）
//! - `sleep <ms>`：等待指定毫秒（由 UI 线程定时器调度，不阻塞消息泵）
//!
//! 安全边界：Agent 发起的输入默认必须经确认卡片 + 确认对话框人工放行；
//! 设置项 `agent.input_enabled` 可整体关闭 Agent 输入能力。
//! 本模块注入的按键带 LLKHF_INJECTED 标志，keyboard_hook 已跳过，不会误吞。

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MOVE, MOUSEINPUT, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END,
    VK_ESCAPE, VK_F1, VK_F10, VK_F11, VK_F12, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8,
    VK_F9, VK_HOME, VK_INSERT, VK_LEFT, VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN,
    VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

/// 单个输入动作（解析器产物，纯数据、可单测）
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputAction {
    /// 输入文本（Unicode）
    Type(String),
    /// 按键：修饰键列表 + 主键名（均已小写规范化）
    Key { modifiers: Vec<String>, key: String },
    /// 屏幕物理坐标左键单击
    Click { x: i32, y: i32 },
    /// 等待毫秒（上限 30 秒，防止脚本长时间挂起）
    Sleep(u64),
}

/// sleep 动作上限（毫秒）
pub const MAX_SLEEP_MS: u64 = 30_000;

/// 解析输入脚本为动作序列；任一行非法即整体报错（附行号），避免半截执行。
pub fn parse_input_script(text: &str) -> Result<Vec<InputAction>, String> {
    let mut actions = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let no = idx + 1;
        let (verb, rest) = match line.split_once(char::is_whitespace) {
            Some((v, r)) => (v.to_ascii_lowercase(), r.trim()),
            None => (line.to_ascii_lowercase(), ""),
        };
        match verb.as_str() {
            "type" => {
                if rest.is_empty() {
                    return Err(format!("第 {} 行：type 缺少文本", no));
                }
                actions.push(InputAction::Type(rest.to_string()));
            }
            "key" => {
                if rest.is_empty() {
                    return Err(format!("第 {} 行：key 缺少按键名", no));
                }
                let parts: Vec<String> = rest
                    .split('+')
                    .map(|p| p.trim().to_ascii_lowercase())
                    .collect();
                if parts.iter().any(|p| p.is_empty()) {
                    return Err(format!("第 {} 行：key 组合含空键名", no));
                }
                let (mods, key) = parts.split_at(parts.len() - 1);
                for m in mods {
                    if !matches!(m.as_str(), "ctrl" | "shift" | "alt" | "win") {
                        return Err(format!("第 {} 行：未知修饰键 `{}`", no, m));
                    }
                }
                let key = key[0].clone();
                if vk_from_name(&key).is_none() {
                    return Err(format!("第 {} 行：未知按键 `{}`", no, key));
                }
                actions.push(InputAction::Key {
                    modifiers: mods.to_vec(),
                    key,
                });
            }
            "click" => {
                let (xs, ys) = rest
                    .split_once(',')
                    .ok_or_else(|| format!("第 {} 行：click 需要 `x,y` 坐标", no))?;
                let x: i32 = xs
                    .trim()
                    .parse()
                    .map_err(|_| format!("第 {} 行：click x 坐标非法", no))?;
                let y: i32 = ys
                    .trim()
                    .parse()
                    .map_err(|_| format!("第 {} 行：click y 坐标非法", no))?;
                actions.push(InputAction::Click { x, y });
            }
            "sleep" => {
                let ms: u64 = rest
                    .parse()
                    .map_err(|_| format!("第 {} 行：sleep 需要毫秒数", no))?;
                if ms > MAX_SLEEP_MS {
                    return Err(format!("第 {} 行：sleep 超过上限 {}ms", no, MAX_SLEEP_MS));
                }
                actions.push(InputAction::Sleep(ms));
            }
            _ => return Err(format!("第 {} 行：未知指令 `{}`", no, verb)),
        }
    }
    if actions.is_empty() {
        return Err("脚本为空（无有效动作）".to_string());
    }
    Ok(actions)
}

/// 键名 → 虚拟键码（小写字母/数字/F1-F12/常用命名键）
pub fn vk_from_name(name: &str) -> Option<VIRTUAL_KEY> {
    let n = name.to_ascii_lowercase();
    Some(match n.as_str() {
        "ctrl" | "control" => VK_CONTROL,
        "shift" => VK_SHIFT,
        "alt" => VK_MENU,
        "win" | "meta" | "super" => VK_LWIN,
        "enter" | "return" => VK_RETURN,
        "tab" => VK_TAB,
        "esc" | "escape" => VK_ESCAPE,
        "space" => VK_SPACE,
        "backspace" => VK_BACK,
        "delete" | "del" => VK_DELETE,
        "insert" | "ins" => VK_INSERT,
        "home" => VK_HOME,
        "end" => VK_END,
        "pageup" | "pgup" => VK_PRIOR,
        "pagedown" | "pgdn" => VK_NEXT,
        "up" => VK_UP,
        "down" => VK_DOWN,
        "left" => VK_LEFT,
        "right" => VK_RIGHT,
        "f1" => VK_F1,
        "f2" => VK_F2,
        "f3" => VK_F3,
        "f4" => VK_F4,
        "f5" => VK_F5,
        "f6" => VK_F6,
        "f7" => VK_F7,
        "f8" => VK_F8,
        "f9" => VK_F9,
        "f10" => VK_F10,
        "f11" => VK_F11,
        "f12" => VK_F12,
        _ => {
            let b = n.as_bytes();
            if b.len() == 1 && b[0].is_ascii_lowercase() {
                VIRTUAL_KEY(0x41 + (b[0] - b'a') as u16)
            } else if b.len() == 1 && b[0].is_ascii_digit() {
                VIRTUAL_KEY(0x30 + (b[0] - b'0') as u16)
            } else {
                return None;
            }
        }
    })
}

/// 动作清单摘要（确认卡片/对话框展示用）
pub fn action_summary(actions: &[InputAction]) -> String {
    let mut out = String::new();
    for (i, a) in actions.iter().enumerate() {
        let desc = match a {
            InputAction::Type(t) => {
                let shown: String = t.chars().take(40).collect();
                let ellipsis = if t.chars().count() > 40 { "…" } else { "" };
                format!("输入文本 \"{}{}\"（{} 字符）", shown, ellipsis, t.chars().count())
            }
            InputAction::Key { modifiers, key } => {
                let mut combo = modifiers.join("+");
                if !combo.is_empty() {
                    combo.push('+');
                }
                combo.push_str(key);
                format!("按键 {}", combo)
            }
            InputAction::Click { x, y } => format!("左键点击 ({}, {})", x, y),
            InputAction::Sleep(ms) => format!("等待 {}ms", ms),
        };
        out.push_str(&format!("{}. {}\n", i + 1, desc));
    }
    out
}

/// 执行单个动作（Sleep 由调度器处理，此处直接返回成功）。
/// 所有按键/鼠标事件经 SendInput 注入，带注入标志不被自家键盘钩子拦截。
pub fn execute_action(action: &InputAction) -> Result<(), String> {
    match action {
        InputAction::Sleep(_) => Ok(()),
        InputAction::Type(text) => {
            let mut inputs: Vec<INPUT> = Vec::new();
            for unit in text.encode_utf16() {
                inputs.push(key_input(0, unit, KEYEVENTF_UNICODE));
                inputs.push(key_input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
            }
            send(&inputs)
        }
        InputAction::Key { modifiers, key } => {
            let vk = vk_from_name(key).ok_or_else(|| format!("未知按键 `{}`", key))?;
            let mut inputs: Vec<INPUT> = Vec::new();
            let mut mod_vks: Vec<VIRTUAL_KEY> = Vec::new();
            for m in modifiers {
                let mv = vk_from_name(m).ok_or_else(|| format!("未知修饰键 `{}`", m))?;
                mod_vks.push(mv);
                inputs.push(key_input(mv.0, 0, Default::default()));
            }
            inputs.push(key_input(vk.0, 0, Default::default()));
            inputs.push(key_input(vk.0, 0, KEYEVENTF_KEYUP));
            for mv in mod_vks.into_iter().rev() {
                inputs.push(key_input(mv.0, 0, KEYEVENTF_KEYUP));
            }
            send(&inputs)
        }
        InputAction::Click { x, y } => {
            // 绝对坐标归一化到 0..65535（覆盖整个虚拟屏幕，物理像素）
            let (vx, vy, vw, vh) = unsafe {
                (
                    GetSystemMetrics(SM_XVIRTUALSCREEN),
                    GetSystemMetrics(SM_YVIRTUALSCREEN),
                    GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1),
                    GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1),
                )
            };
            let nx = ((*x - vx) as i64 * 65535 / (vw as i64 - 1).max(1)) as i32;
            let ny = ((*y - vy) as i64 * 65535 / (vh as i64 - 1).max(1)) as i32;
            let move_input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: nx,
                        dy: ny,
                        dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
                        ..Default::default()
                    },
                },
            };
            let down = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dwFlags: MOUSEEVENTF_LEFTDOWN,
                        ..Default::default()
                    },
                },
            };
            let up = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dwFlags: MOUSEEVENTF_LEFTUP,
                        ..Default::default()
                    },
                },
            };
            send(&[move_input, down, up])
        }
    }
}

fn key_input(vk: u16, scan: u16, flags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: scan,
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}

fn send(inputs: &[INPUT]) -> Result<(), String> {
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        return Err(format!("SendInput 仅注入 {}/{} 个事件", sent, inputs.len()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_full_script() {
        let script = "# 注释\n\ntype 你好 world\nkey ctrl+shift+a\nclick 100,200\nsleep 500\n";
        let actions = parse_input_script(script).unwrap();
        assert_eq!(
            actions,
            vec![
                InputAction::Type("你好 world".to_string()),
                InputAction::Key { modifiers: vec!["ctrl".into(), "shift".into()], key: "a".into() },
                InputAction::Click { x: 100, y: 200 },
                InputAction::Sleep(500),
            ]
        );
    }

    #[test]
    fn test_parse_single_key() {
        let actions = parse_input_script("key enter").unwrap();
        assert_eq!(
            actions,
            vec![InputAction::Key { modifiers: vec![], key: "enter".into() }]
        );
    }

    #[test]
    fn test_parse_errors() {
        assert!(parse_input_script("").is_err());
        assert!(parse_input_script("# 只有注释").is_err());
        assert!(parse_input_script("type").is_err());
        assert!(parse_input_script("key").is_err());
        assert!(parse_input_script("key ctrl+").is_err());
        assert!(parse_input_script("key foo+a").is_err());
        assert!(parse_input_script("key ctrl+nosuchkey").is_err());
        assert!(parse_input_script("click 100").is_err());
        assert!(parse_input_script("click a,b").is_err());
        assert!(parse_input_script("sleep abc").is_err());
        assert!(parse_input_script("sleep 99999999").is_err());
        assert!(parse_input_script("dance now").is_err());
        // 错误信息附行号
        let err = parse_input_script("type ok\nkey bad").unwrap_err();
        assert!(err.contains("第 2 行"), "{}", err);
    }

    #[test]
    fn test_vk_mapping() {
        assert_eq!(vk_from_name("a"), Some(VIRTUAL_KEY(0x41)));
        assert_eq!(vk_from_name("Z"), Some(VIRTUAL_KEY(0x5A)));
        assert_eq!(vk_from_name("0"), Some(VIRTUAL_KEY(0x30)));
        assert_eq!(vk_from_name("9"), Some(VIRTUAL_KEY(0x39)));
        assert_eq!(vk_from_name("enter"), Some(VK_RETURN));
        assert_eq!(vk_from_name("ESC"), Some(VK_ESCAPE));
        assert_eq!(vk_from_name("f12"), Some(VK_F12));
        assert_eq!(vk_from_name("pageup"), Some(VK_PRIOR));
        assert_eq!(vk_from_name("nosuch"), None);
    }

    #[test]
    fn test_action_summary() {
        let actions = parse_input_script("type hi\nkey ctrl+s\nclick 1,2\nsleep 50").unwrap();
        let s = action_summary(&actions);
        assert!(s.contains("1. 输入文本 \"hi\"（2 字符）"), "{}", s);
        assert!(s.contains("2. 按键 ctrl+s"), "{}", s);
        assert!(s.contains("3. 左键点击 (1, 2)"), "{}", s);
        assert!(s.contains("4. 等待 50ms"), "{}", s);
    }

    #[test]
    fn test_summary_truncates_long_text() {
        let long = "x".repeat(100);
        let actions = vec![InputAction::Type(long)];
        let s = action_summary(&actions);
        assert!(s.contains("…"), "{}", s);
        assert!(s.contains("100 字符"), "{}", s);
    }
}