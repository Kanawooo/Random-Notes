# 随笺 (Suijian) — Windows 桌面便签

按 `Ctrl+Space` 呼出，随手记，写完自动保存，收起即走。下次打开，内容都在。

## 能做什么

- 富文本便签：加粗、斜体、列表、待办、代码、链接、图片（截图直接粘贴）
- 全文搜索，几千条也能秒出结果
- 标签分类、归档、回收站
- 自动保存：输入停顿约 300 毫秒即写入磁盘；保存失败会明确提示并可重试，不会静默丢内容
- 备份导出为 zip；恢复前先展示包内清单供核对，并自动做安全快照
- 数据库异常时自动进入只读模式，保住数据供导出

## 安装

- 安装版 `Random-Notes-x.y.z-Setup.exe`：安装后开始菜单出现“随笺”
- 单文件版 `Random-Notes-x.y.z-Portable.exe`：双击即用，免安装
- 需要 Windows 10/11 x64；WebView2 运行时系统一般自带，安装版缺失时会自动安装

## 数据在哪里

`%APPDATA%\com.suijian.notes`（数据库、附件、备份都在此）。换机时**退出随笺后**整目录拷走即可。
试用可用 `--user-data-dir <路径>` 参数开独立数据（安装版对 `随笺.exe`，单文件版对下载的 exe 本身）。
请勿同时运行多个共用同一数据目录的随笺，存在互相覆盖风险。

## 快捷键

- 呼出/收起：`Ctrl+Space`（系统级，其他软件里也能唤起）
- 新建 `Ctrl+N`、返回搜索 `Ctrl+E`（窗口在前台时生效，可在设置页改）
- `Escape` 用于关闭弹窗、返回搜索、清空搜索词；收起窗口统一用 `Ctrl+Space`
- 与编辑器快捷键（如 `Ctrl+B` 加粗）冲突时，正文内以你设置的为准；设置页录入时即时提示冲突

## 开发

技术栈 Tauri 2 + Rust + React 18 + TypeScript + Tiptap；环境需 Node ≥ 18、pnpm ≥ 9、Rust（MSVC）。

```bash
pnpm install
pnpm tauri dev        # 开发运行
pnpm build            # 前端构建（tsc + vite）
pnpm typecheck        # 类型检查
pnpm lint             # ESLint
pnpm test             # Vitest
cd src-tauri && cargo test
pnpm tauri build      # 打包：exe 与 NSIS 安装包
```

产物在 `src-tauri/target/release/`（exe）与 `src-tauri/target/release/bundle/nsis/`（安装包）。

## 许可

MIT，见 LICENSE。
