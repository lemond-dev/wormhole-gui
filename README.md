# chat_one — wormhole-gui

基于 [magic-wormhole.rs](https://github.com/magic-wormhole/magic-wormhole.rs) 的
Windows 端会话式 GUI 文件 / 文字传输工具。

> v0.1：**端到端加密、双向消息、双向文件、SAS 验证、纯本地、零账号**。

## 功能

- ✉️ **双向文字消息**：握手后双方都能随时发，时间线左右气泡
- 📁 **双向文件传输**：拖拽 / 点附件，支持图片、文档、任意类型；进行中可取消
- ⚠️ **可执行文件红色警告**：`.exe / .msi / .bat / .cmd / .com / .scr / .ps1` 双因素判定
- 🔒 **端到端加密**：基于 magic-wormhole 协议，PAKE + NaCl SecretBox + Noise transit
- 🛡️ **SAS 验证码**：4 位数字双方核对，抗"抢码冒充"攻击
- ⏰ **5 分钟短码 TTL**：缩短攻击窗口
- 🚫 **零持久身份**：没有账号、没有手机号、没有联系人、关闭即销毁

## 安装

下载最新 [Release](https://github.com/lemond-dev/chat_one/releases) 里的 `.msi`，
双击安装。需要 Windows 10/11 + WebView2（一般已预装）。

> v0.1 自签名：首次启动 Windows SmartScreen 可能拦截，点 **更多信息 → 仍要运行**。
> 正规代码签名在 v1.0 加入。

## 使用

1. **A 端**点 **发送** → 屏幕显示一个短码（如 `26-dinosaur-spaniel`）
2. 把短码用**电话 / 当面 / Signal** 等可信渠道告诉 B 端
   （**不要**在同一渠道既发短码又发内容！）
3. **B 端**点 **接收**，输入短码，点连接
4. 双方屏幕都会显示同样的 4 位 SAS 数字（如 `1234`），通过电话/当面核对一致后
   各自点 **一致，继续**
5. 进入会话，双向发文字、拖文件、点附件
6. 接收方文件默认保存到 `~\Downloads\Wormhole\`
7. 任一方点 **结束会话** 即终止；双方关闭后会话密钥从内存清除

## 安全使用建议

1. 短码必须通过**和你信任对方的渠道**告知（电话、当面、Signal）
2. **不要**在同一个微信／邮件里既发短码又发内容
3. 连接后**务必**核对 4 位 SAS 验证码
4. 收到的文件请先看清楚再打开，特别是 `.exe / .bat / .msi`
5. 完成后双方关闭软件，会话密钥才会从内存清除
6. 短码超过 5 分钟没人接 → 自动失效，请重发

## 仓库结构

```
chat_one/
├── wormhole-gui-architecture.md   架构方案 + Tier 1/1.5 spike 验证记录
├── wormhole-spike/                 协议层验证 spike（13 项已 PASS）
└── wormhole-gui/                   v0.1 主项目
    ├── core/                       Rust 协议层 / 状态机 (smol)
    └── tauri-app/                  Tauri 2 + Svelte 4 桌面应用
```

## 从源码构建

要求：Rust 1.92+，Node 20+，pnpm 9+，Windows 10/11（带 WebView2）。

```bash
cd wormhole-gui

# 后端单元测试（9 个）
cargo test -p wormhole-gui-core

# 集成测试（3 个，需要外网，访问公共 magic-wormhole relay）
cargo test -p wormhole-gui-core -- --ignored

# 前端依赖
cd tauri-app
pnpm install

# 开发模式
pnpm tauri:dev

# 打包（产出 src-tauri/target/release/bundle/msi/*.msi）
pnpm tauri:build
```

## 当前限制

- v0.1 不支持自定义 relay（写死官方）；v0.2 加入
- v0.1 自动接收策略写死"始终询问"；v0.2 加入"< N MB 自动"
- v0.1 仅中文；v0.2 加入英文 + 跟随系统
- v0.1 不支持目录传输；v0.2 用 tar 打包
- v0.1 不支持断点续传；spike 验证 transit 中断后 wormhole 仍存活，
  v0.2 在协议上加 resume 字段
- v0.1 仅 Windows 安装包；macOS / Linux 编译能过但未打包

## 设计与协议

- [架构方案](wormhole-gui-architecture.md) — 协议、状态机、IPC、安全分析、spike 验证全记录
- [spike 代码](wormhole-spike/src/main.rs) — 13 项协议层验证

## License

MIT
