//! 模拟用户输入（SendInput）：供 Agent 自动化测试与 repro 脚本驱动 UI。
//!
//! 脚本行格式（每行一个动作，`#` 开头为注释）：
//! - `type <文本>`：以 KEYEVENTF_UNICODE 逐 UTF-16 单元输入文本（支持中文）
//! - `key <ctrl+shift+a>`：组合键/单键（修饰键 ctrl/shift/alt/win + 主键名）
//! - `move <坐标>`：移动鼠标
//! - `click <坐标> [left|right|middle]`：单击（默认左键）
//! - `dblclick <坐标> [left|right|middle]`：双击
//! - `down [<坐标>] [left|right|middle]`：按下鼠标（可先移动；拖拽起点）
//! - `up [left|right|middle]`：释放鼠标（拖拽终点）
//! - `drag <起点> <终点> [left|right|middle]`：按下并拖拽到终点后释放
//! - `scroll <格数>` / `scroll <坐标> <格数>`：滚轮（正数向上、负数向下，一格 = WHEEL_DELTA）
//! - `sleep <ms>`：等待指定毫秒（由 UI 线程定时器调度，不阻塞消息泵）
//!
//! 坐标：`x,y` 为屏幕物理像素；`win:x,y` 为相对应用窗口左上角（与 AETHER_SHOT
//! window 截图像素一一对应，操作本应用时推荐使用）。
//!
//! 安全边界：Agent 发起的输入默认必须经确认卡片 + 确认对话框人工放行；
//! 设置项 `agent.input_enabled` 可整体关闭 Agent 输入能力。
//! 本模块注入的按键带 LLKHF_INJECTED 标志，keyboard_hook 已跳过，不会误吞。

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY, VK_BACK,
    VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_F10, VK_F11, VK_F12, VK_F2, VK_F3,
    VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_HOME, VK_INSERT, VK_LEFT, VK_LWIN, VK_MENU,
    VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, GetWindowRect, IsIconic, SetForegroundWindow, ShowWindow, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_RESTORE,
};

/// 鼠标按键
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

impl MouseButton {
    pub fn label(self) -> &'static str {
        match self {
            MouseButton::Left => "左键",
            MouseButton::Right => "右键",
            MouseButton::Middle => "中键",
        }
    }

    fn flags(self) -> (MOUSE_EVENT_FLAGS, MOUSE_EVENT_FLAGS) {
        match self {
            MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
            MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
            MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
        }
    }
}

/// 目标坐标：屏幕物理像素，或相对应用窗口左上角（与窗口截图像素一致）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AimPoint {
    pub x: i32,
    pub y: i32,
    /// true = 相对应用窗口左上角；false = 屏幕物理像素
    pub window: bool,
}

impl AimPoint {
    pub fn describe(&self) -> String {
        if self.window {
            format!("窗口内 ({}, {})", self.x, self.y)
        } else {
            format!("屏幕 ({}, {})", self.x, self.y)
        }
    }
}

/// 单个输入动作（解析器产物，纯数据、可单测）
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputAction {
    /// 输入文本（Unicode）
    Type(String),
    /// 按键：修饰键列表 + 主键名（均已小写规范化）
    Key { modifiers: Vec<String>, key: String },
    /// 移动鼠标
    Move(AimPoint),
    /// 单击
    Click { at: AimPoint, button: MouseButton },
    /// 双击
    DoubleClick { at: AimPoint, button: MouseButton },
    /// 按下鼠标（可选先移动到指定坐标）
    ButtonDown {
        at: Option<AimPoint>,
        button: MouseButton,
    },
    /// 释放鼠标
    ButtonUp { button: MouseButton },
    /// 从起点按下并拖拽到终点后释放
    Drag {
        from: AimPoint,
        to: AimPoint,
        button: MouseButton,
    },
    /// 滚轮：正数向上、负数向下（一格 = WHEEL_DELTA = 120）
    Scroll { at: Option<AimPoint>, notches: i32 },
    /// 等待毫秒（上限 30 秒，防止脚本长时间挂起）
    Sleep(u64),
}

/// sleep 动作上限（毫秒）
pub const MAX_SLEEP_MS: u64 = 30_000;

/// 单次滚动上限（格），防止一次事件滚动量过大
const MAX_SCROLL_NOTCHES: i32 = 100;

/// 拖拽过程中的中间移动步数（让目标窗口收到连续 WM_MOUSEMOVE）
const DRAG_STEPS: i32 = 8;

