# Wormhole-GUI 架构方案（v0.1）

> 基于 magic-wormhole.rs 的 Windows 端会话式 GUI 文件传输工具
>
> 整理自 2026-04-28 与 lemon 的讨论；SAS 验证码已确认为 v0.1 必需特性。
>
> **2026-04-28 Tier 1 spike 已通过**（[wormhole-spike/](wormhole-spike/)）：mailbox 持续会话、长时间多轮收发、单 wormhole 并发收发三项均验证通过。本文已据 spike 实测结果修订。关键约束：后端用 smol 而非 tokio；`Wormhole` 不能用 `Arc<Mutex>` 包裹，必须用单 task event loop；`transfer::send_file` 会消耗 Wormhole 因此 v0.1 必须自建文件协议。

---

## 目录

1. [需求与背景](#1-需求与背景)
2. [为什么选 magic-wormhole + 自建 GUI](#2-为什么选-magic-wormhole--自建-gui)
3. [整体架构](#3-整体架构)
4. [技术栈](#4-技术栈)
5. [应用层消息协议](#5-应用层消息协议)
6. [Tauri IPC 接口](#6-tauri-ipc-接口)
7. [核心状态机](#7-核心状态机)
8. [项目结构](#8-项目结构)
9. [关键设计决策](#9-关键设计决策)
10. [短码生命周期与第三方接入](#10-短码生命周期与第三方接入)
11. [SAS 验证码（v0.1 必需）](#11-sas-验证码v01-必需)
12. [安全性分析](#12-安全性分析)
13. [构建与发布](#13-构建与发布)
14. [里程碑](#14-里程碑)
15. 附录 A：Tier 1 Spike 验证记录

---

## 1. 需求与背景

### 1.1 用户需求

- 跨网络（非局域网）传输文字、图片、任意文件
- Windows 桌面 GUI（不接受纯 CLI）
- 单次会话内可双向、多次发送各种类型，UI 形成时间线列表
- 单次会话使用一个短码可接受
- 内容传输安全，不被服务器或中间人偷窥

### 1.2 已排除的方案

| 方案 | 不合适原因 |
|---|---|
| Warp（GNOME GUI） | 仅 Linux，Windows 无官方安装路径 |
| wormhole-rs CLI | 用户不接受命令行 |
| wormhole-william GUI | 项目实际未提供 Windows GUI |
| LocalSend | 仅同局域网 |
| wormhole.app（浏览器） | 异步模型，发链接对方再下，非实时会话 |
| Bitwarden Send | 同上，异步链接 |
| OnionShare | 走 Tor 速度慢，需要双方都装 |
| Signal/WhatsApp | 需要绑定持久身份（手机号） |
| 微信、Telegram 普通聊天 | 内容服务器可见，安全性不达标 |

### 1.3 选定方向

**自建 Tauri + magic-wormhole.rs 的会话式 GUI**：
- 协议层用 magic-wormhole（成熟、E2E 加密、PAKE 短码）
- UI 层做 Warp Linux 那种"短码连上后持续会话 + 双向多次收发"
- 打包成 Windows .msi，跨平台代码可复用 Mac/Linux

---

## 2. 为什么选 magic-wormhole + 自建 GUI

### 2.1 协议优势

- **PAKE (SPAKE2)**：用短密码（4-6 个单词）安全协商出 256-bit 强密钥
- **NaCl 加密**：和 Signal 同等密码学强度（XSalsa20 + Poly1305）
- **前向保密**：每次会话独立密钥
- **零持久身份**：不绑手机号、不绑邮箱、不暴露任何长期标识
- **服务器零信任**：mailbox 和 transit relay 都看不到内容

### 2.2 GUI 自建的理由

magic-wormhole 协议**本身就是双向通道**：

```
建立连接（用一次短码）
        ↓
   ┌────────────┐
   │ Wormhole   │  ← 双向加密 mailbox（控制 + 文字）
   │ (E2E)      │
   ├────────────┤
   │ Transit    │  ← 双向加密通道（文件字节流）
   └────────────┘
        ↓
任意一方都可以随时发任意东西
```

官方 CLI 是"一次性 send/receive"的 UX 约定，不是协议限制。Warp（Linux GUI）已经实现了"短码连上后持续会话"的模型，B 方案只是把这个搬到 Windows。

---

## 3. 整体架构

```
┌──────────────────────────────────────────┐
│  前端 UI（HTML + JS / Svelte）           │
│  - 时间线列表 / 输入框 / 短码显示        │
│  - SAS 验证码核对界面                    │
└────────────┬─────────────────────────────┘
             │ Tauri IPC（命令 + 事件）
┌────────────┴─────────────────────────────┐
│  Tauri 应用层（Rust）                    │
│  - commands.rs：命令入口                 │
│  - events.rs：事件推送                   │
└────────────┬─────────────────────────────┘
             │ 调用
┌────────────┴─────────────────────────────┐
│  Core 业务层（Rust）                     │
│  - session：会话生命周期 + SAS           │
│  - protocol：自定义应用消息格式          │
│  - transfer：文件收发与进度              │
│  - storage：落盘 / 默认下载路径          │
└────────────┬─────────────────────────────┘
             │ 调用
┌────────────┴─────────────────────────────┐
│  magic-wormhole crate                    │
│  - Wormhole（mailbox 双向加密通道）      │
│  - transit（文件字节流加密通道）         │
└──────────────────────────────────────────┘
```

---

## 4. 技术栈

| 层 | 选型 | 理由 |
|---|---|---|
| 桌面壳 | **Tauri 2** | 包小（5-15MB）、用系统 WebView2、Rust 原生 |
| 后端运行时 | **smol（async-io）** ⚠️ | magic-wormhole 0.8 内部用 async-tungstenite + smol-runtime，**不能用 tokio 直接驱动** |
| 协议 | **magic-wormhole 0.8** | 官方 crate（spike 已验证可用） |
| 前端 | **Svelte 或 vanilla JS** | 轻量、避免 React 全家桶 |
| 序列化 | **serde + JSON** | 应用层消息格式 |

**Tauri ↔ smol 的桥接**：Tauri 自身基于 tokio。最干净的做法是在 Tauri 启动时 spawn 一个独立 OS 线程跑 `smol::block_on`，所有 wormhole 调用走 channel 进出该线程；不要把两个 runtime 混着用。

---

## 5. 应用层消息协议

在 mailbox（已加密）通道上跑 JSON 消息：

```jsonc
// 文字
{ "v": 1, "type": "text", "id": "<uuid>", "content": "你好", "ts": 1714290000 }

// 文件提议
{ "v": 1, "type": "file_offer", "id": "<uuid>", "name": "图.png", "size": 102400, "mime": "image/png" }

// 接受/拒绝
{ "v": 1, "type": "file_accept", "id": "<uuid>" }
{ "v": 1, "type": "file_reject", "id": "<uuid>", "reason": "user_cancel" }

// 完成确认
{ "v": 1, "type": "file_done", "id": "<uuid>", "ok": true }

// 控制
{ "v": 1, "type": "ping" }
{ "v": 1, "type": "bye" }
```

文件字节流走 **transit 子通道**，不挤 mailbox。

**版本字段 `v` 必须严格校验**：拒绝未知版本，防止协议混淆攻击。

**mailbox 单条消息客户端 size cap = 1 MB**（T1.5 实测：4 MB 通过、16 MB 服务器断连）。所有上述消息加起来都远小于 64 KB，但 `text.content` 字段必须在 send 前检查长度——超过 64 KB 的文字走 transit（额外消息类型 `long_text_offer`），不走 mailbox。

**v0.1 不使用 `magic_wormhole::transfer` 模块**：高层 `transfer::send_file` / `transfer::request_file` 接收 `Wormhole` 按值传入，调用一次即消耗会话。v0.1 在 mailbox 上自建上述协议，文件字节流走 `transit::init` + `TransitConnector::connect`（这两个 API 与 Wormhole 解耦，可在同一会话内多次调用）。

**SAS 双方确认必须通过应用层握手**。补充协议消息：

```jsonc
{ "v": 1, "type": "sas_ok" }                                    // 本端用户已点"一致"
{ "v": 1, "type": "sas_reject", "reason": "user_mismatch" }     // 本端用户已点"不一致"
```

只有双方都 `sas_ok` 后才转 [Connected]，期间不允许 send_text / send_file。

---

## 6. Tauri IPC 接口

### 6.1 命令（前端 → 后端）

```rust
// src-tauri/src/commands.rs
#[tauri::command] async fn start_session(mode: SessionMode, code: Option<String>) -> Result<String>
#[tauri::command] async fn confirm_sas(matches: bool) -> Result<()>           // SAS 核对结果
#[tauri::command] async fn send_text(content: String) -> Result<String>
#[tauri::command] async fn send_file(path: String) -> Result<String>
#[tauri::command] async fn accept_file(offer_id: String, save_path: Option<String>) -> Result<()>
#[tauri::command] async fn reject_file(offer_id: String) -> Result<()>
#[tauri::command] async fn cancel_file(transfer_id: String) -> Result<()>   // 进行中取消
#[tauri::command] async fn close_session() -> Result<()>
```

### 6.2 事件（后端 → 前端）

```
"session:code"            { code: "26-dinosaur-spaniel" }    // 立即发，先于 PAKE 完成
"session:sas_ready"       { sas: "1234" }                     // PAKE 完成（双方都连入）后发
"session:connected"       {}                                  // 双方 sas_ok 后进入
"session:closed"          { reason }
"msg:text"                { id, from, content, ts }
"msg:file_offer"          { id, from, name, size, mime }
"file:progress"           { id, bytes, total, dir: "in"|"out" }
"file:done"               { id, path?, ok }
"file:cancelled"          { id, by: "self"|"peer" }
"error"                   { code, message }
```

---

## 7. 核心状态机

```
[Idle] ──start_session(send)──→ [Connecting]
[Idle] ──start_session(recv,code)──→ [Connecting]
                                          │
                                  PAKE 完成 │
                                          ↓
                                    [SasPending] ★ 新增状态
                                          │
                              双方都 confirm_sas(true)
                                          ↓
                                    [Connected]──┐
                                          │      │
                            send_text/    │      │ recv loop
                            send_file ───→│      ├──→ msg:text
                            accept_file   │      ├──→ msg:file_offer
                                          │      ├──→ file:progress
                                          ↓      └──→ file:done
                                    [Closing]
                                          │
                                          ↓
                                       [Idle]
```

**SasPending 状态规则**：
- 任一方 `confirm_sas(false)` → 立刻进入 [Closing]，session 销毁
- SasPending 超过 60 秒未确认 → 自动 [Closing]
- SasPending 期间禁止任何 send/accept 操作

### 7.1 会话由单 task 拥有 Wormhole（不能用 Mutex）

**Spike 实测结论**：`Arc<Mutex<Wormhole>>` 模式会死锁。`receive_json().await` 在等待下一条 mailbox 消息时一直持有 mutex，`send_json` 永远抢不到锁。`Wormhole` 0.8 也不提供 `split()`。

正确架构：

```
┌──────────────────────────────────────────────────┐
│  Tauri 主线程（tokio）                            │
│  IPC commands → cmd_tx                            │
│  events ← evt_rx                                  │
└────────────┬──────────────────────────────────────┘
             │ async-channel
┌────────────┴──────────────────────────────────────┐
│  会话线程（独立 OS thread, smol::block_on）       │
│                                                   │
│  loop {                                           │
│    select! {                                      │
│      cmd = cmd_rx.recv()  => handle_cmd(&mut wh), │  ← UI 触发的 send / accept / close
│      msg = wh.receive()   => handle_peer(&mut wh) │  ← 对方推过来的 mailbox 消息
│      _   = transit_evt    => 更新进度,            │  ← 进行中文件传输的进度回调
│    }                                              │
│  }                                                │
└───────────────────────────────────────────────────┘
```

`Wormhole` 永远只被这一个 task 持有；UI 侧通过 channel 发命令进、收事件出。文件传输用 transit 时，也是从这个 loop 里 spawn 子 task 持有 `Transit`，走另一组 channel 汇报进度。

---

## 8. 项目结构

```
wormhole-gui/
├── Cargo.toml                       # workspace
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       ├── main.rs                  # 启动 Tauri
│       ├── commands.rs              # IPC 命令入口
│       ├── events.rs                # 事件发射器
│       ├── error.rs                 # 错误类型
│       └── core/
│           ├── mod.rs
│           ├── session.rs           # Session 状态机 + SAS
│           ├── protocol.rs          # 应用消息编解码 + 版本校验
│           ├── transfer.rs          # 文件收发 + 进度
│           └── storage.rs           # 路径净化 / 落盘
└── src/                             # 前端
    ├── index.html
    ├── main.js (or App.svelte)
    ├── components/
    │   ├── CodeBanner.*             # 顶部短码区
    │   ├── SasDialog.*              # ★ SAS 核对弹窗
    │   ├── Timeline.*               # 中间时间线
    │   └── Composer.*               # 底部输入 + 拖拽
    ├── stores.js                    # 全局状态（消息列表）
    └── styles.css
```

---

## 9. 关键设计决策

| 决策 | 选择 | 理由 |
|---|---|---|
| 会话只支持 1v1 | ✅ | 短码协议本就是双方协商，多人需要另一套架构 |
| 文件并发数 | **每方向 1，总并发 2** | 维持双向对称体验；v0.2 再放开 |
| 自动接收策略 | 默认弹确认 | 安全优先，可在设置里改"自动接收 < 10MB" |
| 关闭策略 | 任一方关闭即结束 | 短码已作废，无法重连 |
| 历史记录 | 仅当前会话内存 | v0.1 不持久化，关闭即丢 |
| 重连 | 不支持 | magic-wormhole 协议本身不支持，要重连只能新短码 |
| **SAS 验证码** | **强制开启** | **v0.1 必需，防止抢码攻击** |
| 短码 TTL | 5 分钟 | 客户端定时器，到点 abort（mailbox 服务器自身超时更长） |
| **会话并发模型** | **单 task event loop + select** | spike 实测 Mutex<Wormhole> 死锁；Wormhole 不提供 split() |
| **transfer 模块** | **不使用** | 高层 API 消耗 Wormhole，与持续会话不兼容；v0.1 自建 mailbox 协议 + 手动 transit |
| **异步运行时** | **smol（会话线程）+ tokio（Tauri 主线程）** | magic-wormhole 0.8 绑定 smol-runtime，不能改 |

---

## 10. 短码生命周期与第三方接入

### 10.1 生命周期

```
[发起方生成短码] 
       ↓
mailbox 服务器分配 nameplate (短码前缀的数字)
       ↓
[等待对方输入短码连接...]
       ↓
[对方输入短码 → SPAKE2 双向握手]
       ↓
握手成功 → nameplate 立刻释放回池子   ★关键
       ↓
[SAS 核对 → 双方确认]
       ↓
[两人在专属 mailbox 通信，第三方进不来]
       ↓
任一方关闭 → mailbox 销毁，密钥从内存清除
```

### 10.2 第三方在不同时点尝试连接

| 时机 | 结果 |
|---|---|
| **握手前**（短码已生成，对方还没输入） | ⚠️ 谁先输完 SPAKE2 谁连上——抢码风险窗口 |
| **握手中**（对方正在握手） | 第三方的 PAKE 消息会和对方的混在一起，**双方密钥都对不上**，连接失败 |
| **握手成功后** | ✅ 连不上——nameplate 已释放，第三方查询会得到"no such nameplate" |
| **会话结束后** | ✅ 连不上——mailbox 已销毁 |

**结论**：握手成功 = 短码作废 + mailbox 私有化，第三方无法接入。

唯一风险窗口是"握手前的抢码"——这就是 SAS 验证码要解决的问题。

### 10.3 PAKE 时序约束（spike 踩到的坑）

`Wormhole::connect(mc)` 在加入方接入之前不会返回。但 code 在 `MailboxConnection::create` 返回时就已生成。**正确顺序**：

```
1. mc = MailboxConnection::create(...)        // 立即返回，code 在 mc.code() 里
2. emit "session:code"                          // UI 立刻显示 code
3. wh = Wormhole::connect(mc).await             // 阻塞等加入方
4. emit "session:sas_ready"                     // PAKE 完成
```

错误顺序"先 await `Wormhole::connect` 再发 code"会自锁——加入方拿不到 code 就不会接入，PAKE 永远不完成。Spike 第一版就栽在这上面。

---

## 11. SAS 验证码（v0.1 必需）

### 11.1 是什么

SAS = Short Authentication String。从 PAKE 协商出的对称密钥**衍生出一段短字符串**（如 4 位数字 `1234` 或 emoji 序列），双方核对一致才能进入正式会话。

### 11.2 为什么必需

**SAS 是抗"抢码攻击"的最后一道防线**：

```
正常情况:
  你 ←PAKE→ 对方
  你的 SAS == 对方的 SAS == 1234   ✅ 一致 → 进入会话

抢码攻击:
  你 ←PAKE→ 攻击者（抢先输入了短码）
  对方 ←PAKE→ ??? （无法连接，因为攻击者已占用）
  
  你的 SAS = 5678（和攻击者协商出的）
  对方拿不到任何 SAS（连不上）
  → 你和对方电话核对 SAS：发现你说 5678、对方根本没收到 → 立即断开
```

即使攻击者实时拿到短码、抢先连上，**他无法伪造对方的 SAS**——因为 SAS 是从 PAKE 密钥派生的，攻击者和你协商出的密钥与攻击者和对方协商出的密钥不可能相同。

Signal、WhatsApp、Wire 都有同类机制（Signal 叫"安全数字"，WhatsApp 叫"安全码"）。

### 11.3 实现要点（已用 spike 验证）

magic-wormhole 0.8 在 `Wormhole::verifier()` 已经暴露双方独立计算的 32 字节 verifier hash——这就是协议层抗 MITM 的依据，第三方插入则双方算出的不同。SAS 直接基于它派生：

```rust
// core/session.rs
use magic_wormhole::Wormhole;

pub fn derive_sas(wh: &Wormhole) -> String {
    let v = wh.verifier();         // &crypto_secretbox::Key (32 bytes)
    let bytes = v.as_slice();
    let num = u16::from_be_bytes([bytes[0], bytes[1]]);
    format!("{:04}", num % 10000)  // 4 位数字 0000-9999
}
```

Spike 实测两端 `verifier()` 前 4 字节相等（`c5 40 96 9f`），方案可行。

### 11.4 UI 流程

```
1. PAKE 完成后立即在两端各显示同一段 SAS（如 "1234"）
2. 弹窗提示：
   ┌────────────────────────────────────────┐
   │  ⚠️ 请通过电话/当面与对方核对验证码    │
   │                                        │
   │           1 2 3 4                      │
   │                                        │
   │  对方看到的应该是相同的 4 位数字       │
   │                                        │
   │   [一致，继续]    [不一致，断开]       │
   └────────────────────────────────────────┘
3. 双方都点"一致" → 进入会话
4. 任一方点"不一致" → 立即销毁会话，提示重新生成短码
5. 60 秒未操作 → 自动断开
```

### 11.5 SAS 选型

| 选型 | 长度 | 优势 | 劣势 |
|---|---|---|---|
| 4 位数字 | 10000 种组合 | 简单好读 | 攻击者 1/10000 蒙对几率 |
| 6 位数字 | 1M 种组合 | 更安全 | 略繁琐 |
| Emoji 序列 | 4 个 emoji | 视觉直观、不易听错 | 需要双方 OS 支持相同 emoji |
| Word 序列 | 2-3 个单词 | 电话易读 | 国际化复杂 |

**v0.1 推荐：4 位数字**——平衡简单和安全。可在设置里允许切换 6 位数字。

---

## 12. 安全性分析

### 12.1 加密链路（5 层防护）

```
1. SPAKE2 (PAKE)：短码 → 256 位强密钥
2. NaCl SecretBox：mailbox 双向加密通道（XSalsa20 + Poly1305）
3. Transit Encryption：文件字节流加密（同上）
4. Forward Secrecy：每次会话独立密钥
5. 完整性认证：每条消息带 MAC
```

### 12.2 各角色可见内容

| 谁 | 能看到 | 看不到 |
|---|---|---|
| 正常接收方 | 你发的所有内容 ✅ | / |
| mailbox 服务器 | 加密包大小、双方 IP、连接时间 | 文字内容、文件内容、密钥 |
| transit relay 服务器 | 加密包大小、双方 IP、传输时长 | 文件内容、密钥 |
| 网络中间人（ISP/WiFi） | TLS 内的加密流量 | 一切实际内容 |
| 抢码攻击者 | 短码失败的连接尝试 | 无内容（被 SAS 阻断） |

### 12.3 协议已解决的威胁

| 威胁 | 防御机制 |
|---|---|
| 服务器监听内容 | E2E 加密，服务器只见密文 |
| 网络中间人嗅探 | 全程加密 + MAC |
| 服务器篡改流量 | MAC 检测，篡改即断 |
| 重放攻击 | 每会话 nonce + 一次性 nameplate |
| 离线穷举短码 | 协议设计禁止离线攻击 |
| **抢码冒充** | **SAS 验证码（v0.1 必需）** |

### 12.4 应用层（B 方案 GUI）需额外注意

| 威胁 | 缓解策略 |
|---|---|
| **短码在传递中被截获** | 用比传输内容更可信的渠道传短码（电话、Signal、当面）；**绝对不要同一渠道既发短码又发内容** |
| **路径遍历**（对方文件名带 `../`） | `storage.rs` 必须 sanitize_filename，去掉路径分隔符 |
| **大小欺诈** | 接收时按 streaming 写入，超过声明 size 立即中止 |
| **MIME 欺骗** | 永不根据 mime 自动执行；`.exe/.bat/.msi` UI 标红警告 |
| **应用消息字段污染** | 严格校验 JSON schema 和 `v` 字段，未知 type 拒绝 |
| **会话内存泄露** | 关闭时清零密钥缓冲 |
| **GUI 二进制被替换** | 官方 GitHub Release + SHA256 + 代码签名 |

### 12.5 GUI 内置安全提示（给用户看）

```
🔒 安全使用建议
1. 短码请通过和你信任对方的渠道告知（电话、当面、Signal）
2. 不要在同一个微信/邮件里既发短码又发内容
3. 连接后请务必和对方核对 4 位验证码
4. 收到的文件请先看清楚再打开，特别是 .exe .bat .msi
5. 如果短码连接超过 5 分钟没人接，请关闭重发
6. 完成后双方关闭软件，会话密钥才会从内存清除
```

### 12.6 与其他方案对比

| 方案 | 内容加密 | 元数据可见 | 短码风险 | 持久身份风险 |
|---|---|---|---|---|
| **B 方案 GUI（magic-wormhole + SAS）** | E2E 强加密 | 服务器看到 IP/大小 | ✅ SAS 阻断抢码 | ✅ 无 |
| Signal | E2E 强加密 | 元数据收集（争议） | ✅ 无短码 | ⚠️ 绑手机号 |
| WhatsApp | E2E 强加密 | Meta 收集元数据 | ✅ 无短码 | ⚠️ 绑手机号 + Meta |
| 微信 | ❌ 服务器可见 | 全可见 | / | / |
| Telegram 普通聊天 | 仅传输加密 | 服务器可见 | / | / |
| LocalSend（同 LAN） | E2E 加密 | 仅同 LAN 可见 | ✅ 无 | ✅ 无 |
| wormhole.app（异步） | E2E 加密 | 服务器看到密文 | / | / |

---

## 13. 构建与发布

```bash
# 开发
cd wormhole-gui
pnpm install                # 前端依赖
cargo tauri dev             # 开发热重载

# Windows 打包
cargo tauri build           # 输出 src-tauri/target/release/bundle/msi/*.msi

# 签名（推荐，避免 SmartScreen）
signtool sign /a /tr http://timestamp.digicert.com /td sha256 /fd sha256 *.msi
```

无证书时可自签 + 用户首次手动允许。

---

## 14. 里程碑

| 版本 | 范围 | 估时 |
|---|---|---|
| **v0.1** | 单文件 + 单文字、双向、SAS 验证、Windows .msi | 1 周 |
| v0.2 | 拖拽 / 剪贴板贴图 / 进度条打磨 / 多文件并发 | 2-3 天 |
| v0.3 | 设置页 / 自动接收策略 / 历史持久化 | 3-5 天 |
| v1.0 | Mac/Linux 打包 / 代码签名 / 完整 README + 文档 | 1 周 |

### v0.1 验收标准

- [ ] Windows 上能装、能跑、显示短码
- [ ] 一端输入短码后，**双方都看到 SAS 4 位数字**
- [ ] 双方点击"一致"后才进入会话
- [ ] 任一方点击"不一致"立即关闭
- [ ] 会话内可双向发送文字、文件
- [ ] 文件落盘到指定目录
- [ ] 任一方关闭程序会话即结束

---

## 附录 A：Tier 1 Spike 验证记录（2026-04-28）

代码：[wormhole-spike/](wormhole-spike/)，依赖 `magic-wormhole = "0.8"`，运行环境 Windows 11 + Rust 1.95。

| 项目 | 设计 | 结果 | 实测耗时 |
|---|---|---|---|
| **T1.1** 持续 mailbox | A/B 各 5 次 ping/pong + text 三回合，验证 wormhole 不被 crate 自动关闭，verifier 双方一致 | ✅ PASS | 6.6s |
| **T1.2** 多轮 + 大消息 | 25 轮 × 4KB JSON 消息（模拟连续多次 file_offer 的 mailbox 负载） | ✅ PASS | 13.1s |
| **T1.3** 并发收发 v1 | `Arc<Mutex<Wormhole>>` 包裹，send/recv 各一 task | ❌ DEADLOCK（4 分钟仅交换 1 条/方向） |
| **T1.3** 并发收发 v2 | 单 task `select! { cmd_rx, wh.receive }` | ✅ PASS | 4.7s |
| **T1.4** 真 transit 文件传输 | 同一 Wormhole 上连续 3 轮 transit，1KB / 1MB / 100MB；自建 transit_handshake JSON 协议 + `transit::init` + `TransitConnector::connect` | ✅ PASS | 总 6.87s |
| **T1.5** mailbox 单消息上限 | 指数倍增（1KB→16MB），公共 relay | 4 MB ✅ / 16 MB ❌ 服务器断连 | 总 43.8s |
| **T1.6** transit drop / cancel 后 wormhole 存活 | 16MB 传至 50% 接收方 abort，再 send_json 验证 | ✅ wormhole 仍可用 | 4s |
| **T1.7** 反向并发 transit | A→B 8MB + B→A 8MB 同时 | ✅ 128ms 跑完 | 4.5s |
| **T1.8** mailbox idle | 静默 5 分钟后 round-trip | ✅ 383ms | 300s |
| **T1.9** PAKE 失败 | 加入方用错误密码 | ✅ 双方 `WormholeError::PakeFailed` 干净返回 | <1s |
| **T1.10** 长跑稳定 | 10 × 1MB transit + 12s gap | ✅ 131s 内全部成功，吞吐 113-137 MB/s | 131s |
| **T1.11** 流式内存稳定 | 5/50/500MB transit | ✅ RSS 全程 15.7-15.8 MB，**与文件大小无关** | 5s |
| **T1.12** Unicode | 中文文件名 + emoji + 数学符号 round-trip | ✅ 端到端无损 | <1s |
| **T1.13** tokio + smol 双 runtime 桥接 | `#[tokio::main]` 主线程 + 两条 `smol::block_on` 子线程，async-channel 双向通讯，做完整 PAKE + 双向消息 | ✅ 全程通过 | ~6s |

由 Tier 1.5 spike 得出的关键架构结论：

- **断点续传可行**（T1.6）：transit TCP 失效后 wormhole 仍存活；v0.1 协议预留 `resume_request` / `resume_offer` 消息字段，v0.2 实现。
- **反向并发实测通过**（T1.7）：决策表"每方向并发 1，总 2"经实测，无需特殊调度。
- **mailbox 5 分钟 idle 无需心跳**（T1.8）：v0.1 不实现 keepalive；30 分钟以上会话再考虑。
- **PAKE 失败干净可恢复**（T1.9）：UI 可直接根据 `PakeFailed` 显示"短码不正确"。
- **流式 API 验证 100% 流式**（T1.11）：500MB 传输 RSS Δ ≈ 0.1 MB，文件大小**不是**内存上限因素；理论无界。
- **i18n 零成本**（T1.12）：UTF-8 文件名/文字端到端无损，无需特殊编码处理。
- **tokio + smol 双 runtime 实战可行**（T1.13）：tokio 主线程 + 独立 OS 线程跑 `smol::block_on(session_loop)`，`async-channel` 在两边都能 await，跨边界传 `Code` / 命令 / 事件均无阻塞；tokio 自身的 timer/scheduler 不受影响。Tauri 2 的 main 即 tokio，因此该结构可直接迁移：把 `#[tokio::main]` 换成 `tauri::Builder::default().setup`，把 `println!` 换成 `app.emit_all`。代码：[wormhole-spike/src/bridge.rs](wormhole-spike/src/bridge.rs)。

**T1.4 实测吞吐**（同机直连，IPv6 loopback）：

| 大小 | Transit 建立 | 传输 | 吞吐 |
|---|---|---|---|
| 1 KB | 828 ms | 142 µs | 7.2 MB/s |
| 1 MB | 827 ms | 8.2 ms | 127 MB/s |
| 100 MB | 823 ms | 805 ms | **130 MB/s** |

观察：
- **同一 Wormhole 多次 transit 完全可行**（3 条独立 TCP 连接共享一个 wormhole）。
- **每次 transit 建立稳定开销 ~820ms**（STUN + 握手）：小文件批量发送应在 v0.2 考虑合并 transit。
- **流式收发，内存与文件大小无关**：100MB 进程 RSS 仅几十 MB。
- 跨 NAT 走公共 relay 时实际带宽通常 1–5 MB/s（未在本 spike 实测）。

**最大可发文件大小（v0.1 文档建议值）**：
- 直连：≤ 5 GB（耐心 / 磁盘边界）
- 走 relay：≤ 500 MB（断线无续传，超过越大越脆弱）
- 协议硬上限：u64，事实无界

**T1.5 mailbox 单条消息上限**（公共 relay，实测）：

| 大小 | 往返 | 结果 |
|---|---|---|
| 1 KB | 383 ms | ✅ |
| 64 KB | 1.89 s | ✅ |
| 1 MB | 3.92 s | ✅ |
| **4 MB** | **12.78 s** | ✅ |
| **16 MB** | — | ❌ 服务器直接断连（wormhole 报废） |

结论：
- **上限在 (4 MB, 16 MB]**，且失败方式是 wormhole 被断、必须从头重建短码。
- mailbox 吞吐 ~0.66 MB/s（远低于 transit 的 130 MB/s），且每消息 ~380ms 基线 RTT。
- v0.1 应用协议据此约束：**任何 > 64 KB 的 payload 必须走 transit**，mailbox 只承载控制信令（file_offer / sas_ok / accept 等，单条 < 1 KB）。文字消息超过 64 KB 也走 transit（新增 "long_text" 路径）。
- 客户端在 send_json 前自检 size，> 1 MB 直接拒绝避免触发服务器断连。

由 spike 得出且**已写回本文**的修订：

1. 后端运行时必须是 smol，不是 tokio（第 4 节）
2. `transfer::send_file` 消耗 Wormhole，v0.1 不能用（第 5 节、第 9 节）
3. 会话状态用单 task event loop，禁止 Mutex<Wormhole>（第 7.1 节、第 9 节）
4. SAS 用 `Wormhole::verifier()` 直接派生（第 11.3 节）
5. PAKE 时序：先发 code 再 await connect（第 10.3 节）
6. 应用协议补 `sas_ok` / `sas_reject`、IPC 补 `cancel_file` / `file:cancelled`
7. T1.4 验证同一 Wormhole 上多次 transit 端到端可行；transit_key 派生用 `wh.key().derive_subkey_from_purpose::<TransitKey>("{appid}/transit-key")`（`derive_transit_key` 是 pub(crate)）

---

## 附录 B：关键参考资料

- magic-wormhole.rs 仓库：https://github.com/magic-wormhole/magic-wormhole.rs
- magic-wormhole.rs CLI 源码（API 调用范例）：https://github.com/magic-wormhole/magic-wormhole.rs/tree/main/cli
- crate 文档：https://docs.rs/magic-wormhole
- Warp（Linux GUI 参考实现）：https://gitlab.gnome.org/World/warp
  - 重点关注 `mod transfer`、`protocol::*` 模块
- Tauri 教程：https://tauri.app/v1/guides/getting-started/setup
- magic-wormhole 协议文档：https://github.com/magic-wormhole/magic-wormhole-protocols

## 附录 C：术语表

- **PAKE (Password-Authenticated Key Exchange)**：用短密码安全协商出强密钥的密码学协议
- **SPAKE2**：magic-wormhole 用的具体 PAKE 算法
- **SAS (Short Authentication String)**：从协商密钥派生的短验证字符串，用于抗 MITM
- **Nameplate**：mailbox 服务器分配的短码前缀编号，一次性使用
- **Mailbox**：双方握手后协商的专属加密消息通道
- **Transit**：用于大文件字节流传输的加密通道
- **Forward Secrecy**：历史会话密钥泄露不影响后续会话安全

---

_最后更新：2026-04-28_
