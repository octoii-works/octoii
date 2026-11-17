# Thread 启动门：第一阶段设计

2026-11-17。第一阶段内核实现，core 文件范围及补充的停止入口均已获用户单独确认。
对应 WORKTREE_START.md 的后端启动衔接，兼顾 SIPHON_DISTRIBUTION.md 的执行与产品边界。

## 已有机制与实际缺口

- siphon-runtime 的 Program / Run / Ending / Wait 已支持 checkpoint、等待与原子提交，继续复用。
- Threads::create 会创建 Thread 并启动主进程；空 inbox 时主进程等待。
- deliver 会写 inbox 并唤醒主进程；queue 的 Joins::Turn 仅防止正在运行的 turn 被插入输入，空闲时仍立即执行。
- ThreadProgram 在 rest 发现 Input 后直接 gather，再在 begin 中取输入、启动标题任务和 turn。
- Volume 目前没有持久保存启动状态并受控放行 Thread 的公共原语；recorded 是临时命令记录，结束后清理，不能当启动回执。
- 扩展不可直接使用 runtime/Beak 或拼内部路径写入。因此不能只改 Git 扩展实现正确的启动门。

## 最小语义

不增加第二套 Run、Workspace 或通用 Job 系统。只为尚未启动的 Thread 增加可选启动状态，正常 Thread 不受影响。

1. **接受**：一次提交创建 Thread、保存首条输入和启动描述；输入可被查询，但主进程不进行 gather、标题生成、审核或 Agent 调用。
2. **准备**：产品扩展读取启动描述并准备资源。core 不解释 Git 参数；描述记录拥有该任务的扩展与其 JSON 请求。
3. **放行**：绑定最终 workspace 与项目归属、记录 ready、允许输入执行必须属于同一次状态提交。
4. **失败**：保存错误，Thread、输入和附件继续保留，禁止执行。
5. **重试**：必须是原 Thread 的新一次准备尝试；输入和已接受请求不被新客户端参数偷偷替换。
6. **取消**：禁止后续准备结果放行；文件清理由产品扩展另行处理，core 不删除工作目录。

准备状态使用明确枚举（pending、failed、cancelled、ready），不用多个互相冲突的布尔字段。
每次准备尝试带 token；放行、失败和重试按当前 token 校验，旧请求的迟到结果不能改变新尝试或取消后的状态。
已经放行的 Thread 不允许重新进入准备状态，既有 Agent 运行/停止语义不改变。

## 公共接口职责（名称在实施时保持同一组词汇）

通过 Volume 暴露以下受控操作，内部复用 Threads 和现有提交机制：

- 创建待准备 Thread：NewThread、首条输入、拥有者/请求、幂等 key → Thread ID 与尝试 token。
- 读取启动状态：返回接受的请求、状态、尝试 token、失败原因及已绑定目录；不要求扩展扫描内部存储布局。
- 完成准备：Thread ID、当前 token、实际目录 → 放行结果；重复成功返回同一结果。
- 标记准备失败：Thread ID、当前 token、原因 → 保留输入的失败状态。
- 重试准备：Thread ID、预期旧 token → 新 token；只接受 failed 状态，不用客户端盲重试替代状态转换。
- 取消准备：Thread ID、当前 token → cancelled；与 stop/delete 行为协调，迟到的完成不可复活它。

这些是内核侧 Rust 原语，不在第一阶段新增客户端 RPC，不给客户端提供任意改写启动状态的通用入口。
产品扩展的 RPC 继续按能力检查权限；Git 操作要求 git.operate，不因创建 Thread 自动获得 Git 写入权限。
启动门的单写者与 token 校验必须覆盖并发，不以“先 snapshot 再无条件 commit”的检查代替原子转换。
可以利用主进程 inbox 串行处理控制消息；具体提交方式在实现审查中核对，不能暴露裸 Changes 写接口。

## 所有权和闭源边界

- Siphon 持有输入、启动状态、token 校验与不执行的保证，未来属于闭源运行机制。
- Git 扩展解释源仓库、目标路径、分支和基准，执行 Git，并报告结果，保持产品逻辑独立。
- Node 装配与服务入口保持产品职责。第一阶段不增加启动后自动重放 Git 的后台循环。
- Octoii 在放行后沿用现有执行过程，不在 Agent prompt 中实现启动门。
- Web 展示服务端状态，浏览器快照只负责尚未确认接收的内容；现有 23 小时客户端恢复不扩展为服务端调度器。

“收到 Git 创建成功回执”与“节点重启后发现目录存在”不是同一件事。第一阶段不宣称 Git 创建具备事务性。
后续扩展在结果不确定时核查现场，不能仅凭 path/branch/HEAD 匹配就自动放行，不自动清理其他任务的目录。