/// 解析坐标：`x,y`（屏幕）或 `win:x,y`（应用窗口内）
fn parse_point(tok: &str, no: usize) -> Result<AimPoint, String> {
    let (window, body) = match tok.strip_prefix("win:") {
        Some(rest) => (true, rest),
        None => (false, tok),
    };
    let (xs, ys) = body
        .split_once(',')
        .ok_or_else(|| format!("第 {} 行：坐标需要 `x,y` 或 `win:x,y`", no))?;
    let x: i32 = xs
        .trim()
        .parse()
        .map_err(|_| format!("第 {} 行：x 坐标非法", no))?;
    let y: i32 = ys
        .trim()
        .parse()
        .map_err(|_| format!("第 {} 行：y 坐标非法", no))?;
    Ok(AimPoint { x, y, window })
}

fn parse_button(tok: &str, no: usize) -> Result<MouseButton, String> {
    match tok.to_ascii_lowercase().as_str() {
        "left" | "l" => Ok(MouseButton::Left),
        "right" | "r" => Ok(MouseButton::Right),
        "middle" | "m" => Ok(MouseButton::Middle),
        other => Err(format!("第 {} 行：未知鼠标按键 `{}`", no, other)),
    }
}

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
        let toks: Vec<&str> = rest.split_whitespace().collect();
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
            "move" => {
                if toks.len() != 1 {
                    return Err(format!("第 {} 行：move 需要 1 个坐标", no));
                }
                actions.push(InputAction::Move(parse_point(toks[0], no)?));
            }
            "click" | "dblclick" => {
                if toks.is_empty() || toks.len() > 2 {
                    return Err(format!(
                        "第 {} 行：{} 需要 `<坐标> [left|right|middle]`",
                        no, verb
                    ));
                }
                let at = parse_point(toks[0], no)?;
                let button = match toks.get(1) {
                    Some(b) => parse_button(b, no)?,
                    None => MouseButton::Left,
                };
                if verb == "click" {
                    actions.push(InputAction::Click { at, button });
                } else {
                    actions.push(InputAction::DoubleClick { at, button });
                }
            }
            "down" => {
                if toks.len() > 2 {
                    return Err(format!("第 {} 行：down 参数过多", no));
                }
                let (at, button) = match toks.first() {
                    None => (None, MouseButton::Left),
                    Some(first)
                        if matches!(
                            first.to_ascii_lowercase().as_str(),
                            "left" | "l" | "right" | "r" | "middle" | "m"
                        ) =>
                    {
                        (None, parse_button(first, no)?)
                    }
                    Some(first) => {
                        let at = parse_point(first, no)?;
                        let button = match toks.get(1) {
                            Some(b) => parse_button(b, no)?,
                            None => MouseButton::Left,
                        };
                        (Some(at), button)
                    }
                };
                actions.push(InputAction::ButtonDown { at, button });
            }
            "up" => {
                if toks.len() > 1 {
                    return Err(format!("第 {} 行：up 参数过多", no));
                }
                let button = match toks.first() {
                    Some(b) => parse_button(b, no)?,
                    None => MouseButton::Left,
                };
                actions.push(InputAction::ButtonUp { button });
            }
            "drag" => {
                if !(2..=3).contains(&toks.len()) {
                    return Err(format!(
                        "第 {} 行：drag 需要 `<起点> <终点> [left|right|middle]`",
                        no
                    ));
                }
                let from = parse_point(toks[0], no)?;
                let to = parse_point(toks[1], no)?;
                let button = match toks.get(2) {
                    Some(b) => parse_button(b, no)?,
                    None => MouseButton::Left,
                };
                actions.push(InputAction::Drag { from, to, button });
            }
            "scroll" => {
                let (at, notches_tok) = match toks.len() {
                    1 => (None, toks[0]),
                    2 => (Some(parse_point(toks[0], no)?), toks[1]),
                    _ => {
                        return Err(format!(
                            "第 {} 行：scroll 需要 `<格数>` 或 `<坐标> <格数>`",
                            no
                        ))
                    }
                };
                let notches: i32 = notches_tok
                    .parse()
                    .map_err(|_| format!("第 {} 行：滚动格数非法", no))?;
                if notches == 0 {
                    return Err(format!("第 {} 行：滚动格数不能为 0", no));
                }
                if notches.abs() > MAX_SCROLL_NOTCHES {
                    return Err(format!(
                        "第 {} 行：滚动格数超过上限 {}",
                        no, MAX_SCROLL_NOTCHES
                    ));
                }
                actions.push(InputAction::Scroll { at, notches });
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
                format!(
                    "输入文本 \"{}{}\"（{} 字符）",
                    shown,
                    ellipsis,
                    t.chars().count()
                )
            }
            InputAction::Key { modifiers, key } => {
                let mut combo = modifiers.join("+");
                if !combo.is_empty() {
                    combo.push('+');
                }
                combo.push_str(key);
                format!("按键 {}", combo)
            }
            InputAction::Move(p) => format!("移动鼠标到{}", p.describe()),
            InputAction::Click { at, button } => format!("{}单击{}", button.label(), at.describe()),
            InputAction::DoubleClick { at, button } => {
                format!("{}双击{}", button.label(), at.describe())
            }
            InputAction::ButtonDown { at, button } => match at {
                Some(p) => format!("{}按下{}", button.label(), p.describe()),
                None => format!("{}按下", button.label()),
            },
            InputAction::ButtonUp { button } => format!("{}释放", button.label()),
            InputAction::Drag { from, to, button } => format!(
                "{}拖拽{} → {}",
                button.label(),
                from.describe(),
                to.describe()
            ),
            InputAction::Scroll { at, notches } => {
                let dir = if *notches > 0 { "向上" } else { "向下" };
                match at {
                    Some(p) => format!("在{}滚动 {} 格（{}）", p.describe(), notches.abs(), dir),
                    None => format!("滚动滚轮 {} 格（{}）", notches.abs(), dir),
                }
            }
            InputAction::Sleep(ms) => format!("等待 {}ms", ms),
        };
        out.push_str(&format!("{}. {}\n", i + 1, desc));
    }
    out
}

