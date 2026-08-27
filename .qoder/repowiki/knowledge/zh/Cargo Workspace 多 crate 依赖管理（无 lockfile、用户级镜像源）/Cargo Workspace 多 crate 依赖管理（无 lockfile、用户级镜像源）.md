---
kind: dependency_management
name: Cargo Workspace 多 crate 依赖管理（无 lockfile、用户级镜像源）
category: dependency_management
scope:
    - '**'
source_files:
    - Cargo.toml
    - rust-toolchain.toml
    - .cargo/config.toml
    - crates/aether-win32/Cargo.toml
    - crates/aether-ai-panel/Cargo.toml
    - crates/aether-core/Cargo.toml
    - .github/workflows/ci.yml
---

## 1. 系统/工具

本项目是纯 Rust 工程，采用 **Cargo Workspace** 统一管理 17 个 crate（`crates/aether-*`），通过根 `Cargo.toml` 的 `[workspace] members = [...]` 声明成员，并使用 `resolver = "2"` 启用 Cargo 新解析器。

- **包管理器**：Cargo + crates.io；未使用任何私有 registry、vendor 目录或 Git 子模块方式引入第三方库。
- **版本锁定**：仓库中**不存在** `Cargo.lock`，因此没有提交锁文件来固定所有依赖版本。
- **Rust 工具链**：`rust-toolchain.toml` 锁定 channel 为 `stable`，并附带 `rustfmt`、`clippy` 组件与目标 `x86_64-pc-windows-msvc`。
- **构建配置**：`.cargo/config.toml` 设置默认 build target、`rustflags = ["-C", "target-cpu=sandybridge"]`，并通过 `CFLAGS`/`CXXFLAGS = "/MD"` 统一 C/C++ 扩展的 CRT 链接模式，避免与 onnxruntime 预编译库冲突。
- **镜像源策略**：注释明确说明 crates.io 镜像源由用户级 `~/.cargo/config.toml`（rsproxy-sparse）配置，项目级不重复定义以避免 sparse source 冲突。这意味着依赖下载行为依赖于每个开发者的本地配置，而非仓库内固定源。

## 2. 关键文件

- `Cargo.toml`（根）：workspace 成员列表、统一 `version`/`edition`/`authors`/`license`/`rust-version = "1.70"`、release profile（`lto=fat, opt-level=3, panic="abort", strip=true`）。
- `rust-toolchain.toml`：强制 stable toolchain + rustfmt/clippy + Windows 目标。
- `.cargo/config.toml`：默认 target、rustflags、C/C++ CRT 标志、dev/release profile 覆盖。
- 各 `crates/*/Cargo.toml`：声明 crate 间依赖（如 `aether-win32` 引用 `aether-core`、`aether-render` 等 via `{ path = "../aether-*" }`）以及外部 crate 依赖。
- `.github/workflows/ci.yml`：CI 在 `windows-latest` 上执行 `cargo fmt --check`、`cargo check`、`cargo test`，不生成也不校验 `Cargo.lock`。

## 3. 架构与约定

- **Workspace 内 crate 依赖**：通过相对路径引用（`{ path = "../aether-core" }`），体现内部模块边界清晰，例如 `aether-win32` 作为二进制入口聚合多个 crate。
- **外部依赖版本风格**：多数 crate 使用宽松语义化版本（如 `memchr = "2"`、`serde = "1.0"`、`tokio = { version = "1.35", ... }`），仅少数精确到次版本（如 `image = "0.24"`、`ureq = "2.9"`、`ort = "2.0.0-rc.9"`）。由于没有 `Cargo.lock`，实际解析版本取决于首次构建时的 crates.io 状态。
- **特性开关集中**：对体积敏感的外部 crate（如 `image`、`tokenizers`、`ort`、`windows`）显式关闭 default features 并按需开启最小 feature 集合，控制二进制大小与编译时间。
- **Windows 专属依赖**：`aether-win32` 大量使用 `windows` crate 的 Win32 API feature 白名单（`Win32_Foundation`、`Win32_UI_WindowsAndMessaging` 等），体现平台隔离。
- **AI/推理栈**：`aether-ai-panel`、`aether-win32` 共同引入 `ort`（onnxruntime）和 `tokenizers`，用于本地模型推理。

## 4. 约定与约束

- **版本来源**：所有 crate 的 `version` 通过 workspace 的 `[workspace.package]` 统一派生（`version.workspace = true`），保证版本号一致。
- **Edition/Rust 版本**：workspace 统一 `edition = "2021"`、`rust-version = "1.70"`，开发者必须使用不低于该版本的 Rust。
- **无 vendor / 无私有 registry**：仓库未包含 vendored 源码，也未在 `.cargo/config.toml` 中声明自定义 source，完全依赖 crates.io（经用户级镜像代理）。
- **无锁文件**：未提交 `Cargo.lock`，因此 CI 和本地构建可能解析出不同上游版本——这是当前仓库的实际约束，意味着依赖升级不是可重现的。
- **C/C++ 扩展链接约定**：通过 `CFLAGS`/`CXXFLAGS = "/MD"` 强制动态 CRT，这是为了避免与 onnxruntime 预编译库的 `/MT` vs `/MD` 冲突而采取的硬性约定。
- **CI 约束**：PR/Push 到 `dev` 分支会运行 `cargo fmt --check`，格式化风格由仓库内的 `rustfmt` 配置约束（配合 `rust-toolchain.toml` 中的 `rustfmt` 组件）。
- **发布 profile 约束**：根 workspace 与 `.cargo/config.toml` 均定义了 release profile（`lto=fat, codegen-units=1, opt-level=3, panic="abort", strip=true, incremental=false`），确保发布构建体积最小且性能最优。