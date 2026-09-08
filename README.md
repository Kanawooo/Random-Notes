# 随笺 (Suijian) - Tauri 2 / Rust Windows 桌面便签应用

随笺是一款专为 Windows 设计的轻量、优雅、高响应的桌面便签与随手记应用。本项目基于 Tauri 2 + Rust + React 18 + TypeScript + Tiptap 构建，具备更快的启动速度、更低的常驻内存占用与极致的数据可靠性。

---

## 核心特性与架构

### 1. 技术栈
- **后端**：Tauri 2.x、Rust 2021 Edition、rusqlite（集成 SQLite 3 与 FTS5 全文检索）、rfd 原生文件对话框
- **前端**：React 18、TypeScript、Vite 6、Tiptap 富文本编辑器、Vanilla CSS（高质感 Windows 现代暗调与微质感光影）
- **包管理与工具链**：pnpm、Vitest、Testing Library、ESLint 9、Cargo / Clippy / Rustfmt

### 2. 数据可靠性与零丢失自动保存
- **串行化自动保存队列**：在 `useAutoSave` 中建立严格的 FIFO 保存队列与 300ms 防抖机制，自动追踪最新 revision，确保草稿状态、标题及正文按序写入数据库，彻底杜绝多并发保存造成的竞态丢失。
- **窗口隐藏与退出握手**：失焦收起面板、全局快捷键切换、系统托盘右键退出或窗口关闭时，前端与 Rust 后端进行原子握手。在保存未落盘之前绝不静默隐藏或退出；若保存失败，前端界面驻留错误提示与“重试保存”操作，确保数据安全。
- **版本冲突保护**：基于 SQLite 单事务与 `revision` 字段乐观并发控制，防止陈旧闭包数据覆盖最新内容。

### 3. 安全备份与两阶段恢复
- **严格 Zip 检查与防炸弹**：校验 Zip Slip 路径逃逸（拒绝包含 `..`、绝对路径或盘符冒号）、单条目 25MB 上限、总解压 500MB 防炸弹、条目与 manifest 1-to-1 双向对齐。
- **两阶段恢复确认**：先通过系统对话框选择备份文件进行无损完整性解析，在界面展示便签数、标签数、附件数与文件大小供用户核验；用户确认后，在单事务中完成数据库与附件的安全原子替换，并在操作前自动创建本地回滚快照。
- **只读恢复保护模式**：若数据库检测到异常或恢复未完成，应用自动降级为只读诊断模式，提示用户查看数据目录，防止损坏数据被二次污染。

### 4. 品牌与视觉
- 统一使用品牌图标（钴蓝圆角卡片、纸白折角主体与纯黑精细墨迹勾），覆盖 Windows 托盘可见图标、主程序 EXE、NSIS 安装包及桌面快捷方式。

---

## 开发与构建指南

### 环境要求
- Windows 10/11 x64
- Node.js >= 18，pnpm >= 9
- Rust >= 1.77.2 (MSVC 工具链)
- WebView2 运行时

### 常用命令
```bash
# 1. 进入项目目录
cd suijian-tauri

# 2. 安装依赖
pnpm install

# 3. 前端质量门禁
pnpm lint        # ESLint 检查
pnpm typecheck   # TypeScript 类型检查
pnpm test        # Vitest 单元与组件测试
pnpm build       # Vite 生产构建

# 4. Rust 后端门禁
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test

# 5. 开发调试运行
pnpm tauri dev

# 6. 生产打包构建 (Release)
pnpm tauri build
```

打包产物将自动生成于 `src-tauri/target/release/` 以及 `src-tauri/target/release/bundle/nsis/`。