/// 执行单个动作（Sleep 由调度器处理，此处直接返回成功）。
/// 所有按键/鼠标事件经 SendInput 注入，带注入标志不被自家键盘钩子拦截。
/// `win:` 坐标以应用窗口为锚点，执行前会尽力激活并还原窗口。
pub fn execute_action(action: &InputAction, app_hwnd: HWND) -> Result<(), String> {
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
        InputAction::Move(p) => {
            let (x, y) = resolve_point(p, app_hwnd)?;
            send(&[mouse_move_input(x, y)])
        }
        InputAction::Click { at, button } => {
            let (x, y) = resolve_point(at, app_hwnd)?;
            let (down, up) = button.flags();
            send(&[
                mouse_move_input(x, y),
                mouse_button_input(down),
                mouse_button_input(up),
            ])
        }
        InputAction::DoubleClick { at, button } => {
            let (x, y) = resolve_point(at, app_hwnd)?;
            let (down, up) = button.flags();
            send(&[
                mouse_move_input(x, y),
                mouse_button_input(down),
                mouse_button_input(up),
                mouse_button_input(down),
                mouse_button_input(up),
            ])
        }
        InputAction::ButtonDown { at, button } => {
            let mut inputs: Vec<INPUT> = Vec::new();
            if let Some(p) = at {
                let (x, y) = resolve_point(p, app_hwnd)?;
                inputs.push(mouse_move_input(x, y));
            }
            inputs.push(mouse_button_input(button.flags().0));
            send(&inputs)
        }
        InputAction::ButtonUp { button } => send(&[mouse_button_input(button.flags().1)]),
        InputAction::Drag { from, to, button } => {
            let (x1, y1) = resolve_point(from, app_hwnd)?;
            let (x2, y2) = resolve_point(to, app_hwnd)?;
            let (down, up) = button.flags();
            let mut inputs = vec![mouse_move_input(x1, y1), mouse_button_input(down)];
            for step in 1..=DRAG_STEPS {
                let x = x1 + (x2 - x1) * step / DRAG_STEPS;
                let y = y1 + (y2 - y1) * step / DRAG_STEPS;
                inputs.push(mouse_move_input(x, y));
            }
            inputs.push(mouse_button_input(up));
            send(&inputs)
        }
        InputAction::Scroll { at, notches } => {
            let mut inputs: Vec<INPUT> = Vec::new();
            if let Some(p) = at {
                let (x, y) = resolve_point(p, app_hwnd)?;
                inputs.push(mouse_move_input(x, y));
            }
            inputs.push(wheel_input(*notches));
            send(&inputs)
        }
    }
}

