# 最小 Siphon 与公开 Octoii：拆分实施计划

2026-11-17。状态：用户已明确批准 A1 核心范围；代码迁移已实施，两个 workspace 的检查与回归测试已通过。本文替代
SIPHON_DISTRIBUTION.md 中“先完成 S1–S5，再分离 Octoii”的实施顺序；旧边界盘点仍是证据。

## 边界

Siphon 只持有核心机制：持久执行、原子提交、消息投递、等待/取消/恢复、受控存储、
机器执行、沙箱与授权约束的执行。不依赖 AI、Octoii、具体扩展或产品协议。
Octoii 持有 Agent 循环、对话及日志语义、上下文、模型与账号、标题/压缩/审核策略、
工具定义和扩展装配。持久任务的业务实现可以公开，由 Siphon 执行。

不增加第二套 Thread/Run/Job，不把 AI 数据改成无类型 JSON 来掩盖依赖，
不新增通用 Agent 框架，不改写存储和执行语义。

## A1：先迁出已有 Agent 宿主，保持行为

现有 siphon 包是混合宿主，不是已经清晰分离的内核。先将其完整迁到公开 workspace，
避免让私有代码反向调用公开 Agent 包，或为每个模型操作增加回调接口。
这是机械迁移，不代表机制与产品的分离已经完成。

- `siphon/crates/siphon/` → `libs/octoii/`，Cargo 包名为 `octoii`。
- 宿主类型 `Siphon` 改为 `Octoii`，迁移调用方，不保留旧名兼容别名。
- `apps/node` 的 Cargo 包名改为 `octoii-node`；二进制名仍为 `octoii`。
  Node 自身库的引用使用 `octoii_node`，不能与新的 Agent 库混淆。
- 保留 `siphon-runtime` 名称与实现，本批不提前创建空的 siphon 门面包。
- 从私有 workspace 删除 octoii-ai 依赖；公开 Octoii 直接依赖 octoii-ai。
- 迁移根 Cargo 依赖、扩展/Node 导入、测试导入、构建命令及依赖检查规则。
- 扩展包名本批不改；协议方法名、持久程序名、存储路径、注解键、序列化数据不因 Rust 改名而变化。

现有混合宿主中的 Beak/runtime 访问随源文件移动，是必须后续消除的既有边界债务，
不是批准新的底层访问。扩展继续使用原有受控宿主接口，不获得原始写入能力。
迁走的原 core 文件继续适用核心修改单独确认规则，不因路径变化解除约束。

### 为什么不能只修改扩展

Cargo 的 AI 依赖、模型装配、Thread 与工具执行实现就在现有 siphon 包中。
仅改扩展不能迁出这些实现，也不能消除私有 workspace 对公开 AI 的路径依赖。

### 本批需要单独确认的核心范围

附录列出的 92 个已跟踪文件全部迁移；源码及测试只做包名、宿主类型和导入引用调整，
manifest 改为继承公开 workspace。源目录迁空后删除。
此外修改 `siphon/Cargo.toml` 与 `siphon/Cargo.lock`，移除迁出的成员所需依赖。
不改 `siphon-runtime`、Beak、QuickJS 或其他处理库的实现和 manifest。
发现需要改变执行行为或扩大核心范围时，另列原因与文件确认。

非核心配套范围：根 Cargo.toml/Cargo.lock、apps/node、extensions 中的 manifest 和导入，
依赖检查、实际引用旧包名的构建/CI/桌面打包脚本、必要的快照来源元数据及架构文档。
AGENTS.md 不修改。

### 验收

1. 两个 workspace 的 Cargo metadata 均成功；私有 workspace 的依赖图不含 octoii-ai 或 Octoii。
2. 两个 workspace 的 fmt、check、Clippy、测试通过；Node 二进制构建成功。
3. 更新依赖检查：AI 不依赖内核；私有 workspace 不依赖产品；扩展不能新增 Beak/runtime 依赖。
4. 对照迁移前后确认协议、存储键、程序名与审核政策未变；不通过更新行为断言隐藏回归。
5. 单独提交迁移；报告仍未收敛的低层访问，不能将 A1 描述成完整闭源拆分。

## A1 验证记录

- 两个 workspace 的 all-target check、fmt 和 Clippy 通过；依赖边界检查通过（24 个包）。
- 私有 workspace：45 个测试通过。产品 workspace：262 个测试通过，3 个原有测试忽略。
- 初次受限环境运行无法嵌套调用 macOS sandbox-exec；在允许系统沙箱及临时端口的环境重跑后全部通过。
- 91 个 Rust 源文件与原实现按批准的标识符替换及 rustfmt 逐一比对一致；另外 1 个文件为 Cargo manifest。
- `cargo build -p octoii-node --bin octoii --locked --offline` 通过，命令名称保持 `octoii`。
- 两份 lockfile 没有新增第三方依赖版本；协议生成文件只有四处类型注释改名，声明逐字一致。
- 私有仓库尚未创建；A2 所列的受控机制边界仍未完成，不把本批迁移当成最终闭源边界。

## 后续顺序

| 阶段 | 内容 | 验收 |
| --- | --- | --- |
| A2 最小机制边界 | 以现有 runtime 为基础，收敛受控存储、机器/沙箱和授权执行；把 workspace、memory、scratchpad 等产品路径策略留在 Octoii | 上层不直接操作 Beak/runtime 内部状态；实际授权、取消和原子性保持；按文件另行确认 |
| A3 仓库迁移 | 将已收敛的 siphon workspace 迁入 devxiyang/octoii-siphon 私有仓库；产品固定提交引用 | 干净检出可构建；私有仓库没有反向路径依赖；真实任务、工具、取消及重启恢复通过 |
| A4 分发 | 验证公开接口与私有实现的二进制边界 | 公开上层无需私有源码即可构建；独立解决 ABI、产物平台和版本问题 |

