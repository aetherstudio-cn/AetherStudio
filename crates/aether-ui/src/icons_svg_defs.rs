//! 全部 UI / 文件类型图标 SVG 定义
//!
//! 来源（全部 ISC / MIT 许可证，商业友好）：
//! - Lucide Icons (ISC)  https://lucide.dev
//! - Devicon (MIT)       https://devicon.dev
//!
//! 所有图标统一为 24x24 viewBox，渲染时按目标尺寸缩放。
//!
//! 数据格式：
//! - 单色 stroke 风格（Lucide 风格）：fill = None，使用当前 brush 描边
//! - 多色 fill 风格（Devicon 风格）：每个 shape 携带 hex 颜色

/// SVG 形状（用于嵌入到 PathGeometry 几何）
#[derive(Clone, Copy)]
pub(crate) enum SvgShape {
    /// SVG path d="..." 字符串；fill = None 表示 stroke 模式，Some(hex) 表示 fill 模式
    Path(&'static str, Option<&'static str>),
    /// 圆形：cx, cy, r, fill
    Circle(f32, f32, f32, Option<&'static str>),
    /// 椭圆：cx, cy, rx, ry, fill
    Ellipse(f32, f32, f32, f32, Option<&'static str>),
    /// 矩形：x, y, w, h, fill, rx（圆角，可选）
    Rect(f32, f32, f32, f32, Option<&'static str>, Option<f32>),
    /// 直线：x1, y1, x2, y2
    Line(f32, f32, f32, f32),
}

/// 一个完整的 SVG 图标定义
#[derive(Clone, Copy)]
pub(crate) struct SvgDef {
    /// 视图框 (x, y, w, h)，通常 (0, 0, 24, 24)
    pub viewbox: (f32, f32, f32, f32),
    /// 该图标包含的形状
    pub shapes: &'static [SvgShape],
}

// ===========================================================================
// UI 图标（Lucide 风格，stroke 模式 — fill = None）
// ===========================================================================

/// Lucide "folder-open" - 打开的文件夹
const UI_FOLDER_OPEN: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2", None),
    ],
};

/// Lucide "folder" - 关闭的文件夹
const UI_FOLDER: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z", None),
    ],
};

/// Lucide "file-plus" - 新建文件
const UI_NEW_FILE: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z", None),
        SvgShape::Path("M14 2v5a1 1 0 0 0 1 1h5", None),
        SvgShape::Path("M9 15h6", None),
        SvgShape::Path("M12 18v-6", None),
    ],
};

/// Lucide "file" - 普通文件
const UI_FILE: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z", None),
        SvgShape::Path("M14 2v5a1 1 0 0 0 1 1h5", None),
    ],
};

/// Lucide "save" - 保存（软盘）
const UI_SAVE: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M15.2 3a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z", None),
        SvgShape::Path("M17 21v-7a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v7", None),
        SvgShape::Path("M7 3v4a1 1 0 0 0 1 1h7", None),
    ],
};

/// Lucide "copy" - 复制（两个重叠矩形）
const UI_COPY: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Rect(8.0, 8.0, 14.0, 14.0, None, Some(2.0)),
        SvgShape::Path(
            "M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2",
            None,
        ),
    ],
};

/// Lucide "scissors" - 剪切
const UI_CUT: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Circle(6.0, 6.0, 3.0, None),
        SvgShape::Path("M8.12 8.12 12 12", None),
        SvgShape::Path("M20 4 8.12 15.88", None),
        SvgShape::Circle(6.0, 18.0, 3.0, None),
        SvgShape::Path("M14.8 14.8 20 20", None),
    ],
};

/// Lucide "clipboard-paste" - 粘贴
const UI_PASTE: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M11 14h10", None),
        SvgShape::Path("M16 4h2a2 2 0 0 1 2 2v1.344", None),
        SvgShape::Path("m17 18 4-4-4-4", None),
        SvgShape::Path(
            "M8 4H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 1.793-1.113",
            None,
        ),
        SvgShape::Rect(8.0, 2.0, 8.0, 4.0, None, Some(1.0)),
    ],
};

/// Lucide "list-checks" - 全选
const UI_SELECT_ALL: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M13 5h8", None),
        SvgShape::Path("M13 12h8", None),
        SvgShape::Path("M13 19h8", None),
        SvgShape::Path("m3 17 2 2 4-4", None),
        SvgShape::Path("m3 7 2 2 4-4", None),
    ],
};

/// Lucide "search" - 查找
const UI_SEARCH: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("m21 21-4.34-4.34", None),
        SvgShape::Circle(11.0, 11.0, 8.0, None),
    ],
};

/// Lucide "replace" - 替换
const UI_REPLACE: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M14 4a1 1 0 0 1 1-1", None),
        SvgShape::Path("M15 10a1 1 0 0 1-1-1", None),
        SvgShape::Path("M21 4a1 1 0 0 0-1-1", None),
        SvgShape::Path("M21 9a1 1 0 0 1-1 1", None),
        SvgShape::Path("m3 7 3 3 3-3", None),
        SvgShape::Path("M6 10V5a2 2 0 0 1 2-2h2", None),
        SvgShape::Rect(3.0, 14.0, 7.0, 7.0, None, Some(1.0)),
    ],
};

