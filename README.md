<p align="center"><img src="assets/logo.png" width="120" alt="Cascade"></p>

# Cascade

多项目共享的配置中心：一个配置池，多个项目自由组合归属，配置沿 `base → 环境 → 项目` 级联覆盖，再编译成各家工具方言。Rust 单二进制，本地跑、服务端部署都行。CLI 是 `cascade`，vault 在 `~/.cascade`（`$CASCADE_VAULT` 可覆盖，兼容旧 `~/.cc` 自动回退）。

```
配置池 ──任意组合──▶ 项目 × 环境 ──级联解析──▶ gRPC / HTTP / 配置文件
```

## 功能

| 能力 | 说明 |
|------|------|
| 配置池 + 项目组合 | 配置集中存，项目按需勾选归属——**只有挂载到项目的配置**才会出现在该项目的导出/注入/SDK 结果里（未挂载的池内配置不参与） |
| 多环境 + 继承 | base → staging → prod 链式覆盖；每个配置可在任一环境设专属值（桌面端矩阵视图），成环建库时直接拒绝 |
| 三种消费方式 | 文件导出（yaml/json/dotenv）、`cascade run` 进程注入、HTTP/gRPC + SDK（SSE 实时订阅 + 快照容灾） |
| 版本历史 | 每次变更自动记录，`history` 查、`revert` 回滚、`squash` 清历史 |
| 密钥加密 | AES-256-GCM，透明解密；导出默认打码，`--reveal` 出真值；HTTP 侧 reveal 需 admin token |
| 值溯源 | `cascade explain` 逐层展示 base → 环境 → 项目覆盖的来源与生效值 |
| 工具方言渲染 | 一份 MCP 配置渲染进 Qwen Code / Claude Code / Cursor，手写内容冲突跳过 |
| 漂移检测 | `models sync` 拉 OpenAI 兼容 `/models` 端点，只增不删补库存 |
| 导入扫描 | 扫已有 `.env`/`settings.json`/`yaml` 一键归并 |
| 服务端 | Token 鉴权（Bearer/header/query）、只读模式、SSE 变更推送、gRPC |
| 桌面端 | Tauri：配置/项目/环境管理、矩阵视图、编辑/历史/回滚/导出、复制 SDK 链接 |

## 快速开始

```bash
cargo build --release -p cc-cli -p cc-server   # 产出 target/release/cascade 与 cascade-server

cascade init
cascade set database.host localhost
cascade set database.port 5432
cascade set api.key "sk-xxx" --secret     # 自动加密落盘
cascade list
cascade get database.host
```

多环境 + 项目组合：

```bash
cascade env create prod --parent <base-id>
cascade project create my-app
cascade project add-config <project-id> <config-id> <env-id> --value "prod.db.example.com"
```

## 三种消费方式

**1. 文件导出**（本地开发/CI，运行时零依赖）

```bash
cascade export my-app -o ./config.yaml                 # 解析后的最终配置
cascade export my-app -o ./.env --format dotenv        # 密钥自动打码成 ${API_KEY}
cascade export my-app -o ./config.yaml --watch         # 轮询 2s，变了才原子重写
cascade export my-app -o ./config.yaml --reveal        # 出真值（含解密）
```

**2. 进程注入**（零侵入，项目代码不用改）

```bash
cascade run my-app -- python main.py
# database.host → DATABASE_HOST，密钥注入的是解密真值
```

**3. 在线订阅**（线上服务，改一处全生效）

```bash
cascade serve --port 7070 --open   # 拉起 cascade-server + 开状态页
```

桌面端「接入」弹窗还能一键**局域网分享**：自动选端口、生成访问令牌（默认只读，密钥打码；可勾选"允许读取密钥真值"）、探测本机局域网 IP，产出带令牌的链接和「交给 AI」提示词——同一网络下的人粘贴给 AI 即可接入。停止分享/退出应用时服务自动关闭。