处理库按实际职责归属，不因当前在 siphon/ 中就默认全部私有。
A1 后没有名为 siphon 的顶层门面包是过渡状态；A2 根据实际受控接口确定其实现，
不先创建纯转发包。A3 前必须完成边界验收，不能仅凭私有 workspace 无 AI 就迁库。
私有源码依赖是内部构建方式，不是公开可独立构建的二进制分发。

## A1 核心迁移文件清单

以下路径以迁移前为准，迁移后保持相对路径：

```text
siphon/crates/siphon/Cargo.toml
siphon/crates/siphon/src/accounts.rs
siphon/crates/siphon/src/agent.rs
siphon/crates/siphon/src/agent/compaction.rs
siphon/crates/siphon/src/agent/context.rs
siphon/crates/siphon/src/agent/front_matter.rs
siphon/crates/siphon/src/agent/gather.rs
siphon/crates/siphon/src/agent/model.rs
siphon/crates/siphon/src/agent/primitives.rs
siphon/crates/siphon/src/agent/render.rs
siphon/crates/siphon/src/agent/resume.rs
siphon/crates/siphon/src/agent/review.rs
siphon/crates/siphon/src/agent/thread.rs
siphon/crates/siphon/src/agent/title.rs
siphon/crates/siphon/src/agent/told.rs
siphon/crates/siphon/src/agent/turn.rs
siphon/crates/siphon/src/agent/waiting.rs
siphon/crates/siphon/src/agent/writable.rs
siphon/crates/siphon/src/connection.rs
siphon/crates/siphon/src/connection/calls.rs
siphon/crates/siphon/src/connection/journal.rs
siphon/crates/siphon/src/connection/machine.rs
siphon/crates/siphon/src/connection/providers.rs
siphon/crates/siphon/src/connection/subscriptions.rs
siphon/crates/siphon/src/extension.rs
siphon/crates/siphon/src/kept.rs
siphon/crates/siphon/src/lib.rs
siphon/crates/siphon/src/machine.rs
siphon/crates/siphon/src/machine/commands.rs
siphon/crates/siphon/src/machine/coprocess.rs
siphon/crates/siphon/src/machine/files.rs
siphon/crates/siphon/src/machine/namespace.rs
siphon/crates/siphon/src/machine/paths.rs
siphon/crates/siphon/src/machine/search.rs
siphon/crates/siphon/src/machine/search/grep.rs
siphon/crates/siphon/src/machine/search/walk.rs
siphon/crates/siphon/src/protocol.rs
siphon/crates/siphon/src/protocol/set.rs
siphon/crates/siphon/src/protocol/siphon.rs
siphon/crates/siphon/src/protocol/typescript.rs
siphon/crates/siphon/src/providers.rs
siphon/crates/siphon/src/reminder.rs
siphon/crates/siphon/src/settings.rs
siphon/crates/siphon/src/testing.rs
siphon/crates/siphon/src/tests.rs
siphon/crates/siphon/src/tests/accounts.rs
siphon/crates/siphon/src/tests/agent.rs
siphon/crates/siphon/src/tests/calls.rs
siphon/crates/siphon/src/tests/coding.rs
siphon/crates/siphon/src/tests/compaction.rs
siphon/crates/siphon/src/tests/connection.rs
siphon/crates/siphon/src/tests/execute.rs
siphon/crates/siphon/src/tests/files.rs
siphon/crates/siphon/src/tests/live.rs
siphon/crates/siphon/src/tests/namespace.rs
siphon/crates/siphon/src/tests/permissions.rs
siphon/crates/siphon/src/tests/render.rs
siphon/crates/siphon/src/tests/settings.rs
siphon/crates/siphon/src/tests/threads.rs
siphon/crates/siphon/src/threads.rs
siphon/crates/siphon/src/threads/config.rs
siphon/crates/siphon/src/threads/files.rs
siphon/crates/siphon/src/threads/history.rs
siphon/crates/siphon/src/threads/idempotency.rs
siphon/crates/siphon/src/threads/inbox.rs
siphon/crates/siphon/src/threads/input.rs
siphon/crates/siphon/src/threads/log.rs
siphon/crates/siphon/src/threads/marks.rs
siphon/crates/siphon/src/threads/operations.rs
siphon/crates/siphon/src/threads/start.rs
siphon/crates/siphon/src/threads/status.rs
siphon/crates/siphon/src/threads/thread.rs
siphon/crates/siphon/src/threads/window.rs
siphon/crates/siphon/src/tools.rs
siphon/crates/siphon/src/tools/calls.rs
siphon/crates/siphon/src/tools/coding.rs
siphon/crates/siphon/src/tools/declarations.rs
siphon/crates/siphon/src/tools/effect.rs
siphon/crates/siphon/src/tools/execute.rs
siphon/crates/siphon/src/tools/outlet.rs
siphon/crates/siphon/src/tools/permissions.rs
siphon/crates/siphon/src/tools/plan.rs
siphon/crates/siphon/src/tools/projects.rs
siphon/crates/siphon/src/tools/requests.rs
siphon/crates/siphon/src/tools/shell.rs
siphon/crates/siphon/src/tools/told.rs
siphon/crates/siphon/src/tools/tool.rs
siphon/crates/siphon/src/trust.rs
siphon/crates/siphon/src/volume.rs
siphon/crates/siphon/tests/kept.rs
siphon/crates/siphon/tests/machine.rs
siphon/crates/siphon/tests/messages.rs
```