/// Lucide "undo-2" - 撤销
const UI_UNDO: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M9 14 4 9l5-5", None),
        SvgShape::Path(
            "M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5a5.5 5.5 0 0 1-5.5 5.5H11",
            None,
        ),
    ],
};

/// Lucide "redo-2" - 重做
const UI_REDO: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("m15 14 5-5-5-5", None),
        SvgShape::Path(
            "M20 9H9.5A5.5 5.5 0 0 0 4 14.5A5.5 5.5 0 0 0 9.5 20H13",
            None,
        ),
    ],
};

/// Lucide "panel-left" - 侧边栏
const UI_SIDEBAR: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Rect(3.0, 3.0, 18.0, 18.0, None, Some(2.0)),
        SvgShape::Path("M9 3v18", None),
    ],
};

/// Lucide "panel-left-open" - 左侧面板
const UI_PANEL_LEFT: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Rect(3.0, 3.0, 18.0, 18.0, None, Some(2.0)),
        SvgShape::Path("M9 3v18", None),
        SvgShape::Path("m14 9 3 3-3 3", None),
    ],
};

/// Lucide "panel-bottom" - 底部面板
const UI_PANEL_BOTTOM: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Rect(3.0, 3.0, 18.0, 18.0, None, Some(2.0)),
        SvgShape::Path("M3 15h18", None),
    ],
};

/// Lucide "panel-right" - 右侧面板
const UI_PANEL_RIGHT: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Rect(3.0, 3.0, 18.0, 18.0, None, Some(2.0)),
        SvgShape::Path("M15 3v18", None),
    ],
};

/// Lucide "hash" - # 符号
const UI_HASH: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Line(4.0, 9.0, 20.0, 9.0),
        SvgShape::Line(4.0, 15.0, 20.0, 15.0),
        SvgShape::Line(10.0, 3.0, 8.0, 21.0),
        SvgShape::Line(16.0, 3.0, 14.0, 21.0),
    ],
};

/// Lucide "play" - 播放
const UI_PLAY: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[SvgShape::Path(
        "M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z",
        None,
    )],
};

/// Lucide "bug" - 调试
const UI_BUG: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M12 20v-9", None),
        SvgShape::Path(
            "M14 7a4 4 0 0 1 4 4v3a6 6 0 0 1-12 0v-3a4 4 0 0 1 4-4z",
            None,
        ),
        SvgShape::Path("M14.12 3.88 16 2", None),
        SvgShape::Path("M21 21a4 4 0 0 0-3.81-4", None),
        SvgShape::Path("M21 5a4 4 0 0 1-3.55 3.97", None),
        SvgShape::Path("M22 13h-4", None),
        SvgShape::Path("M3 21a4 4 0 0 1 3.81-4", None),
        SvgShape::Path("M3 5a4 4 0 0 0 3.55 3.97", None),
        SvgShape::Path("M6 13H2", None),
        SvgShape::Path("m8 2 1.88 1.88", None),
        SvgShape::Path("M9 7.13V6a3 3 0 1 1 6 0v1.13", None),
    ],
};

/// Lucide "terminal" - 终端
const UI_TERMINAL: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M12 19h8", None),
        SvgShape::Path("m4 17 6-6-6-6", None),
    ],
};

/// Git 分支 — 菱形轮廓 + 分支线（镂空描边风格）
const UI_GIT_BRANCH: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        // 菱形轮廓
        SvgShape::Path("M12 1.5 L22.5 12 L12 22.5 L1.5 12 Z", None),
        // 主干线
        SvgShape::Path("M12 19.5 L12 13.5", None),
        // 分支线
        SvgShape::Path("M12 13.5 L17.5 8", None),
        // 底部圆
        SvgShape::Circle(12.0, 19.5, 1.8, None),
        // 中心圆
        SvgShape::Circle(12.0, 13.5, 1.5, None),
        // 右上圆
        SvgShape::Circle(17.5, 8.0, 1.8, None),
    ],
};

/// Lucide "circle-x" - 错误
const UI_ERROR: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Circle(12.0, 12.0, 10.0, None),
        SvgShape::Path("m15 9-6 6", None),
        SvgShape::Path("m9 9 6 6", None),
    ],
};

/// Lucide "triangle-alert" - 警告
const UI_WARNING: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path(
            "m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3",
            None,
        ),
        SvgShape::Path("M12 9v4", None),
        SvgShape::Path("M12 17h.01", None),
    ],
};

/// Lucide "info" - 信息
const UI_INFO: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Circle(12.0, 12.0, 10.0, None),
        SvgShape::Path("M12 16v-4", None),
        SvgShape::Path("M12 8h.01", None),
    ],
};

/// Lucide "log-out" - 退出
const UI_EXIT: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("m16 17 5-5-5-5", None),
        SvgShape::Path("M21 12H9", None),
        SvgShape::Path("M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4", None),
    ],
};

/// Lucide "arrow-left" - 返回
const UI_BACK: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("m12 19-7-7 7-7", None),
        SvgShape::Path("M19 12H5", None),
    ],
};

/// Lucide "arrow-right" - 前进
const UI_FORWARD: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M5 12h14", None),
        SvgShape::Path("m12 5 7 7-7 7", None),
    ],
};

/// Lucide "settings" - 设置（齿轮）
const UI_SETTINGS: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915", None),
        SvgShape::Circle(12.0, 12.0, 3.0, None),
    ],
};