/// 解析目标坐标：`win:` 相对窗口左上角（与窗口截图一致），并尽力激活窗口。
fn resolve_point(p: &AimPoint, app_hwnd: HWND) -> Result<(i32, i32), String> {
    if !p.window {
        return Ok((p.x, p.y));
    }
    unsafe {
        if IsIconic(app_hwnd).as_bool() {
            let _ = ShowWindow(app_hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(app_hwnd);
        let mut rect = RECT::default();
        GetWindowRect(app_hwnd, &mut rect).map_err(|e| format!("读取应用窗口位置失败: {}", e))?;
        Ok((rect.left + p.x, rect.top + p.y))
    }
}

/// 屏幕物理坐标 → 绝对移动事件（归一化 0..65535，覆盖整个虚拟屏幕）
fn mouse_move_input(x: i32, y: i32) -> INPUT {
    let (vx, vy, vw, vh) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1),
            GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1),
        )
    };
    let nx = ((x - vx) as i64 * 65535 / (vw as i64 - 1).max(1)) as i32;
    let ny = ((y - vy) as i64 * 65535 / (vh as i64 - 1).max(1)) as i32;
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: nx,
                dy: ny,
                dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
                ..Default::default()
            },
        },
    }
}

fn mouse_button_input(flag: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flag,
                ..Default::default()
            },
        },
    }
}

/// 滚轮事件：一格 = WHEEL_DELTA (120)，正数向上、负数向下
fn wheel_input(notches: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                mouseData: (notches * 120) as u32,
                dwFlags: MOUSEEVENTF_WHEEL,
                ..Default::default()
            },
        },
    }
}

