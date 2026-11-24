use std::path::PathBuf;

/// 对话框按钮动作（克隆对话框复用）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogAction {
    None,
    Connect,
    Cancel,
}

/// 克隆仓库对话框
#[derive(Clone, Debug)]
pub struct CloneRepoDialog {
    pub visible: bool,
    pub url: String,
    pub target_path: Option<PathBuf>,
    pub error_message: Option<String>,
    pub focus_field: usize,
    pub hover_button: Option<usize>,
    pub clone_btn_rect: Option<crate::layout::Region>,
    pub cancel_btn_rect: Option<crate::layout::Region>,
}

impl CloneRepoDialog {
    pub fn new() -> Self {
        Self {
            visible: false,
            url: String::new(),
            target_path: None,
            error_message: None,
            focus_field: 0,
            hover_button: None,
            clone_btn_rect: None,
            cancel_btn_rect: None,
        }
    }

    pub fn reset(&mut self) {
        self.url.clear();
        self.target_path = None;
        self.error_message = None;
        self.focus_field = 0;
        self.hover_button = None;
        self.clone_btn_rect = None;
        self.cancel_btn_rect = None;
    }
}