/// Lucide "user" - 用户
const UI_USER: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2", None),
        SvgShape::Circle(12.0, 7.0, 4.0, None),
    ],
};

/// Lucide "x" - 关闭
const UI_CLOSE: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M18 6 6 18", None),
        SvgShape::Path("m6 6 12 12", None),
    ],
};

/// Lucide "plus" - 加号
const UI_PLUS: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M5 12h14", None),
        SvgShape::Path("M12 5v14", None),
    ],
};

/// Lucide "chevron-left" - 左折角
const UI_CHEVRON_LEFT: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[SvgShape::Path("m15 18-6-6 6-6", None)],
};

/// Lucide "chevron-right" - 右折角
const UI_CHEVRON_RIGHT: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[SvgShape::Path("m9 18 6-6-6-6", None)],
};

/// Lucide "chevron-down" - 下折角（文件树展开态）
const UI_CHEVRON_DOWN: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[SvgShape::Path("m6 9 6 6 6-6", None)],
};

/// Lucide "bot" - 机器人（AI 助手）
const UI_BOT: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M12 8V4H8", None),
        SvgShape::Rect(4.0, 8.0, 16.0, 12.0, None, Some(2.0)),
        SvgShape::Path("M2 14h2", None),
        SvgShape::Path("M20 14h2", None),
        SvgShape::Path("M15 13v2", None),
        SvgShape::Path("M9 13v2", None),
    ],
};

/// SSH/远程链接 — 显示器 + 插头（远程连接语义）
const UI_SSH: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        // 显示器轮廓
        SvgShape::Rect(2.0, 4.0, 20.0, 13.0, None, Some(2.0)),
        // 屏幕内的终端提示符 >_
        SvgShape::Path("m7 9 3 3-3 3", None),
        SvgShape::Path("M12 15h4", None),
        // 底座
        SvgShape::Path("M8 21h8", None),
        SvgShape::Path("M12 17v4", None),
    ],
};

/// 克隆仓库 — 双圆 + 下载箭头（克隆语义）
const UI_CLONE: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        // 源仓库圆
        SvgShape::Circle(7.0, 5.0, 3.0, None),
        // 目标仓库圆
        SvgShape::Circle(17.0, 19.0, 3.0, None),
        // 连接曲线
        SvgShape::Path("M7 8v3a5 5 0 0 0 5 5h5", None),
        // 下载箭头
        SvgShape::Path("M17 13v3", None),
        SvgShape::Path("m15 14 2 2 2-2", None),
    ],
};

/// Lucide "file-search" - 转到文件
const UI_GOTO_FILE: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z", None),
        SvgShape::Path("M14 2v5a1 1 0 0 0 1 1h5", None),
        SvgShape::Circle(11.5, 14.5, 2.5, None),
        SvgShape::Path("M13.3 16.3 15 18", None),
    ],
};

/// 自定义 - 羊脸（Lucide 无等价物，使用 SVG 路径近似）
const UI_EMOJI_SHEEP: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        // 头部圆
        SvgShape::Circle(12.0, 13.0, 7.0, None),
        // 左耳
        SvgShape::Circle(4.0, 11.0, 2.0, None),
        // 右耳
        SvgShape::Circle(20.0, 11.0, 2.0, None),
        // 左眼
        SvgShape::Circle(9.0, 11.0, 0.9, Some("#1F2328")),
        // 右眼
        SvgShape::Circle(15.0, 11.0, 0.9, Some("#1F2328")),
        // 顶部绒毛弧
        SvgShape::Path("M10 6 Q12 3 14 6", None),
    ],
};

// ===========================================================================
// 文件类型图标（极简风格 — 透明背景 + 品牌色）
// viewBox: 16x16，适配文件树小尺寸显示
// ===========================================================================

/// 通用文本文件图标（三条横线）
const FILE_TEXT: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        SvgShape::Line(4.0, 5.0, 12.0, 5.0),
        SvgShape::Line(4.0, 8.0, 12.0, 8.0),
        SvgShape::Line(4.0, 11.0, 10.0, 11.0),
    ],
};

/// Python 文件图标（双蛇互锁简化）
const FILE_PYTHON: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        SvgShape::Path("M8 3c-1.5 0-2.7 0.7-2.7 2v1.3h2.7v0.7H4.7c-1.1 0-2 0.9-2 2s0.9 2 2 2H6v-1.3c0-0.4 0.3-0.7 0.7-0.7h2.7c0.4 0 0.7-0.3 0.7-0.7V5c0-1.3-1.2-2-2.7-2z", None),
        SvgShape::Circle(6.7, 5.0, 0.4, Some("#3776AB")),
        SvgShape::Path("M8 13c1.5 0 2.7-0.7 2.7-2V9.7H8V9h3.3c1.1 0 2-0.9 2-2s-0.9-2-2-2H10v1.3c0 0.4-0.3 0.7-0.7 0.7H6.7c-0.4 0-0.7 0.3-0.7 0.7v2.7c0 1.3 1.2 2 2.7 2z", None),
        SvgShape::Circle(9.3, 11.0, 0.4, Some("#3776AB")),
    ],
};

