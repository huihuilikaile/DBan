# DBan 开发说明

本文档说明 DBan 的主要模块、数据流和发布检查项，便于后续维护和协作。

## 技术栈

- 前端：Vue 3、TypeScript、Vite
- 桌面框架：Tauri 2
- 原生层：Rust、Windows API
- 包管理器：pnpm
- 本地数据：Tauri Store、Windows Credential Manager

## 前端模块

- `src/App.vue`：应用模式切换、主标签和全局悬停提示层。
- `src/store.ts`：共享状态、持久化、Tauri 事件和命令调用。
- `src/components/PanelHeader.vue`：标题栏、窗口移动和设置菜单。
- `src/components/TodoList.vue`：待办创建、编辑、完成和清除。
- `src/components/HistoryList.vue`：已清除完成待办的历史记录。
- `src/components/VaultList.vue`：密码元数据及凭据管理器调用。
- `src/components/AppLauncher.vue`：应用快捷方式和分类管理。
- `src/components/MiniPlayer.vue`：系统媒体状态和控制。
- `src/components/CapsuleBar.vue`：胶囊模式和待办展开区域。

## Rust 模块

- `src-tauri/src/main.rs`：应用状态、窗口模式、托盘、快捷键、窗口位置和 Tauri 命令。
- `src-tauri/src/hot_corner.rs`：屏幕顶部识别区域和停留触发逻辑。
- `src-tauri/src/media.rs`：Windows 系统媒体会话读取和控制。
- `src-tauri/src/secrets.rs`：Windows 凭据管理器读写。
- `src-tauri/src/launcher.rs`：应用信息提取和进程启动。

## 窗口与多显示器

主面板的逻辑尺寸为 `320 x 520`，胶囊的逻辑尺寸为 `220 x 40`。Rust 根据鼠标所在显示器的缩放比例换算物理尺寸，并为每个显示器保存独立位置。

窗口位置支持三种锚点：

- `left`：靠近当前显示器左侧工作区。
- `right`：靠近当前显示器右侧工作区。
- `free`：使用显示器宽度比例保存自由位置。

顶部识别区域以记忆窗口位置为中心。默认宽度为 `360` 逻辑像素，默认停留时间为 `250ms`，前端设置会实时同步到 Rust 原子状态。

## 数据模型

普通界面数据保存在 Tauri Store 的 `dban.json` 中，主要键包括：

- `todos`
- `todoHistory`
- `vaults`
- `apps`
- `appCategories`
- `activeAppCategoryId`
- `theme`
- `pinned`
- `globalShortcutEnabled`
- `topTriggerWidth`
- `topTriggerDwellMs`

窗口位置单独保存在 `window-placement.json`。密码条目只在 Store 中保存站点、账户和凭据 ID，密码正文由 Windows Credential Manager 管理。

## 事件与命令

前端通过 Tauri `invoke` 调用窗口、媒体、凭据和启动器命令。Rust 向前端发送的主要事件包括：

- `dban://mode`
- `dban://autostart`
- `media://update`

新增命令后，需要同时更新 `tauri::generate_handler!`。新增插件权限时，还需要检查 `src-tauri/capabilities/default.json`。

## 发布检查

发布前至少运行：

```powershell
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build --no-bundle
```

构建前应关闭正在运行的 `dban.exe`，否则 Windows 会锁定 Release 输出文件。

检查仓库状态，确保以下内容未被提交：

- `node_modules/`
- `dist/`
- `src-tauri/target/`
- 用户数据文件
- 本地会话文件
- 日志和临时文件

## 代码约定

- 前端状态优先放在 `src/store.ts`，组件保留与界面交互直接相关的局部状态。
- Windows 原生逻辑放在 Rust 侧，避免在前端模拟系统能力。
- 窗口尺寸和位置统一使用逻辑尺寸定义，在显示器边界处转换为物理像素。
- 修改共享行为时补充小范围 Rust 测试，并同时验证前端生产构建。
