# wormhole-gui

基于 [magic-wormhole.rs](https://github.com/magic-wormhole/magic-wormhole.rs) 的 Windows 桌面 GUI 文件 / 文字传输工具。**端到端加密、双向消息、双向文件、纯本地、零账号。**

## 功能

- ✉️ **双向文字消息**：连接后双方都能随时发，左右气泡时间线
- 📁 **双向文件传输**：拖拽 / 点附件，支持任意类型；进行中可取消
- ⚠️ **可执行文件红色警告**：`.exe / .msi / .bat / .cmd / .com / .scr / .ps1` 双因素判定
- 🔒 **端到端加密**：magic-wormhole 协议，PAKE + NaCl SecretBox + Noise transit
- 🔢 **数字短码**（默认）：示例 `15-123-456`，比英文词更适合口播；可在设置切换为英文词表
- 🌐 **自定义 relay**：可在设置里改 mailbox / transit 服务器（默认 Least Authority 公共 relay）
- ⏰ **短码 TTL**：mailbox 服务器侧 5 分钟过期，缩短攻击窗口
- 🚫 **零持久身份**：没有账号、没有手机号、没有联系人，关闭即销毁
- 🔄 **自动更新**：v0.3.0 起内置（仅安装版自动应用；便携版需手动覆盖）

## 安装

[Releases](https://github.com/lemond-dev/chat_one/releases) 提供两种下载：

| 形态 | 适用场景 | 文件 |
|---|---|---|
| **NSIS 安装版** | 普通用户、希望开始菜单 / 控制面板入口、需要自动更新 | `wormhole-gui-setup.exe` |
| **便携版** | 技术用户、U 盘携带、单机多副本测试 | `wormhole-gui.exe` |

系统要求：Windows 10/11 + WebView2（一般已预装）。

> 自签名：首次启动 SmartScreen 可能拦截，点 **更多信息 → 仍要运行**。正规代码签名在 v1.0 加入。

## 使用

1. **A 端**点 **发送** → 屏幕显示一个短码（如 `15-123-456`）
2. 把短码用**电话 / 当面 / Signal** 等可信渠道告诉 B 端
   （**不要**在同一渠道既发短码又发内容！）
3. **B 端**点 **接收**，输入短码，点连接
4. 双方进入会话，发文字、拖文件、点附件
5. 接收方文件默认保存到 `~\Downloads\Wormhole\`（可在设置改）
6. 任一方点 **结束会话** 即终止；双方关闭后会话密钥从内存清除

## 安全使用建议

1. 短码必须通过**和你信任对方的渠道**告知（电话、当面、Signal）
2. **不要**在同一个微信／邮件里既发短码又发内容
3. 收到的文件请先看清楚再打开，特别是 `.exe / .bat / .msi`
4. 完成后双方关闭软件，会话密钥才会从内存清除
5. 短码超过 5 分钟没人接 → 自动失效，请重发
6. 自定义 relay 时，**两端必须配相同的 mailbox**，否则无法相遇

## 仓库结构

```
chat_one/
├── wormhole-gui-architecture.md   架构方案 + spike 验证记录
├── wormhole-spike/                 协议层验证 spike（13 项已 PASS）
└── wormhole-gui/                   主项目
    ├── core/                       Rust 协议层 / 状态机（smol）
    ├── tauri-app/                  Tauri 2 + Svelte 4 桌面应用
    └── dist/                       发布产物（exe / 安装包）
```

## 从源码构建

要求：Rust 1.92+，Node 20+，pnpm 9+，Windows 10/11（带 WebView2）。

```bash
cd wormhole-gui

# 后端单元测试（14 个，离线）
cargo test -p wormhole-gui-core

# 集成测试（3 个，需要外网，访问公共 magic-wormhole relay）
cargo test -p wormhole-gui-core -- --ignored

# 前端依赖
cd tauri-app
pnpm install

# 开发模式（单实例，不能并行跑两份）
pnpm tauri:dev

# 打包：便携 exe + NSIS 安装器
pnpm tauri:build
# → ../target/release/wormhole-gui.exe                 （便携）
# → ../target/release/bundle/nsis/wormhole-gui-setup.exe  （安装器）
```

本地用两个实例测试时，**直接运行打包后的便携 exe 两次**（每个实例独立进程）。`pnpm tauri:dev` 是单实例模式。

## 自动更新

v0.3.0 起，应用启动时会向 GitHub Pages 上的 `latest.json` 静默查询新版本（不调 API，无限流）：

- **安装版**：发现新版后弹横幅，用户确认 → 下载 + 签名校验 + 静默运行新 setup.exe → 自动重启
- **便携版**：发现新版后弹横幅，用户确认 → 下载新 exe → 替换当前文件 → 重启

更新分发流程由 GitHub Actions 自动完成：tag 触发 → 构建双形态 → ed25519 签名 → 上传到 Release → 同步 manifest 到 gh-pages。

也可在设置页点 **检查更新** 主动触发。

## 当前限制

- 仅 Windows；macOS / Linux 编译能过但未打包
- 不支持目录传输（请逐个文件选；后续用 tar 打包）
- 不支持断点续传（spike 已验证 transit 中断后 wormhole 仍存活，待协议加 resume 字段）
- 仅中文 UI；后续加英文 + 跟随系统

## 设计与协议

- [架构方案](wormhole-gui-architecture.md) — 协议、状态机、IPC、安全分析、spike 验证全记录
- [spike 代码](wormhole-spike/src/main.rs) — 13 项协议层验证

## License

MIT
