# chat_one — wormhole-gui

基于 [magic-wormhole.rs](https://github.com/magic-wormhole/magic-wormhole.rs) 的 Windows 端会话式 GUI 文件传输工具。

> **状态**：v0.1 开发中（Phase 0 完成）。

## 仓库结构

```
chat_one/
├── wormhole-gui-architecture.md   架构方案 + Tier 1/1.5 spike 验证记录
├── wormhole-spike/                 协议层验证 spike（13 项已 PASS）
│   ├── src/main.rs                 T1.1–T1.12 集成在一个 binary
│   └── src/bridge.rs               T1.13 tokio↔smol 桥接验证
└── wormhole-gui/                   v0.1 工程实现
    ├── Cargo.toml                  workspace
    ├── core/                       协议层 / session 状态机 (smol)
    └── tauri-app/                  Tauri 2 + Svelte 桌面应用
        ├── src/                    前端 (Svelte)
        └── src-tauri/              Rust host (tokio main)
```

## 开发

要求：Rust 1.92+，Node 20+，pnpm 9+，Windows 10/11（带 WebView2）。

```bash
# 后端单元测试
cd wormhole-gui
cargo test --workspace

# 前端依赖（首次）
cd tauri-app
pnpm install

# 启动开发模式（自动热重载）
pnpm tauri:dev

# 打包发布版
pnpm tauri:build
```

## 设计文档

- [架构方案](wormhole-gui-architecture.md) — 协议、状态机、IPC、安全分析、spike 验证全记录
- [spike 代码](wormhole-spike/src/main.rs) — 13 项协议层验证

## 安全使用建议

参考架构文档 §12.5。要点：
1. 短码必须通过和你信任对方的渠道告知（电话、当面、Signal）
2. 不要在同一个微信/邮件里既发短码又发内容
3. 连接后请务必和对方核对 4 位 SAS 验证码
4. 收到的文件请先看清楚再打开，特别是 .exe .bat .msi
5. 完成后双方关闭软件，会话密钥才会从内存清除

## License

MIT
