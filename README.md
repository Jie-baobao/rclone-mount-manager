# Rclone Mount Manager

把 NAS / 网络存储挂成本地盘符的跨平台桌面工具，基于 [rclone](https://rclone.org) 与平台原生 FUSE 驱动。
**没有账号、没有登录、没有广告、不联网验证**——打开就用，断网也能用。

> A cross-platform desktop GUI that mounts SMB / WebDAV / FTP / SFTP as local drives.
> Powered by rclone + WinFsp / macFUSE / fuse3. No account, no sign-in, no ads, fully offline.

[![build](https://github.com/Jie-baobao/rclone-mount-manager/actions/workflows/build.yml/badge.svg)](https://github.com/Jie-baobao/rclone-mount-manager/actions/workflows/build.yml)
[![release](https://img.shields.io/github/v/release/Jie-baobao/rclone-mount-manager)](https://github.com/Jie-baobao/rclone-mount-manager/releases)
[![license](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
![platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)

![主界面](docs/screenshots/main.png)

---

## 为什么做这个

同类工具各有各的坑：

| 工具 | 问题 |
|---|---|
| **RaiDrive** | 强制登录账号，服务器在境外，网络受限时连界面都进不去 |
| **Mountain Duck** | 好用，但 $39 起 |
| **Windows 自带 WebDAV 映射** | 单文件 50MB 限制、非标准端口支持差、断线直接卡死资源管理器 |
| **rclone 本身** | 能力最强，但纯命令行，改个连接要背一串参数 |

这个项目做的事很简单：把 rclone 的能力包一层好用的界面，同时把这些坑都填掉。

## 功能

### 挂载

- **四种协议**：SMB / CIFS、WebDAV、FTP、SFTP
- 挂成真实盘符（Windows `Z:`）或挂载点（`/mnt/nas`）
- **本地磁盘 / 网络驱动器**两种形态（Windows）
- **只读挂载**：给「只看不改」的资料库上把锁
- **启动时自动挂载**：常用连接开机即挂
- **断线自动重连**：后台看护线程，指数退避重试（最多 5 次），并记录重连次数
- **全部挂载 / 全部卸载**：一键批量操作，托盘菜单也能触发

### 性能与网络

- **VFS 缓存**（`--vfs-cache-mode full`）+ 可配置缓存上限，异地访问不卡
- **带宽限制**（`--bwlimit`）：走公网 / P2P 时避免占满上行
- **HTTP 代理**：支持给单个连接单独指定代理
- **自签证书**：WebDAV over HTTPS 一键放行

### 诊断与安全

- **连接测试**：WebDAV 发真实 `PROPFIND` 请求，直接区分 `401`（密码错）/ `403`（无权限）/ `404`（路径错）/ `405`（不是 WebDAV）/ `207`（正常），并显示**实际请求的 URL**
- **密码不落明文**：写入 `rclone.conf` 前先用 `rclone obscure` 处理；应用自己的配置里只存连接信息
- **显示密码开关**：肉眼核对用（下面「常见坑」里那条会救你一次）
- **挂载失败显示原始报错**：等 900ms 抓 rclone 的真实退出原因，不给「假成功」
- **配置原子写**：临时文件 + `.bak` 备份，断电/杀进程不会留下半截 JSON

挂载失败时不吞错，把 rclone 的原始报错直接摊在界面上：

![挂载失败](docs/screenshots/error-state.png)

### 体验

- 中文界面，跟随系统深浅色
- 系统托盘常驻：显示主窗口 / 全部挂载 / 全部卸载 / 退出；关窗口 = 收进托盘
- 开机自启动（Windows 注册表 / macOS LaunchAgent / Linux autostart）
- 连接置顶、搜索过滤、`Ctrl+N` 新建、`Ctrl+R` 刷新
- 挂载日志实时刷新（挂载中每 4 秒）

<table>
<tr>
<td><img src="docs/screenshots/new-connection.png" alt="新建连接"></td>
<td><img src="docs/screenshots/main-dark.png" alt="深色模式"></td>
</tr>
</table>

## 平台支持

| | Windows | macOS | Linux |
|---|---|---|---|
| 界面 / 配置 / 日志 | ✅ | ✅ | ✅ |
| 挂载驱动 | WinFsp | macFUSE / FUSE-T | fuse3 |
| 目标形式 | 盘符 `Z:` | 挂载点 | 挂载点 |
| 自启动 | ✅ 注册表 | ✅ LaunchAgent | ✅ .desktop |
| 托盘 | ✅ | ✅ | ✅ |
| 状态 | 已实测 | 代码就绪，待实机验证 | 代码就绪，待实机验证 |

## 安装

### 1. 前置依赖

所有平台都需要 **[rclone](https://rclone.org/downloads/)**：装到 `PATH`，或把可执行文件放到本程序同目录（程序会优先找同目录）。

| 平台 | 额外依赖 |
|---|---|
| Windows | [WinFsp](https://winfsp.dev/rel/) |
| macOS | `brew install --cask macfuse`（或 `fuse-t`） |
| Linux | `sudo apt install fuse3` / `sudo dnf install fuse3` |

程序启动时会在左下角显示驱动与 rclone 检测结果，缺什么一眼能看到。

### 2. 安装包

从 [Releases](https://github.com/Jie-baobao/rclone-mount-manager/releases) 下载：

- Windows：`.msi` / `.exe`
- macOS：`.dmg`
- Linux：`.AppImage` / `.deb`

### 3. 从源码构建

需要 **Node 18+**、**Rust 1.77+**；Windows 还需要 **Microsoft C++ Build Tools**（Tauri 编译依赖，`cl`/`link.exe`）。

```bash
git clone https://github.com/Jie-baobao/rclone-mount-manager.git
cd rclone-mount-manager
npm install
npm run tauri build       # 产物在 src-tauri/target/release/bundle/
```

开发：

```bash
npm run tauri dev         # 桌面应用热重载
npm run dev               # 只跑前端（浏览器打开，用内置模拟数据看界面）
```

界面截图 / 回归检查：

```bash
npx playwright install chromium   # 只需一次
npm run dev                       # 另开一个终端
node scripts/shot.mjs             # 输出到 docs/screenshots/
```

## 使用

1. 点「新建连接」，选协议
2. 填地址 / 端口 / 账号密码 —— 先点「测试连接」验一遍
3. 选盘符（或挂载点），保存
4. 点「挂载」，资源管理器里就出现了

## 常见坑

这些都是真实踩过的，写在最前面：

- **漏写端口**：不少 NAS 的 WebDAV 走非标准端口（如 `5005` / `5006`）。只填主机名会落到 80 端口的反向代理上，返回一段 HTML，客户端就报「XML 语法错误」之类的怪错。本工具会自动补端口，但显式填上更稳。
- **密码里的 `!` 被输入法打成全角 `！`**：屏幕上几乎看不出差别，但服务端一定返回 `401`，客户端还会无限弹密码框。用「显示密码」核对一次就能发现。
- **自签证书**：HTTPS 的 WebDAV 如果是自签证书，必须勾「忽略证书校验」。
- **SMB 共享名不能省**：`\\NAS\share` 里的 `share` 是必填的，只填 IP 挂不上。
- **Windows 上盘符冲突**：选一个没被占用的盘符。

## 工作原理

```
┌─────────────────────────────┐
│  前端  React + TypeScript   │  连接列表 / 向导 / 日志 / 状态
└──────────────┬──────────────┘
               │ Tauri IPC（命令 + 事件）
┌──────────────▼──────────────┐
│  后端  Rust                 │
│  ├ store.rs    配置读写（原子写 + .bak）      │
│  ├ rclone.rs   conf 生成 / obscure / 挂载参数 │
│  ├ diag.rs     PROPFIND 诊断 / 驱动检测       │
│  └ autostart.rs 三平台自启动                  │
└──────────────┬──────────────┘
               │ 子进程
        ┌──────▼──────┐
        │   rclone    │ ← WinFsp / macFUSE / fuse3
        └─────────────┘
```

- 每个连接对应一个 `rclone mount` 子进程，句柄由后端持有
- 后台看护线程每 5 秒检查一次存活，意外退出按 `2s → 5s → 8s → 11s → 14s` 退避重连
- 状态变化通过 Tauri 事件推给前端，另有 8 秒兜底轮询
- 关闭窗口 = 收进托盘；从托盘「退出」才会停掉所有挂载

## 配置文件位置

| 平台 | 路径 |
|---|---|
| Windows | `%APPDATA%\io.github.rclonemount.manager\` |
| macOS | `~/Library/Application Support/io.github.rclonemount.manager/` |
| Linux | `~/.config/io.github.rclonemount.manager/` |

- `connections.json` — 连接列表（不含密码）
- `rclone.conf` — rclone 远程配置（密码已 obscure）
- `logs/mount-<id>.log` — 每个连接的挂载日志

## 路线图

- [ ] macOS / Linux 实机验证与打包签名
- [ ] 更多云存储后端（S3、WebDAV 之外的对象存储）
- [ ] 系统文件管理器右键菜单集成
- [ ] 多语言（English / 简体中文）
- [ ] 随包附带 rclone 二进制，免手动安装
- [ ] 连接配置导入 / 导出
- [ ] 带宽与流量的可视化统计

## 贡献

欢迎 Issue 和 PR。提交前请确保：

```bash
npm run build                        # 前端类型检查 + 构建
cd src-tauri && cargo fmt --check    # Rust 格式
cd src-tauri && cargo check          # Rust 编译检查
```

## 许可

[MIT](LICENSE)。

rclone 同为 MIT；WinFsp 为 GPLv3（作为独立驱动安装，不修改其源码）；macFUSE / FUSE-T 各自遵循其许可。

本项目与 rclone、WinFsp、RaiDrive、Mountain Duck 均无隶属关系，仅使用公开协议与公开 API 的独立实现。