## 已确认的 core 文件范围

| 文件 | 修改职责 |
| --- | --- |
| core/siphon/src/threads/start.rs（新增） | 启动状态与描述、token、内部状态转换与存储规则 |
| core/siphon/src/threads.rs | 导出启动公共类型，装配新模块 |
| core/siphon/src/threads/operations.rs | 原子接受、启动操作、与 stop/delete/configure 的协调 |
| core/siphon/src/threads/inbox.rs | 启动门关闭时不消费普通输入，控制消息由主进程独立处理 |
| core/siphon/src/threads/thread.rs | 共用 stop_turn 为待准备 Thread 排入取消消息，覆盖 Agent 工具停止入口 |
| core/siphon/src/threads/status.rs | 查询返回启动状态，明确待准备/失败不能显示为正在执行 |
| core/siphon/src/agent/thread.rs | 在 gather/开始 turn 前守住启动门，处理受控放行 |
| core/siphon/src/volume.rs | 面向扩展的上述受控接口 |
| core/siphon/src/tests/threads.rs | 接受、查询、并发转换、取消/删除、重试与幂等回归 |
| core/siphon/src/tests/agent.rs | 待准备与失败时无模型调用，放行后执行一次，普通 Thread 行为不变 |

不修改 Beak、siphon-runtime、提示词、模型适配或 core 协议权限枚举。生成的 TypeScript 若因状态类型变化需同步，
以及产品层测试和文档，均在 core 外。若实施发现需修改其他 core 文件，先补充具体范围再确认。

## 第一阶段验收

- 原子接受后重启节点，输入、请求与启动状态仍存在；待准备期间没有模型请求。
- 普通 deliver、queue、配置变更及主进程重启不能绕过启动门。
- 放行后首轮使用最终目录；失败状态保留输入，失败期间仍无 Agent 执行。
- 并发完成、重复完成、旧 token、重试后的迟到结果、取消后的迟到成功均有回归测试。
- stop/delete 在准备阶段行为明确；不会被后台准备回调复活。
- 现有普通 Thread、工具调用、委派、取消和恢复测试保持通过。

第一阶段只实现并验证内核原语，不同时迁仓库、做 FFI、改造所有 Agent 或上线 worktree UI。
随后再接 Git 扩展的准备流程和恢复入口，每阶段独立提交。

## 第一阶段实现记录

公共入口为 `Volume::prepare_thread / thread_start / finish_thread_start / fail_thread_start /
retry_thread_start / cancel_thread_start`；查询的 `Status.start` 对普通 Thread 为空。

- 接受提交同时写入 Thread、首条输入、启动状态和密封幂等记录。记录保留请求摘要与 Thread ID，
  不额外保留原始输入；没有 24 小时过期窗口，删除 Thread 后重放也不能重新创建。
- 主进程串行处理持久控制消息，状态转换、最终目录配置、配置日志与操作回执在同一提交生效。
  消费控制消息后等待目录版本变化，再从已提交状态继续；不新增 runtime 程序或 checkpoint 状态。
- token 只防旧尝试的迟到结果，不代表权限。重复完成必须带相同目录；并发重试共享同一新 token。
- 普通输入、排队输入继续保留；准备期间拒绝修改 workspace，避免覆盖完成准备所绑定的目录。
- 客户端 stop 等待取消回执；Agent 的共用 stop_turn 持久排入取消控制，尚未消费的完成消息不能抢先放行。
  已 ready 的 Thread 沿用原有停止语义；删除后迟到回调不能创建 Thread。

边界：本阶段没有 Git 准备 worker、客户端启动 RPC 或 UI。调用完成/失败/重试的方法需运行节点的
Thread 主进程处理回执；节点退出时调用方仍需中止请求，重启后读取状态并重试。
控制回执随 Thread 保存和删除，尚未增加后台回执清理机制。

后续顺序：先由 Git 扩展通过这些公开原语实现准备与现场核查，再接 Node 恢复调度，最后接客户端
新任务入口并验收响应丢失、节点重启和部分失败。不要仅凭目录存在自动放行。

### 本批验证

- Siphon：106 项通过、1 项忽略，含并发接受、失败后重启恢复、并发重试、重复放行、旧 token、
  无效目录、客户端/工具停止、删除与真实 Agent 执行回归。
- 节点权限与协议：3 项通过；生成 TypeScript 保持与节点输出一致。
- Clippy 全目标、Rust 格式、客户端类型检查、24 个包的 Rust 依赖边界检查通过。
- 本批未改产品 UI，未进行浏览器验收。

