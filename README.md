# Rclone Mount Manager

把 NAS / 网络存储 / 对象存储挂成本地盘符的跨平台桌面工具，基于 [rclone](https://rclone.org) 与平台原生 FUSE 驱动。
**没有账号、没有登录、没有广告、不联网验证**——打开就用，断网也能用。

> A cross-platform desktop GUI that mounts SMB / WebDAV / FTP / SFTP / S3 as local drives.
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

这个项目做的事很简单：把 rclone 的能力包一层好用的界面，把依赖一起打进安装包，同时把这些坑都填掉。

## 功能

### 挂载

- **五种后端**：SMB / CIFS、WebDAV、FTP、SFTP、**S3 兼容对象存储**
- S3 兼容族内置服务商预设：Amazon S3、阿里云 OSS、腾讯云 COS、Cloudflare R2、Backblaze B2、Wasabi、DigitalOcean Spaces、MinIO / 自建
- 挂成真实盘符（Windows `Z:`）或挂载点（`/mnt/nas`）
- **本地磁盘 / 网络驱动器**两种形态（Windows）
- **只读挂载**：给「只看不改」的资料库上把锁
- **启动时自动挂载**：常用连接开机即挂
- **断线自动重连**：后台看护线程，指数退避重试（最多 5 次），并记录重连次数
- **列表行内启停**：不用点进详情，每行右侧直接「启动 / 停止」
- **全部启动 / 全部停止**：一键批量操作，托盘菜单也能触发

### 状态一眼看清

列表每个连接前面有个状态圆点，四种颜色对应四种状态：

| 颜色 | 含义 |
|---|---|
| 🟢 绿 | 已挂载，探测正常 |
| 🟡 黄 | 已挂载，但延迟偏高 / 网络波动 / 探测超时 |
| 🔴 红 | 进程退出、认证失败、连续探测失败，当前不可用 |
| ⚪ 灰 | 已停止 |

黄色不是猜的：后台按设定间隔对已挂载连接跑一次轻量探测（`rclone lsd --max-depth 0`，走真实协议栈），
默认每 15 秒一次，**单次往返超过 1200ms、或连续 2 次超时变黄，连续 3 次失败变红**。
这三组阈值和探测间隔都能在「设置 → 探测规则」里改，也可以一键恢复推荐值。探测结果会写进对应连接的挂载日志。

### 依赖自带，装完即用

- **rclone 内置在安装包里**，不用自己下载、不用配 PATH
- **WinFsp 驱动**（Windows 挂盘符必需）随包附带，可在「设置 → 依赖与更新」里一键安装 / 升级
- 「设置 → 依赖与更新」显示管理器、rclone、WinFsp 的当前版本与来源，可一键检查更新
- rclone 更新会解压到应用配置目录，**优先于内置版本**，升级不需要重装应用
- **升级 / 重装软件不会动配置目录**：连接、设置、日志都原样保留

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

- **五语言界面**：简体中文 / 繁體中文 / English / 日本語 / 한국어，默认跟随系统语言，可在设置里手动切换（下拉项用各自语言显示）
- 跟随系统深浅色
- **统一的设置页**：通用 / 启动 / 探测规则 / 日志 / 依赖与更新 / 数据，分区标题，全部弹窗同一套样式
- **开机自启动 + 静默启动**（默认勾选）：静默时只在托盘待命，不弹主界面；手动打开程序永远显示窗口
- **托盘左键 = 打开主界面**，右键才是菜单（显示主窗口 / 全部挂载 / 全部卸载 / 退出）；关窗口 = 收进托盘
- **日志自动管理**：单个连接日志到上限自动裁剪，超过保留天数的日志自动清理
- **连接配置导入 / 导出**：换机器、重装系统都不用手动重建；密码以 rclone 混淆格式随文件带走，不是明文
- 连接置顶、搜索过滤（名称 / 地址 / 盘符 / 用户名）、`Ctrl+N` 新建、`Ctrl+R` 刷新
- 挂载日志实时刷新（挂载中每 4 秒），探测结果也在这里

<table>
<tr>
<td><img src="docs/screenshots/new-connection.png" alt="新建连接"></td>
<td><img src="docs/screenshots/s3-form.png" alt="S3 兼容对象存储"></td>
</tr>
<tr>
<td><img src="docs/screenshots/settings.png" alt="设置：探测规则与语言"></td>
<td><img src="docs/screenshots/settings-deps.png" alt="设置：依赖与数据"></td>
</tr>
<tr>
<td><img src="docs/screenshots/en-main.png" alt="English 界面"></td>
<td><img src="docs/screenshots/main-dark.png" alt="深色模式"></td>
</tr>
</table>

## 平台支持

| | Windows | macOS | Linux |
|---|---|---|---|
| 界面 / 配置 / 日志 | ✅ | ✅ | ✅ |
| 挂载驱动 | WinFsp（随包） | macFUSE / FUSE-T | fuse3 |
| 目标形式 | 盘符 `Z:` | 挂载点 | 挂载点 |
| rclone | 内置 | 内置 | 内置 |
| 自启动 | ✅ 注册表 | ✅ LaunchAgent | ✅ .desktop |
| 托盘 | ✅ | ✅ | ✅ |
| 状态 | 已实测 | 代码就绪，待实机验证 | 代码就绪，待实机验证 |

## 安装

### 1. 安装包

从 [Releases](https://github.com/Jie-baobao/rclone-mount-manager/releases) 下载：

- Windows：`.msi` / `.exe`
- macOS：`.dmg`
- Linux：`.AppImage` / `.deb` / `.rpm`

**rclone 已内置，不需要另外安装。**

### 2. 挂载驱动

| 平台 | 说明 |
|---|---|
| Windows | 安装包里带了 WinFsp 安装程序。没装过时，打开「设置 → 依赖与更新」点「一键安装」（会弹一次管理员授权） |
| macOS | `brew install --cask macfuse`（或 `fuse-t`），首次需在「系统设置 → 隐私与安全性」里允许系统扩展 |
| Linux | `sudo apt install fuse3` / `sudo dnf install fuse3` |

程序启动时左下角会显示环境自检结果；点它打开设置页，「依赖与更新」里缺什么一眼能看到，也能直接装。

### 3. 从源码构建

需要 **Node 18+**、**Rust 1.77+**；Windows 还需要 **Microsoft C++ Build Tools**。

```bash
git clone https://github.com/Jie-baobao/rclone-mount-manager.git
cd rclone-mount-manager
npm install

# 可选：把 rclone 放进 resources，构建出的包才自带 rclone
#   下载 https://downloads.rclone.org/ 对应平台压缩包，解出 rclone(.exe)
#   放到 src-tauri/resources/rclone/

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
3. 选盘符（或挂载点），保存 —— 新建的连接会**自动挂载**，不用再点一次启动
4. 资源管理器里就出现了；之后在列表里直接点「启动 / 停止」即可

## 常见坑

这些都是真实踩过的，写在最前面：

- **漏写端口**：不少 NAS 的 WebDAV 走非标准端口（如 `5005` / `5006`）。只填主机名会落到 80 端口的反向代理上，返回一段 HTML，客户端就报「XML 语法错误」之类的怪错。本工具会自动补端口，但显式填上更稳。
- **密码里的 `!` 被输入法打成全角 `！`**：屏幕上几乎看不出差别，但服务端一定返回 `401`，客户端还会无限弹密码框。用「显示」核对一次就能发现。
- **自签证书**：HTTPS 的 WebDAV 如果是自签证书，必须勾「忽略证书校验」。
- **SMB 共享名不能省**：`\\NAS\share` 里的 `share` 是必填的，只填 IP 挂不上。
- **S3 与服务商不匹配**：MinIO 和多数自建网关要勾「强制 path-style」，选对应预设会自动设置；区域填错会报签名错误。
- **Windows 上盘符冲突**：选一个没被占用的盘符。

## 工作原理

```
┌─────────────────────────────┐
│  前端  React + TypeScript   │  连接列表 / 向导 / 日志 / 版本面板
└──────────────┬──────────────┘
               │ Tauri IPC（命令 + 事件）
┌──────────────▼──────────────┐
│  后端  Rust                 │
│  ├ store.rs    配置读写（原子写 + .bak）        │
│  ├ rclone.rs   conf 生成 / obscure / 挂载参数   │
│  ├ diag.rs     PROPFIND 诊断 / 驱动检测         │
│  ├ update.rs   版本检查 / 下载安装 rclone 与驱动 │
│  ├ settings.rs 设置持久化（语言 / 探测 / 日志）   │
│  └ autostart.rs 三平台自启动（带 --silent）      │
└──────────────┬──────────────┘
               │ 子进程
        ┌──────▼──────┐
        │   rclone    │ ← WinFsp / macFUSE / fuse3
        └─────────────┘
```

- 每个连接对应一个 `rclone mount` 子进程，句柄由后端持有
- 后台看护线程每 5 秒检查一次存活，意外退出按 `2s → 5s → 8s → 11s → 14s` 退避重连
- 健康探测线程按设置里的间隔（默认 15 秒）对已挂载连接做一次轻量请求，得出绿 / 黄 / 红，并把结果写进该连接的日志
- 每小时巡检一次日志目录，按「上限大小 / 保留天数」自动裁剪与清理
- 状态变化通过 Tauri 事件推给前端，另有 8 秒兜底轮询
- 关闭窗口 = 收进托盘；从托盘「退出」才会停掉所有挂载

## 配置文件位置

| 平台 | 路径 |
|---|---|
| Windows | `%APPDATA%\io.github.rclonemount.manager\` |
| macOS | `~/Library/Application Support/io.github.rclonemount.manager/` |
| Linux | `~/.config/io.github.rclonemount.manager/` |

- `connections.json` — 连接列表（不含密码）
- `settings.json` — 设置（语言 / 静默启动 / 探测规则 / 日志策略）
- `rclone.conf` — rclone 远程配置（密码已 obscure）
- `bin/rclone(.exe)` — 手动更新过的 rclone（优先于内置版本）
- `logs/mount-<id>.log` — 每个连接的挂载日志（含健康探测记录）

升级安装包只会覆盖程序本体，不会碰这个目录；卸载时也默认保留。

## 路线图

- [x] 随包附带 rclone 二进制，免手动安装
- [x] S3 兼容对象存储
- [x] 多语言（简体中文 / 繁體中文 / English / 日本語 / 한국어）
- [x] 连接配置导入 / 导出
- [x] 设置页（探测规则、日志策略、依赖更新、启动行为）
- [ ] 更多云盘后端（Google Drive / OneDrive / Dropbox，需要 OAuth 授权流程）
- [ ] macOS / Linux 实机验证与打包签名
- [ ] 系统文件管理器右键菜单集成
- [ ] 带宽与流量的可视化统计（**待定**：常驻采样会额外占用 CPU / IO，先不做，等有更轻量的方案再评估）

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
