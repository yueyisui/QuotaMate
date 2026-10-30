# QuotaMate on Ubuntu / Ubuntu 首次使用

支持 Ubuntu 22.04 / 24.04 LTS x86_64。推荐使用 `.deb`，安装时会解析 WebKitGTK 和 Ayatana AppIndicator 运行依赖：

```bash
sudo apt install ./QuotaMate_0.1.5_amd64.deb
```

在 Ubuntu 的应用菜单搜索 **QuotaMate**。首次启动默认在右上角显示 `5h …% · W …%`；等待 Codex 返回数据后自动更新。点击图标打开原生菜单，选择额度行或“打开 QuotaMate”查看完整详情，也可刷新、打开计划任务、切换显示模式和退出。关闭主窗口后仍然更新额度并执行已启用的计划任务。

## 顶栏图标与 GNOME

Ubuntu 默认桌面通常已启用 AppIndicator 扩展。使用精简安装或原生 GNOME 时，如顶栏没有图标：

```bash
sudo apt install gnome-shell-extension-appindicator
gnome-extensions enable ubuntu-appindicators@ubuntu.com
```

若扩展尚未加载，请注销再登录。在 GNOME“扩展”应用中确认 **Ubuntu AppIndicators** 已启用。其他桌面环境需要支持 StatusNotifier/AppIndicator 的托盘。

部分桌面只显示图标，不显示图标旁文字；菜单顶部仍显示实时额度。Linux 托盘使用原生菜单，不依赖 macOS 的托盘坐标或点击事件。

## Codex

先确认已安装并登录 Codex：

```bash
codex --version
codex login
```

QuotaMate 支持 PATH、`~/.local/bin`、`~/.npm-global/bin`、nvm、Volta 等常见安装位置，以及 `/usr/lib/codex/resources/codex` 和 `/usr/lib/chatgpt/resources/codex`。自定义位置可运行：

```bash
CODEX_CLI_PATH=/absolute/path/to/codex quotamate
```

不需要复制 Token。计划任务使用当前本机 Codex 登录，在应用运行时按本地时间执行；休眠、关机或退出时不会执行，也不会补跑。

## AppImage

```bash
chmod +x QuotaMate_0.1.5_amd64.AppImage
./QuotaMate_0.1.5_amd64.AppImage
```

若系统没有 FUSE，可免挂载运行：

```bash
./QuotaMate_0.1.5_amd64.AppImage --appimage-extract-and-run
```

AppImage 也需要桌面支持 AppIndicator。建议安装 `.deb` 以获得应用菜单和可靠的开机启动路径；若使用 AppImage 并开启自启动，请将文件保存在固定位置。

配置保存在 `~/.config/com.quotamate.app/`。在“关于”页面点击“打开日志目录”查看诊断信息；反馈时请移除个人信息。桌面宠物的拖动、定位和置顶效果取决于窗口管理器，Wayland 下尤其如此；顶栏模式使用原生 AppIndicator。

## English

Supports Ubuntu 22.04 / 24.04 LTS x86_64. Install the `.deb` with `sudo apt install ./QuotaMate_0.1.5_amd64.deb`, then launch QuotaMate from the application menu. Sign in to the local Codex CLI first. Top-bar quota is enabled by default; click the indicator for current usage and actions. Closing the main window keeps the app running; choose Exit from the indicator menu to quit.

GNOME requires the Ubuntu AppIndicators extension. Some desktops only show the icon; live quota remains available in the first menu row. AppImage users can use `--appimage-extract-and-run` without FUSE. Prefer `.deb` for desktop integration and autostart. Scheduled sessions run only while the app and computer are awake. Floating pet positioning and always-on-top behavior depend on the desktop, particularly under Wayland.