fn key_input(
    vk: u16,
    scan: u16,
    flags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS,
) -> INPUT {
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

/// UI 线程定时器驱动的执行器状态：每次tick弹出就绪动作，Sleep 转为等待截止时刻，
/// 不阻塞消息泵；纯状态逻辑可单测（实际注入由调用方执行 execute_action）。
#[derive(Default)]
pub struct InputSimRunner {
    /// 待执行动作队列
    pub queue: std::collections::VecDeque<InputAction>,
    /// Sleep 动作的等待截止时刻
    pub wait_until: Option<std::time::Instant>,
}

impl InputSimRunner {
    /// 追加一批动作（确认放行后调用）
    pub fn enqueue(&mut self, actions: Vec<InputAction>) {
        self.queue.extend(actions);
    }

    /// 是否仍有在途动作/等待（定时器据此决定停表）
    pub fn is_active(&self) -> bool {
        !self.queue.is_empty() || self.wait_until.is_some()
    }

    /// 返回 now 时刻应立即执行的动作；队首为 Sleep 时设置等待截止并返回 None。
    pub fn next_ready(&mut self, now: std::time::Instant) -> Option<InputAction> {
        if let Some(until) = self.wait_until {
            if now < until {
                return None;
            }
            self.wait_until = None;
        }
        match self.queue.pop_front() {
            Some(InputAction::Sleep(ms)) => {
                self.wait_until = Some(now + std::time::Duration::from_millis(ms));
                None
            }
            other => other,
        }
    }

    /// 清空队列与等待（出错/中断时调用）
    pub fn clear(&mut self) {
        self.queue.clear();
        self.wait_until = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: i32, y: i32) -> AimPoint {
        AimPoint {
            x,
            y,
            window: false,
        }
    }

    fn wpt(x: i32, y: i32) -> AimPoint {
        AimPoint { x, y, window: true }
    }

    #[test]
    fn test_parse_full_script() {
        let script = "# 注释\n\ntype 你好 world\nkey ctrl+shift+a\nclick 100,200\nsleep 500\n";
        let actions = parse_input_script(script).unwrap();
        assert_eq!(
            actions,
            vec![
                InputAction::Type("你好 world".to_string()),
                InputAction::Key {
                    modifiers: vec!["ctrl".into(), "shift".into()],
                    key: "a".into()
                },
                InputAction::Click {
                    at: pt(100, 200),
                    button: MouseButton::Left
                },
                InputAction::Sleep(500),
            ]
        );
    }

    #[test]
    fn test_parse_mouse_actions() {
        let script = "\
move win:10,20
click 5,6 right
dblclick win:1,2
down win:3,4
up
drag win:1,1 win:9,9 middle
scroll -3
scroll win:7,8 2
";
        let actions = parse_input_script(script).unwrap();
        assert_eq!(
            actions,
            vec![
                InputAction::Move(wpt(10, 20)),
                InputAction::Click {
                    at: pt(5, 6),
                    button: MouseButton::Right
                },
                InputAction::DoubleClick {
                    at: wpt(1, 2),
                    button: MouseButton::Left
                },
                InputAction::ButtonDown {
                    at: Some(wpt(3, 4)),
                    button: MouseButton::Left
                },
                InputAction::ButtonUp {
                    button: MouseButton::Left
                },
                InputAction::Drag {
                    from: wpt(1, 1),
                    to: wpt(9, 9),
                    button: MouseButton::Middle
                },
                InputAction::Scroll {
                    at: None,
                    notches: -3
                },
                InputAction::Scroll {
                    at: Some(wpt(7, 8)),
                    notches: 2
                },
            ]
        );
    }

    #[test]
    fn test_parse_single_key() {
        let actions = parse_input_script("key enter").unwrap();
        assert_eq!(
            actions,
            vec![InputAction::Key {
                modifiers: vec![],
                key: "enter".into()
            }]
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
        assert!(parse_input_script("click win:1").is_err());
        assert!(parse_input_script("click 1,2 nosuchbutton").is_err());
        assert!(parse_input_script("move").is_err());
        assert!(parse_input_script("move 1,2 3,4").is_err());
        assert!(parse_input_script("drag 1,2").is_err());
        assert!(parse_input_script("down 1,2 left extra").is_err());
        assert!(parse_input_script("up left extra").is_err());
        assert!(parse_input_script("scroll 0").is_err());
        assert!(parse_input_script("scroll 101").is_err());
        assert!(parse_input_script("scroll 1,2").is_err());
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
        let actions = parse_input_script(
            "type hi\nkey ctrl+s\nclick 1,2\ndblclick win:3,4 right\ndrag win:1,1 win:2,2\nscroll -2\nsleep 50",
        )
        .unwrap();
        let s = action_summary(&actions);
        assert!(s.contains("1. 输入文本 \"hi\"（2 字符）"), "{}", s);
        assert!(s.contains("2. 按键 ctrl+s"), "{}", s);
        assert!(s.contains("3. 左键单击屏幕 (1, 2)"), "{}", s);
        assert!(s.contains("4. 右键双击窗口内 (3, 4)"), "{}", s);
        assert!(
            s.contains("5. 左键拖拽窗口内 (1, 1) → 窗口内 (2, 2)"),
            "{}",
            s
        );
        assert!(s.contains("6. 滚动滚轮 2 格（向下）"), "{}", s);
        assert!(s.contains("7. 等待 50ms"), "{}", s);
    }

    #[test]
    fn test_summary_truncates_long_text() {
        let long = "x".repeat(100);
        let actions = vec![InputAction::Type(long)];
        let s = action_summary(&actions);
        assert!(s.contains("…"), "{}", s);
        assert!(s.contains("100 字符"), "{}", s);
    }

    #[test]
    fn test_runner_order_and_sleep_wait() {
        let mut r = InputSimRunner::default();
        assert!(!r.is_active());
        r.enqueue(parse_input_script("key a\nsleep 500\nkey b").unwrap());
        assert!(r.is_active());
        let t0 = std::time::Instant::now();
        assert_eq!(
            r.next_ready(t0),
            Some(InputAction::Key {
                modifiers: vec![],
                key: "a".into()
            })
        );
        // 队首 Sleep：设置等待截止，未到期返回 None
        assert_eq!(r.next_ready(t0), None);
        assert!(r.wait_until.is_some());
        assert_eq!(
            r.next_ready(t0 + std::time::Duration::from_millis(499)),
            None
        );
        // 到期后弹出后续动作
        assert_eq!(
            r.next_ready(t0 + std::time::Duration::from_millis(500)),
            Some(InputAction::Key {
                modifiers: vec![],
                key: "b".into()
            })
        );
        assert!(!r.is_active());
        assert_eq!(r.next_ready(t0), None);
    }

    #[test]
    fn test_runner_clear_stops_wait() {
        let mut r = InputSimRunner::default();
        r.enqueue(parse_input_script("sleep 1000\nkey a").unwrap());
        let t0 = std::time::Instant::now();
        assert_eq!(r.next_ready(t0), None);
        assert!(r.is_active());
        r.clear();
        assert!(!r.is_active());
        assert_eq!(r.next_ready(t0), None);
    }
}
