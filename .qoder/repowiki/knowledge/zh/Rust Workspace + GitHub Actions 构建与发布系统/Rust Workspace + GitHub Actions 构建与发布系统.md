---
kind: build_system
name: Rust Workspace + GitHub Actions 构建与发布系统
category: build_system
scope:
    - '**'
source_files:
    - Cargo.toml
    - rust-toolchain.toml
    - .cargo/config.toml
    - scripts/build-release.ps1
    - crates/aether-win32/Cargo.toml
    - crates/aether-cli/Cargo.toml
    - installer/aether-setup.nsi
    - .github/workflows/ci.yml
    - .github/workflows/release-main.yml
    - .github/workflows/release-demo.yml
---

## 1. 构建系统与工具链

项目使用 **Cargo workspace** 聚合 15 个 `aether-*` crate（根 `Cargo.toml` 的 `[workspace] members`），统一版本、edition、rust-version（1.70）和 release profile。工具链通过 `rust-toolchain.toml` 锁定为 `stable`，并预装 `rustfmt`、`clippy` 组件，目标平台固定为 `x86_64-pc-windows-msvc`。

`.cargo/config.toml` 中设置默认 build target 为 Windows MSVC，并通过 `CFLAGS=/MD`、`CXXFLAGS=/MD` 强制 C/C++ 依赖使用动态 CRT，避免与 onnxruntime 及 Rust 运行时产生 LNK2038 链接冲突；同时设置 `target-cpu=sandybridge` 作为通用优化基准。

Release profile 在 workspace 根与 `.cargo/config.toml` 双重声明：`lto=fat`、`codegen-units=1`、`opt-level=3`、`panic=abort`、`strip=true`、`incremental=false`，确保最终二进制体积最小、性能最优。

## 2. 关键文件与产物

- **Workspace 定义**: `Cargo.toml`（members、workspace.package、release profile）
- **工具链**: `rust-toolchain.toml`（stable + rustfmt/clippy + x86_64-pc-windows-msvc）
- **Cargo 配置**: `.cargo/config.toml`（默认 target、rustflags、CRT 统一、dev/release profile）
- **GUI 二进制入口**: `crates/aether-win32/Cargo.toml` 中的 `[[bin]] name = "aether-app"`
- **CLI 二进制入口**: `crates/aether-cli/Cargo.toml` 中的 `[[bin]] name = "aether"`
- **本地构建脚本**: `scripts/build-release.ps1`（支持 `--Release`、`--Run` 参数）
- **NSIS 安装包脚本**: `installer/aether-setup.nsi`（Unicode、MUI2、中文/英文界面）
- **CI/CD**: `.github/workflows/ci.yml`、`release-main.yml`、`release-demo.yml`、`branch-protection.yml`、`dev-feature-tracker.yml`

## 3. 架构与流程

### 开发构建
```bash
cargo build              # debug (增量编译)
cargo build --release    # release (fat LTO, strip)
scripts/build-release.ps1 -Release -Run   # 一键构建并运行 aether-app.exe
```

### CI（push/PR to dev）
`ci.yml` 在 `windows-latest` 上执行：`cargo fmt --check` → `cargo check` → `cargo test`，仅做质量门禁，不产出 artifact。

### 正式 Release（push to main）
`release-main.yml` 流水线：
1. Checkout main（fetch-depth=0）+ 安装 stable Rust + `rust-cache` 缓存
2. 用 GitHub Script 生成版本号：`vYYYY.MM.DD-buildN`（UTC+8 北京时间，自动递增 build 号）
3. 注入环境变量 `AETHER_VERSION` 到 `cargo build --release --bin aether-app`
4. 复制产物到 `release/aether-app.exe`
5. 调用 `makensis /DVERSION=... /DSOURCE_EXE=... /DOUTPUT_EXE=... installer/aether-setup.nsi` 生成 `aether-setup.exe`
6. 创建 GitHub Release（tag=`vYYYY.MM.DD-N`），上传 `aether-app.exe` 与 `aether-setup.exe`，并从合入 PR 正文提取「更新了哪些特性」「修复了哪些问题」作为 release notes

### 演示版 Release（手动触发）
`release-demo.yml` 支持 workflow_dispatch 传入版本号或自动生成，从 `dev` 分支构建，读取 `.github/features/*.md` 汇总特性列表，发布为 `prerelease: true` 的草稿 release，并在 commit 下评论发布结果，最后清理 `.github/features/`。

### 安装包行为（NSIS）
- 每用户安装到 `%LOCALAPPDATA%\Programs\Aether Studio`，无需 UAC（`RequestExecutionLevel user`），便于后续应用内直接替换 exe 实现静默更新
- 安装前 `taskkill /F /IM aether-app.exe /T` 强制关闭运行中的实例
- 注册 HKCU 卸载信息、开始菜单快捷方式、可选桌面快捷方式
- 将 `resources/app_icons/aether.ico` 一并安装到 `INSTDIR\resources\app_icons` 供运行时加载

## 4. 约定与约束

- **版本管理**：所有 crate 通过 `version.workspace = true` 共享 workspace 根定义的 `0.1.0`；发布时由 CI 覆盖生成 `vYYYY.MM.DD-N` tag，本地开发仍保持 `0.1.0`。
- **目标平台**：全局锁定 `x86_64-pc-windows-msvc`，不支持跨平台交叉编译（无 cross-compilation 配置）。
- **CRT 一致性**：通过 `.cargo/config.toml` 的 `CFLAGS=/MD`、`CXXFLAGS=/MD` 强制所有原生依赖使用 `/MD` 动态 CRT，这是硬性约束（注释明确说明是为避免 LNK2038 RuntimeLibrary 不匹配）。
- **二进制命名**：GUI 程序固定名为 `aether-app`，CLI 固定名为 `aether`，NSIS 脚本中硬编码 `APP_EXE = "aether-app.exe"`，修改需同步两处。
- **Release Profile 双写**：workspace 根 `Cargo.toml` 与 `.cargo/config.toml` 均声明 release profile，二者必须保持一致（当前均为 fat LTO + strip + panic=abort）。
- **Feature 收集约定**：`release-demo.yml` 要求特性描述以 `.github/features/PRxxx.md` 形式提交，且内容遵循 `## 更新的特性` 标题格式，否则回退为“常规维护和改进”。
- **Release Notes 约定**：`release-main.yml` 解析 PR 正文中的 `### 更新了哪些特性` 与 `### 修复了哪些问题` 两段，缺失则显示“本次为常规维护与改进”。
- **测试**：单元测试通过 `cargo test` 运行；GUI 自动化测试位于 `tests/`，使用 PowerShell + PostMessage 注入框架（`tests/framework/AetherTest.psm1`），非本构建系统范畴。
- **依赖缓存**：CI 使用 `Swatinem/rust-cache@v2`，shared-key 为 `aether-release`，加速 release 构建。
- **可执行产物路径**：始终位于 `target/x86_64-pc-windows-msvc/{debug|release}/`，脚本与 CI 均硬编码该路径。