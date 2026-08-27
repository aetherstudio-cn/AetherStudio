---
kind: dependency_management
name: Rust Workspace 依赖管理（Cargo workspace + 用户级 crates.io 镜像）
category: dependency_management
scope:
    - '**'
source_files:
    - Cargo.toml
    - rust-toolchain.toml
    - .cargo/config.toml
    - crates/aether-core/Cargo.toml
    - crates/aether-win32/Cargo.toml
    - crates/aether-render/Cargo.toml
    - crates/aether-lsp/Cargo.toml
    - crates/aether-ai/Cargo.toml
---

## 1. 使用的系统/方法

本项目采用 **Cargo workspace** 统一管理多个 `aether-*` crate 的依赖，通过根 `Cargo.toml` 集中声明工作区成员、统一版本与发布配置。所有 crate 共享同一份 Rust toolchain（`rust-toolchain.toml` 锁定为 `stable` channel，并附带 `rustfmt`、`clippy` 组件及 `x86_64-pc-windows-msvc` 目标）。

- **包管理器**：Cargo（workspace resolver = "2"）。
- **版本策略**：工作区级别使用 `[workspace.package]` 统一定义 `version = "0.1.0"`、`edition = "2021"`、`authors`、`license`、`rust-version = "1.70"`；各子 crate 通过 `version.workspace = true` / `edition.workspace = true` 引用，保证版本一致。
- **crate 间依赖**：通过 `{ path = "../aether-xxx" }` 以相对路径引用同 workspace 内的其他 crate，形成清晰的内部依赖图（如 `aether-win32` → `aether-core`、`aether-render`、`aether-lsp`、`aether-ai` 等）。
- **第三方依赖**：每个 crate 在自身 `Cargo.toml` 的 `[dependencies]` 中声明，版本号采用语义化主/次版本约束（如 `tokio = "1.35"`、`serde = "1.0"`、`windows = "0.58"`），未使用精确 `=` 锁定。
- **构建时 C/C++ 依赖**：通过 `.cargo/config.toml` 设置 `CFLAGS = "/MD"`、`CXXFLAGS = "/MD"`，统一静态 CRT (`/MT`) 与 onnxruntime 预编译库 (`/MD`) 之间的运行时冲突，确保 cargo test 链接期 LNK2038 不报错。
- **crates.io 源**：项目级 `.cargo/config.toml` 明确注释“crates.io 镜像源统一由用户级 `~/.cargo/config.toml` 配置（rsproxy-sparse），项目级不再重复定义，避免同名 sparse 源冲突”，因此仓库内不包含私有 registry 或 vendor 目录。
- **无 vendoring**：仓库中没有 `vendor/` 目录，也没有 `Cargo.lock` 提交到仓库（`target/` 在 `.gitignore` 中），依赖解析由 Cargo 每次从远程源拉取。

## 2. 关键文件

- `Cargo.toml`：workspace 定义、成员列表、`[workspace.package]` 统一元数据、`[profile.release]` 全局优化（lto=fat、opt-level=3、panic=abort、strip=true）。
- `rust-toolchain.toml`：锁定 stable toolchain、附加 rustfmt/clippy 组件、Windows MSVC target。
- `.cargo/config.toml`：HTTP 超时、默认 build target、rustflags（`target-cpu=sandybridge`）、dev/release profile 覆盖、C/C++ 编译器标志。
- 各 `crates/aether-*/Cargo.toml`：具体 crate 的依赖声明与 feature 选择（如 `ort = { version = "2.0.0-rc.9", features = ["std", "download-binaries", "tls-rustls"] }`、`image = { default-features = false, features = [...] }`）。

## 3. 架构与约定

- **分层依赖**：底层 `aether-core` 仅依赖通用 Rust crate（regex、rayon、smallvec 等），上层 `aether-win32` 作为二进制入口聚合 UI、LSP、AI、终端等 crate，再引入平台相关依赖（`windows`、`webview2-com`、`ort`、`tokenizers`）。
- **Feature 裁剪**：对体积敏感 crate 显式关闭默认 feature（如 `image`、`ort`、`tokenizers`），仅启用所需功能，减少最终二进制大小。
- **跨 crate 同步**：由于版本集中在 workspace 层，新增/升级 crate 版本只需修改对应 crate 的 `Cargo.toml`，无需维护独立版本表。
- **CI/本地构建一致性**：通过 `rust-toolchain.toml` 和 `.cargo/config.toml` 保证本地与 CI 使用相同 toolchain、target 和编译选项。

## 4. 约定与约束

- **版本来源**：所有 crate 的 `version`、`edition` 必须通过 `workspace = true` 继承，禁止在子 crate 中单独指定（由 workspace 强制）。
- **内部 crate 引用**：必须使用 `{ path = "../aether-xxx" }` 形式，不得通过 crates.io 发布中间版本。
- **外部 crate 版本约束**：使用宽松的主/次版本范围（如 `"1.0"`、`"0.58"`），不在仓库中提交 `Cargo.lock`，依赖锁定由 Cargo 缓存决定。
- **crates.io 镜像配置**：仓库内不配置任何 registry 源，镜像地址由开发者个人 `~/.cargo/config.toml` 提供；若需私有源应在用户级配置而非仓库级。
- **C/C++ 运行时**：所有 C/C++ 扩展必须接受 `/MD` 动态 CRT 标志，否则将触发 LNK2038 链接错误。
- **目标平台**：默认构建目标固定为 `x86_64-pc-windows-msvc`，跨平台构建需显式覆盖该设置。
- **发布 profile**：release 构建统一启用 fat LTO、单 codegen unit、opt-level 3、panic=abort、strip、禁用增量编译，以保证最终二进制性能与体积。