## 首条输入与附件原子接受（已确认并实现）

现有 HTTP 上传绑定已存在的 Thread，且图片经过 keep_given / keep_image 后引用可能改变。
不得先创建会执行的空 Thread、保存占位消息、预计算图片引用，或把附件暂存到另一个 Thread。

采用同一个接受原语：prepare_thread 的输入改为有序的 InputPart 列表，分为已有 InputContent 与
待接收附件（bytes、media_type、可选 filename）。这是 Rust 接收接口，不是模型协议的新消息类型，
不新增持久化状态或运行时程序。所有现有纯文本调用方一起迁移，不保留平行 API。

处理顺序：

1. 对原始输入的顺序、内容、附件字节摘要及元数据、Thread 配置和准备请求计算幂等身份。
   使用附件摘要，避免为大文件构建 JSON 数字数组；同一 key 的附件变化必须被拒绝。
2. 查询已有接受记录；若已经成功接受，返回原 Thread，不重复拟合图片或写附件。
3. 复用 files::keep_given，把文件写入待提交 Changes；图片仍用现有 MODELS bounds。
   为每个附件形成最终 InputContent，保留原始输入顺序。
4. 在一次提交中创建 Thread、文件、最终首条输入、启动状态和密封接受记录。
   处理失败或并发冲突不会留下部分 Thread、部分首条输入或孤立文件。

Siphon 同时提供 volume() 公共访问器，供节点 HTTP 接入同一公开接口，避免节点为此直接写 runtime/Beak。
附件原语不处理 Git、multipart、用户认证或 UI。输入类型不进入模型协议；接收完成后仍只保存既有 InputContent。

### 已确认的 core 文件

- 新增 core/siphon/src/threads/input.rs：InputPart、原始输入身份与复用现有文件接收逻辑。
- core/siphon/src/threads.rs：导出输入类型。
- core/siphon/src/threads/operations.rs：prepare 接收新输入，文件与首条消息共同原子提交。
- core/siphon/src/volume.rs：迁移公开 prepare_thread 签名。
- core/siphon/src/lib.rs：新增只读取得 Volume 句柄的公共方法，不暴露裸 runtime。
- core/siphon/src/tests/threads.rs：迁移调用，补纯附件、图文顺序、无效图片回滚、附件幂等与重启读取测试。
- core/siphon/src/tests/agent.rs：迁移调用，验证放行前不调用模型、放行后输入引用已落盘。

不修改 files.rs 的现有处理规则，不修改 Beak、runtime、启动状态机或模型协议。
本批实现限于上述已确认文件；Git 扩展的纯内容调用方同步迁移。没有修改其他核心文件。

### core 之外的后续接入

节点 HTTP 接受有总大小限制的 multipart：一个有序消息清单及附件二进制，各附件索引只允许引用一次，
缺失/重复/多余部分均拒绝；不通过 WebSocket 传大文件 JSON 数组。鉴权在接收处理前验证 Git 与 Thread
操作权限，复用节点 loopback/tunnel 的会话边界。Git 扩展继续验证固定 commit、目录及分支参数。

客户端先把原始草稿、Blob、Git 参数和 key 写入现有 IndexedDB。接受成功后立即保存 Thread ID，再进入
显式准备流程；接受响应丢失用同一份 multipart 和 key 重放。不复用旧 threads.create/deliver 的 23 小时窗口，
也不把 Git 写操作改成自动重试。新建入口和恢复页共享现有持久草稿，不引入第二套待处理记录。

本批优先完成核心原语及回归，之后分别提交 HTTP/客户端接收和新建入口，完整任务验收后再开放功能。

### 附件原语实现记录

InputPart 仅用于 Rust 接收边界，最终持久化仍是已有 InputContent。纯内容部分沿用原有序列化形式，
附件身份包含字节 SHA-256、媒体类型与文件名，按输入顺序参与整体指纹；没有大文件 JSON 数字数组。
纯文本、纯附件和混合输入均使用同一 prepare_thread，不新增旁路接口。

回归覆盖：并发接受同一图片、拟合前后引用变化、图文顺序、重启后读取、原始字节及文件名变化拒绝、
无效图片导致整次接受回滚、纯文件消息和放行后 Agent 输入引用可读取。HTTP multipart 和 UI 新建入口尚未实现。

本批验证：Siphon 108 项、Git 14 项、节点权限/协议 3 项测试通过，另有 1 项忽略；Siphon/Git 全目标
Clippy、Rust 格式和 24 个包的依赖边界检查通过。本批没有界面变更，未进行浏览器验收。