/// Java 文件图标（咖啡杯）
const FILE_JAVA: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        SvgShape::Path("M10.5 6h0.6a2 2 0 1 1 0 4h-0.6", None),
        SvgShape::Path("M3.5 6h7v4.5a2 2 0 0 1-2 2h-3a2 2 0 0 1-2-2V6z", None),
        SvgShape::Line(4.5, 3.5, 4.5, 4.8),
        SvgShape::Line(7.0, 3.5, 7.0, 4.8),
        SvgShape::Line(9.5, 3.5, 9.5, 4.8),
    ],
};

/// C 文件图标
const FILE_C: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // C 字符（马蹄形）
        SvgShape::Path("M11 5.5 Q9.5 4 6.5 4 Q4 4 4 8 Q4 12 6.5 12 Q9.5 12 11 10.5 L10 9.5 Q8.8 11 7 11 Q5.5 11 5.5 8 Q5.5 5 7 5 Q8.8 5 10 6.5 Z", Some("#00599C")),
    ],
};

/// C++ 文件图标
const FILE_CPP: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // C 字符
        SvgShape::Path("M9.5 5.5 Q8 4 5.5 4 Q3 4 3 8 Q3 12 5.5 12 Q8 12 9.5 10.5 L8.5 9.5 Q7.3 11 6 11 Q4.5 11 4.5 8 Q4.5 5 6 5 Q7.3 5 8.5 6.5 Z", Some("#00599C")),
        // 第一个 +（横竖两条窄矩形）
        SvgShape::Rect(10.5, 6.7, 3.0, 0.7, Some("#00599C"), None),
        SvgShape::Rect(11.7, 5.5, 0.7, 3.0, Some("#00599C"), None),
        // 第二个 +（横竖两条窄矩形）
        SvgShape::Rect(10.5, 9.7, 3.0, 0.7, Some("#00599C"), None),
        SvgShape::Rect(11.7, 8.5, 0.7, 3.0, Some("#00599C"), None),
    ],
};

/// C# 文件图标
const FILE_CSHARP: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // C 字符
        SvgShape::Path("M9 5.5 Q7.5 4 5 4 Q2.5 4 2.5 8 Q2.5 12 5 12 Q7.5 12 9 10.5 L8 9.5 Q6.8 11 5.5 11 Q4 11 4 8 Q4 5 5.5 5 Q6.8 5 8 6.5 Z", Some("#239120")),
        // # 符号
        SvgShape::Path("M10.5 4 L11 4 L10.5 12 L10 12 Z", Some("#239120")),
        SvgShape::Path("M12.5 4 L13 4 L12.5 12 L12 12 Z", Some("#239120")),
        SvgShape::Path("M9.5 6.5 L13.5 6.5 L13.5 7 L9.5 7 Z", Some("#239120")),
        SvgShape::Path("M9.5 9.5 L13.5 9.5 L13.5 10 L9.5 10 Z", Some("#239120")),
    ],
};

/// Go 文件图标
const FILE_GO: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        SvgShape::Line(3.5, 7.0, 4.5, 7.0),
        SvgShape::Line(3.5, 8.0, 5.0, 8.0),
        SvgShape::Line(3.5, 9.0, 4.5, 9.0),
        // Go 字符
        SvgShape::Path("M10 5.5 Q9 4.5 7.5 4.5 Q6 4.5 6 8 Q6 11.5 7.5 11.5 Q9 11.5 10 10.5 L10 8.5 L8 8.5 L8 9.5 L9 9.5 L9 10 Q8.5 10.5 7.5 10.5 Q7 10.5 7 8 Q7 5.5 7.5 5.5 Q8.5 5.5 9 6.5 Z", Some("#00ADD8")),
    ],
};

/// Rust 文件图标（齿轮蟹）
const FILE_RUST: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        SvgShape::Ellipse(8.0, 9.0, 4.0, 3.3, None),
        SvgShape::Path("M4 7.5l-1.3-1.3", None),
        SvgShape::Circle(2.7, 6.2, 0.7, None),
        SvgShape::Path("M12 7.5l1.3-1.3", None),
        SvgShape::Circle(13.3, 6.2, 0.7, None),
        SvgShape::Circle(6.3, 8.0, 0.6, Some("#CE422B")),
        SvgShape::Circle(9.7, 8.0, 0.6, Some("#CE422B")),
        SvgShape::Path("M6.7 10c0.7 0.5 2 0.5 2.7 0", None),
        SvgShape::Path("M4.7 12l-0.7 1.3", None),
        SvgShape::Path("M11.3 12l0.7 1.3", None),
    ],
};

/// JavaScript 文件图标
const FILE_JS: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // JS 字符
        SvgShape::Path("M5 5 L6.5 5 L6.5 10.5 Q6.5 11.5 5.5 11.5 Q4.5 11.5 4 10.5 L3 11 Q3.8 12.5 5.5 12.5 Q7.5 12.5 7.5 10.5 L7.5 5 Z", Some("#E8C300")),
        SvgShape::Path("M9 11 L10 10.5 Q10.5 11.5 11.5 11.5 Q12.5 11.5 12.5 10.5 Q12.5 9.5 11 9 Q9 8.5 9 7 Q9 5.5 10.5 5.5 Q11.8 5.5 12.5 6.5 L11.5 7 Q11 6.5 10.5 6.5 Q10 6.5 10 7 Q10 7.7 11.5 8 Q13 8.5 13 10 Q13 12 11.5 12 Q10 12 9 11 Z", Some("#E8C300")),
    ],
};

