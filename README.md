# 随笺 (Suijian) — Windows 桌面便签

按 `Ctrl+Space` 呼出，随手记，写完自动保存，收起即走。下次打开，内容都在。

## 能做什么

- 富文本便签：加粗、斜体、列表、待办、代码、链接、图片（截图直接粘贴）
- 全文搜索，几千条也能秒出结果
- 标签分类、归档、回收站
- 自动保存：输入停顿约 300 毫秒即写入磁盘；保存失败会明确提示并可重试，不会静默丢内容
- 备份导出为 zip；恢复前先展示包内清单供核对，并自动做安全快照
- 数据库异常时自动进入只读模式，保住数据，可在数据目录手动取用

## 安装

- 安装版 `Random-Notes-x.y.z-Setup.exe`：安装后开始菜单出现“随笺”
- 单文件版 `Random-Notes-x.y.z-Portable.exe`：双击即用，免安装
- 需要 Windows 10/11 x64；WebView2 运行时系统一般自带，安装版缺失时会自动联网下载安装（首次安装需联网）

## 数据在哪里

`%APPDATA%\com.suijian.notes`（数据库、附件、备份都在此）。换机时**退出随笺后**整目录拷走即可。
试用可用 `--user-data-dir <路径>` 参数开独立数据（安装版对 `随笺.exe`，单文件版对下载的 exe 本身）；**需先退出正在运行的随笺**——应用为单实例：同一时间只允许一个进程，多开时新进程会直接退出并唤醒已有窗口。

备份包不加密（含便签与图片内容），请妥善保管。

## 云端备份

设置页「云端备份」可把备份包上传到坚果云 WebDAV：账号填坚果云注册邮箱，密码填网页端「安全选项 → 添加应用」生成的应用密码（不是登录密码），支持手动上传与按间隔自动备份。备份前会比对内容指纹，内容无变化时不重复上传；坚果云免费版每月上传流量 1GB（下载 3GB）。

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