```python
from cascade import ConfigCenter
client = ConfigCenter.from_url("cascade://localhost:7070/project/<id>?env=prod&token=xxx")
client.get("database.host")
```

链接在桌面端 Projects 页点复制即得。Go / TypeScript SDK 在 `sdk/` 下同名 API。

**安装 SDK**（尚未发布公共包仓库，从仓库目录装）：

```bash
pip install ./sdk/python          # Python（已验证）
npm install ./sdk/typescript      # TypeScript（已验证）
# Go：go.mod 加一行 replace github.com/configcenter/sdk-go => <repo>/sdk/go
```

**零 SDK 也能用**（任何语言，纯 HTTP + JSON）：

```bash
curl "http://localhost:7070/api/projects/<id>/resolved?env=prod" \
  -H "Authorization: Bearer $CASCADE_TOKEN"   # 本机免鉴权时省略
```

## 服务端部署

```bash
cascade-server --port 7070                            # 只绑 127.0.0.1，无需 token
cascade-server --listen 0.0.0.0 --token s3cret         # 对外必须带 token，否则拒绝启动
cascade-server --token s3cret --readonly               # 只读模式，写操作 403
```

- 鉴权：`Authorization: Bearer <token>`，或 `?token=`（SSE/EventSource 用这个）
- Token 存 `tokens` 表，支持过期时间；gRPC 同端口+1（`--grpc-port` 可改），同样走 Bearer 拦截
- HTTP 明文传输 secret 永远打码 `***`，解密只发生在 `run` / `export --reveal` 本机通道

## 密钥管理

```bash
cascade secrets init        # 生成主密钥（$CASCADE_MASTER_KEY（兼容 $CC_MASTER_KEY）→ 系统 keychain → ~/.cascade/master.key）
cascade secrets encrypt     # 把标 secret 的明文全量加密
cascade secrets ls          # encrypted / plaintext 状态一览
cascade secrets squash --yes  # 清空全部历史（旧明文不可恢复）
```

## 渲染与同步

```bash
# MCP 配置渲染进各家工具（手写条目冲突跳过，--force 接管）
cascade render --project my-app --target cursor
cascade render --project my-app --target qwen-code,claude-code  # 逗号分隔

# 模型库存漂移检测（OpenAI 兼容端点）
cascade models sync --url https://api.openai.com/v1 --dry-run
cascade models sync --url http://localhost:11434/v1
```

## CLI 一览

```
cascade init | list [--group] | get <key> [--reveal] | set <key> <v> [--secret] [--env <e>] | unset <key> [--env <e>]
cascade explain <key> [--project <p>] [--env <e>] [--reveal]   # 逐层展示来源与生效值
cascade project create|list|delete|add-config|remove-config|configs
cascade env create|list|delete
cascade export <project> [-o file] [--format yaml|json|dotenv] [--env] [--watch] [--reveal]
cascade run <project> [--env <e>] -- <cmd...>
cascade history <key> | revert <history-id>
cascade import <files...> [--group] [--dry-run]
cascade render --project <p> [--target ...] [--env] [--force]
cascade models sync --url <base> [--key] [--dry-run]
cascade secrets init|encrypt|ls|squash
cascade serve [--port] [--listen] [--token] [--readonly] [--open]
```

## 项目结构

```
crates/
  cc-core/      核心库：继承解析 / 加密 / 导出 / 渲染 / 漂移纯逻辑
  cc-store/     SQLite 存储：配置 / 项目 / 环境 / 历史 / token
  cc-cli/       cascade 命令行
  cc-server/    HTTP + SSE + gRPC 服务
  cc-desktop/   Tauri 桌面端（内嵌 cc-core，无 sidecar）
sdk/            python(cascade) / go(cascade) / typescript（复制链接即用）
proto/          gRPC 定义
```

## 状态

- 测试：`cargo test` 31 项全绿；`cargo check --workspace` 零警告
- 已知取舍：单连接串行 DB（CLI/桌面够用，server 高并发前换连接池）；`--watch` 是轮询（2s 可调）不是文件事件