/// TypeScript 文件图标
const FILE_TS: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // TS 字符
        SvgShape::Path("M3 5 L9.5 5 L9.5 6.5 L7.25 6.5 L7.25 12 L5.75 12 L5.75 6.5 L3 6.5 Z", Some("#3178C6")),
        SvgShape::Path("M10.5 10.3 L11.7 9.7 Q12 10.5 12.8 10.5 Q13.6 10.5 13.6 9.7 Q13.6 8.9 12 8.5 Q10.3 8.1 10.3 6.7 Q10.3 5.3 11.7 5.3 Q12.9 5.3 13.6 6.3 L12.5 7 Q12.2 6.5 11.7 6.5 Q11.2 6.5 11.2 7 Q11.2 7.6 12.8 8 Q14.5 8.4 14.5 9.9 Q14.5 11.5 12.9 11.5 Q11.4 11.5 10.5 10.3 Z", Some("#3178C6")),
    ],
};

/// HTML 文件图标（尖括号）
const FILE_HTML: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        SvgShape::Path("M5 5 L2 8 L5 11", None),
        SvgShape::Path("M11 5 L14 8 L11 11", None),
    ],
};

/// CSS 文件图标（# 符号）
const FILE_CSS: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        SvgShape::Line(4.0, 5.0, 12.0, 5.0),
        SvgShape::Line(4.0, 11.0, 12.0, 11.0),
        SvgShape::Line(6.0, 3.0, 5.0, 13.0),
        SvgShape::Line(11.0, 3.0, 10.0, 13.0),
    ],
};

/// JSON 文件图标（大括号）
const FILE_JSON: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        SvgShape::Path(
            "M5 4c-1.5 0-2 1-2 2v2c0 1-0.5 1.5-1 2 0.5 0.5 1 1 1 2v2c0 1 0.5 2 2 2",
            None,
        ),
        SvgShape::Path(
            "M11 4c1.5 0 2 1 2 2v2c0 1 0.5 1.5 1 2-0.5 0.5-1 1-1 2v2c0 1-0.5 2-2 2",
            None,
        ),
    ],
};

/// YAML 文件图标
const FILE_YAML: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // Yml 文字简化为 Y 形路径
        SvgShape::Path(
            "M4 4 L5.5 4 L8 7.5 L10.5 4 L12 4 L8.5 9 L8.5 12 L7.5 12 L7.5 9 Z",
            Some("#C72C48"),
        ),
    ],
};

/// TOML 文件图标
const FILE_TOML: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // T 字符
        SvgShape::Path(
            "M3 4 L13 4 L13 5.5 L9.5 5.5 L9.5 12 L8 12 L8 5.5 L3 5.5 Z",
            Some("#D24939"),
        ),
    ],
};

/// Markdown 文件图标
const FILE_MARKDOWN: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // Md 简化为 M 形路径
        SvgShape::Path("M3 12 L3 4 L4.5 4 L8 9.5 L11.5 4 L13 4 L13 12 L11.5 12 L11.5 6.5 L8 12 L4.5 6.5 L4.5 12 Z", Some("#FFFFFF")),
    ],
};

/// Shell 脚本文件图标
const FILE_SHELL: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // sh 简化为 >_ 提示符
        SvgShape::Path("M4 5 L7 8 L4 11 L3 10 L5 8 L3 6 Z", Some("#4EAA25")),
        SvgShape::Path("M8 11 L13 11 L13 12 L8 12 Z", Some("#4EAA25")),
    ],
};

/// SQL 文件图标（数据库圆柱）
const FILE_SQL: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        SvgShape::Path(
            "M4 6 Q4 4.5 8 4.5 Q12 4.5 12 6 L12 10 Q12 11.5 8 11.5 Q4 11.5 4 10 Z",
            Some("#E38C00"),
        ),
        SvgShape::Path("M4 6 Q4 7.5 8 7.5 Q12 7.5 12 6", Some("#E38C00")),
        SvgShape::Path("M4 8 Q4 9.5 8 9.5 Q12 9.5 12 8", Some("#E38C00")),
    ],
};

/// Ruby 文件图标（红宝石）
const FILE_RUBY: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[SvgShape::Path(
        "M4.5 3.5h7l1.5 2.7-5 4.8-5-4.8 1.5-2.7z",
        None,
    )],
};

/// PHP 文件图标（大象简化）
const FILE_PHP: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        SvgShape::Path("M5 6.5c0-0.8 0.7-1.5 1.5-1.5h5c1.4 0 2.5 1.1 2.5 2.5v2c0 0.8-0.7 1.5-1.5 1.5h-6c-0.8 0-1.5-0.7-1.5-1.5v-3z", None),
        SvgShape::Path("M5 7c-0.8 0-1.5 0.7-1.5 1.5v2.5c0 0.3 0.2 0.5 0.5 0.5s0.5-0.2 0.5-0.5v-2", None),
        SvgShape::Ellipse(6.5, 6.0, 1.0, 1.2, None),
        SvgShape::Path("M12.5 8.5l1-0.3c0.3 0.2 0.3 0.6 0 0.8l-1-0.3", None),
        SvgShape::Line(6.0, 11.0, 6.0, 12.5),
        SvgShape::Line(7.5, 11.0, 7.5, 12.5),
        SvgShape::Line(10.0, 11.0, 10.0, 12.5),
        SvgShape::Line(11.5, 11.0, 11.5, 12.5),
        SvgShape::Circle(6.2, 7.5, 0.4, Some("#777BB4")),
    ],
};

/// Lua 文件图标（月牙）
const FILE_LUA: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[SvgShape::Path(
        "M7 3.5a4.5 4.5 0 1 0 0 9 3.5 3.5 0 0 1 0-7 3.5 3.5 0 0 1 0-2z",
        None,
    )],
};

/// Swift 文件图标（鸟形）
const FILE_SWIFT: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        SvgShape::Path("M8 1.5c1.3 3.3 4.7 4.7 6.7 5.3-2 0.7-4.7 2.7-6.7 8-0.7-3.3-4-5.3-6.7-6 2-0.7 5.3-2 6.7-7.3z", None),
    ],
};

/// Kotlin 文件图标
const FILE_KOTLIN: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // Kt 简化为 K 形路径
        SvgShape::Path(
            "M4 4 L5.5 4 L5.5 7 L9 4 L11 4 L7 8 L11 12 L9 12 L5.5 9 L5.5 12 L4 12 Z",
            Some("#7F52FF"),
        ),
    ],
};

/// Docker 文件图标（鲸鱼）
const FILE_DOCKER: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        SvgShape::Path(
            "M2 10c1-2 3.5-4 7-4s5.5 1.5 6 3.5c-0.8 1.2-2.8 2-5 2H3.5c-1 0-1.5-0.7-1.5-1.5z",
            None,
        ),
        SvgShape::Path("M2 10l-0.8-1.2", None),
        SvgShape::Path("M2 10l-0.8 1.2", None),
        SvgShape::Rect(6.0, 4.5, 1.3, 1.3, None, Some(0.2)),
        SvgShape::Rect(8.0, 4.5, 1.3, 1.3, None, Some(0.2)),
        SvgShape::Rect(10.0, 4.5, 1.3, 1.3, None, Some(0.2)),
        SvgShape::Circle(11.5, 9.2, 0.5, Some("#2496ED")),
    ],
};

// ===========================================================================
// 新增语言图标
// ===========================================================================

/// Dart 文件图标（飞镖）
const FILE_DART: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        SvgShape::Path("M12 8l-6-3.5 1.5 3.5-1.5 3.5L12 8z", None),
        SvgShape::Line(6.0, 8.0, 4.0, 8.0),
    ],
};

/// Haskell 文件图标（λ 符号）
const FILE_HASKELL: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[SvgShape::Path("M5 11l3-6 2 3 3-3", None)],
};

/// Vue 文件图标
const FILE_VUE: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // V 形路径
        SvgShape::Path("M3 4 L8 12 L13 4 L11 4 L8 9 L5 4 Z", Some("#359969")),
    ],
};

/// React 文件图标
const FILE_REACT: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // 原子轨道简化
        SvgShape::Ellipse(8.0, 8.0, 6.0, 2.5, None),
        SvgShape::Ellipse(8.0, 8.0, 6.0, 2.5, None),
        SvgShape::Circle(8.0, 8.0, 1.0, Some("#08A4B9")),
    ],
};

/// Svelte 文件图标
const FILE_SVELTE: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // S 形路径
        SvgShape::Path("M11 5.5 Q10 4 8 4 Q5.5 4 5.5 6.5 Q5.5 8.5 8 9 Q10.5 9.5 10.5 11.5 Q10.5 13 8 13 Q6 13 5 11.5 L6 10.5 Q6.8 12 8 12 Q9.5 12 9.5 11 Q9.5 10 8 9.5 Q5.5 9 5.5 6.5 Q5.5 4 8 4 Q10 4 11 5.5 Z", Some("#FF3E00")),
    ],
};

/// Zig 文件图标
const FILE_ZIG: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // Z 形路径
        SvgShape::Path(
            "M4 4 L12 4 L12 5.5 L6 10.5 L12 10.5 L12 12 L4 12 L4 10.5 L10 5.5 L4 5.5 Z",
            Some("#D48806"),
        ),
    ],
};

/// R 文件图标
const FILE_R: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // R 字符路径
        SvgShape::Path("M5 4 L5 12 L6.5 12 L6.5 9 L8 9 L10 12 L11.5 12 L9.5 8.5 Q11 8 11 6.5 Q11 4 8.5 4 Z M6.5 5.5 L8.5 5.5 Q9.5 5.5 9.5 6.5 Q9.5 7.5 8.5 7.5 L6.5 7.5 Z", Some("#165CAA")),
    ],
};

/// Scala 文件图标
const FILE_SCALA: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // Sc 简化为 S 形路径
        SvgShape::Path("M11 5.5 Q10 4 8 4 Q5.5 4 5.5 6.5 Q5.5 8.5 8 9 Q10.5 9.5 10.5 11.5 Q10.5 13 8 13 Q6 13 5 11.5 L6 10.5 Q6.8 12 8 12 Q9.5 12 9.5 11 Q9.5 10 8 9.5 Q5.5 9 5.5 6.5 Q5.5 4 8 4 Q10 4 11 5.5 Z", Some("#DC322F")),
    ],
};

/// Perl 文件图标
const FILE_PERL: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // P 字符路径
        SvgShape::Path("M5 4 L5 12 L6.5 12 L6.5 9.5 L8.5 9.5 Q11 9.5 11 7 Q11 4 8.5 4 Z M6.5 5.5 L8.5 5.5 Q9.5 5.5 9.5 7 Q9.5 8.5 8.5 8.5 L6.5 8.5 Z", Some("#394578")),
    ],
};

/// Clojure 文件图标
const FILE_CLOJURE: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // Cl 简化为 C 形路径
        SvgShape::Path("M11 5.5 Q9.5 4 6.5 4 Q4 4 4 8 Q4 12 6.5 12 Q9.5 12 11 10.5 L10 9.5 Q8.8 11 7 11 Q5.5 11 5.5 8 Q5.5 5 7 5 Q8.8 5 10 6.5 Z", Some("#5881D8")),
    ],
};

/// Elixir 文件图标
const FILE_ELIXIR: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // Ex 简化为 E 形路径
        SvgShape::Path("M5 4 L5 12 L11 12 L11 10.5 L6.5 10.5 L6.5 8.5 L10 8.5 L10 7 L6.5 7 L6.5 5.5 L11 5.5 L11 4 Z", Some("#6E4A7E")),
    ],
};

/// Erlang 文件图标
const FILE_ERLANG: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // Er 简化为 E 形路径
        SvgShape::Path("M5 4 L5 12 L11 12 L11 10.5 L6.5 10.5 L6.5 8.5 L10 8.5 L10 7 L6.5 7 L6.5 5.5 L11 5.5 L11 4 Z", Some("#B93821")),
    ],
};

/// Julia 文件图标
const FILE_JULIA: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // Jl 简化为 J 形路径
        SvgShape::Path(
            "M9 4 L9 10 Q9 12 7 12 Q5 12 4.5 10.5 L6 10 Q6.5 11 7.5 11 Q8 11 8 10 L8 4 Z",
            Some("#9558B2"),
        ),
    ],
};

/// F# 文件图标
const FILE_FSHARP: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 16.0, 16.0),
    shapes: &[
        // F 字符
        SvgShape::Path(
            "M5 4 L5 12 L6.5 12 L6.5 8.5 L10 8.5 L10 7 L6.5 7 L6.5 5.5 L11 5.5 L11 4 Z",
            Some("#378BBA"),
        ),
        // # 符号
        SvgShape::Path("M12 4 L12.5 4 L12 12 L11.5 12 Z", Some("#378BBA")),
        SvgShape::Path("M13.5 4 L14 4 L13.5 12 L13 12 Z", Some("#378BBA")),
        SvgShape::Path("M11.5 6.5 L14.5 6.5 L14.5 7 L11.5 7 Z", Some("#378BBA")),
        SvgShape::Path("M11.5 9.5 L14.5 9.5 L14.5 10 L11.5 10 Z", Some("#378BBA")),
    ],
};

// ===========================================================================
// AI 面板输入框工具栏图标（Lucide 风格，stroke 模式）
// ===========================================================================

/// Lucide "send" - 发送（纸飞机/箭头）
const UI_SEND: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M22 2L11 13", None),
        SvgShape::Path("M22 2l-7 20-4-9-9-4 20-7z", None),
    ],
};

/// Lucide "mic" - 麦克风
const UI_MIC: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M12 19v3", None),
        SvgShape::Path("M8 12v1a4 4 0 0 0 8 0v-1", None),
        SvgShape::Path("M12 19c-2.8 0-5-2.2-5-5v-4", None),
        SvgShape::Path("M17 8v4a5 5 0 0 1-10 0V8", None),
        SvgShape::Path("M12 1a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V4a3 3 0 0 1 3-3z", None),
    ],
};

/// Lucide "sparkles" - 星星/闪光
const UI_SPARKLES: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path(
            "M12 2l1.5 4.5L18 8l-4.5 1.5L12 14l-1.5-4.5L6 8l4.5-1.5z",
            None,
        ),
        SvgShape::Path(
            "M18 12l.8 2.4L21 15l-2.4.8L18 18l-.8-2.4L15 15l2.4-.8z",
            None,
        ),
    ],
};

/// Lucide "list" - 菜单/列表
const UI_LIST: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M8 6h13", None),
        SvgShape::Path("M8 12h13", None),
        SvgShape::Path("M8 18h13", None),
        SvgShape::Path("M3 6h.01", None),
        SvgShape::Path("M3 12h.01", None),
        SvgShape::Path("M3 18h.01", None),
    ],
};

/// Lucide "clock" - 时钟（历史记录）
const UI_CLOCK: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Circle(12.0, 12.0, 10.0, None),
        SvgShape::Path("M12 6v6l4 2", None),
    ],
};

/// Lucide "eye" - 眼睛（预览）
const UI_EYE: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7z", None),
        SvgShape::Circle(12.0, 12.0, 3.0, None),
    ],
};

/// Lucide "pencil" - 铅笔（编辑）
const UI_PENCIL: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[SvgShape::Path(
        "M17 3a2.83 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z",
        None,
    )],
};

/// Lucide "trash-2" - 垃圾桶（删除）
const UI_TRASH: SvgDef = SvgDef {
    viewbox: (0.0, 0.0, 24.0, 24.0),
    shapes: &[
        SvgShape::Path("M3 6h18", None),
        SvgShape::Path("M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6", None),
        SvgShape::Path("M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2", None),
        SvgShape::Line(10.0, 11.0, 10.0, 17.0),
        SvgShape::Line(14.0, 11.0, 14.0, 17.0),
    ],
};

// ===========================================================================
// 图标定义表（按 IconKind 索引）
// ===========================================================================

/// 全图标定义表，索引与 IconKind::ALL 一致
pub(crate) const SVG_DEFS: &[SvgDef] = &[
    /*  0 OpenFolder    */ UI_FOLDER_OPEN,
    /*  1 NewFile       */ UI_NEW_FILE,
    /*  2 Clone         */ UI_CLONE,
    /*  3 Ssh           */ UI_SSH,
    /*  4 Folder        */ UI_FOLDER,
    /*  5 File          */ UI_FILE,
    /*  6 Save          */ UI_SAVE,
    /*  7 Undo          */ UI_UNDO,
    /*  8 Redo          */ UI_REDO,
    /*  9 Cut           */ UI_CUT,
    /* 10 Copy          */ UI_COPY,
    /* 11 Paste         */ UI_PASTE,
    /* 12 SelectAll     */ UI_SELECT_ALL,
    /* 13 Search        */ UI_SEARCH,
    /* 14 Replace       */ UI_REPLACE,
    /* 15 Sidebar       */ UI_SIDEBAR,
    /* 16 PanelLeft     */ UI_PANEL_LEFT,
    /* 17 PanelBottom   */ UI_PANEL_BOTTOM,
    /* 18 GotoFile      */ UI_GOTO_FILE,
    /* 19 Hash          */ UI_HASH,
    /* 20 Play          */ UI_PLAY,
    /* 21 Bug           */ UI_BUG,
    /* 22 Terminal      */ UI_TERMINAL,
    /* 23 GitBranch     */ UI_GIT_BRANCH,
    /* 24 Error         */ UI_ERROR,
    /* 25 Warning       */ UI_WARNING,
    /* 26 Info          */ UI_INFO,
    /* 27 Exit          */ UI_EXIT,
    /* 28 Back          */ UI_BACK,
    /* 29 Forward       */ UI_FORWARD,
    /* 30 Settings      */ UI_SETTINGS,
    /* 31 User          */ UI_USER,
    /* 32 Close         */ UI_CLOSE,
    /* 33 Plus          */ UI_PLUS,
    /* 34 ChevronLeft   */ UI_CHEVRON_LEFT,
    /* 35 ChevronRight  */ UI_CHEVRON_RIGHT,
    /* 36 EmojiSheep    */ UI_EMOJI_SHEEP,
    /* 37 Bot           */ UI_BOT,
    /* 38 Send          */ UI_SEND,
    /* 39 Mic           */ UI_MIC,
    /* 40 Sparkles      */ UI_SPARKLES,
    /* 41 List          */ UI_LIST,
    /* 42 FilePython    */ FILE_PYTHON,
    /* 43 FileJava      */ FILE_JAVA,
    /* 44 FileText      */ FILE_TEXT,
    /* 45 FileC         */ FILE_C,
    /* 46 FileCpp       */ FILE_CPP,
    /* 47 FileCSharp    */ FILE_CSHARP,
    /* 48 FileGo        */ FILE_GO,
    /* 49 FileRust      */ FILE_RUST,
    /* 50 FileJs        */ FILE_JS,
    /* 51 FileTs        */ FILE_TS,
    /* 52 FileHtml      */ FILE_HTML,
    /* 53 FileCss       */ FILE_CSS,
    /* 54 FileJson      */ FILE_JSON,
    /* 55 FileYaml      */ FILE_YAML,
    /* 56 FileToml      */ FILE_TOML,
    /* 57 FileMarkdown  */ FILE_MARKDOWN,
    /* 58 FileShell     */ FILE_SHELL,
    /* 59 FileSql       */ FILE_SQL,
    /* 60 FileRuby      */ FILE_RUBY,
    /* 61 FilePhp       */ FILE_PHP,
    /* 62 FileLua       */ FILE_LUA,
    /* 63 FileSwift     */ FILE_SWIFT,
    /* 64 FileKotlin    */ FILE_KOTLIN,
    /* 65 FileDocker    */ FILE_DOCKER,
    /* 66 PanelRight    */ UI_PANEL_RIGHT,
    /* 67 ChevronDown   */ UI_CHEVRON_DOWN,
    /* 68 FileDart      */ FILE_DART,
    /* 69 FileHaskell   */ FILE_HASKELL,
    /* 70 FileVue       */ FILE_VUE,
    /* 71 FileReact     */ FILE_REACT,
    /* 72 FileSvelte    */ FILE_SVELTE,
    /* 73 FileZig       */ FILE_ZIG,
    /* 74 FileR         */ FILE_R,
    /* 75 FileScala     */ FILE_SCALA,
    /* 76 FilePerl      */ FILE_PERL,
    /* 77 FileClojure   */ FILE_CLOJURE,
    /* 78 FileElixir    */ FILE_ELIXIR,
    /* 79 FileErlang    */ FILE_ERLANG,
    /* 80 FileJulia     */ FILE_JULIA,
    /* 81 FileFSharp    */ FILE_FSHARP,
    /* 82 Clock         */ UI_CLOCK,
    /* 83 Trash         */ UI_TRASH,
    /* 84 Eye           */ UI_EYE,
    /* 85 Pencil        */ UI_PENCIL,
];
