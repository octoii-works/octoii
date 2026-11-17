# 新任务工作目录：首个实现切片

2026-11-17。对应 ROADMAP R1a；已完成 worktree 查询、创建 RPC 和权限回归；目录准备与 Thread 启动的可靠衔接尚未实现。

## 复用现状

siphon-git::Git 已实现 repository、worktrees 和 create_worktree，Agent 的 git 工具已使用这些实现。客户端协议原先只提供仓库、diff、版本和提交等只读方法，本切片补齐 worktree 查询和创建。新 Thread 已支持在创建时指定 Config.workspace，但目录准备和首条消息目前没有统一的失败恢复流程。

不新增 Workspace 管理层、worktree 池、自动合并或运行中 Thread 迁移。本切片只提供客户端创建任务所需的 Git 原语，不把创建目录包装成启动任务。

## 客户端协议

- git.worktrees：参数 path（仓库内的目录），返回 worktrees；沿用 Git 的解析与真实注册状态，要求 files.read。
- git.create-worktree：参数 path（源仓库内的目录）、destination（绝对路径）、branch（新分支）、base（明确的基准 revision），返回 worktree（实际路径、新分支、实际 base_commit）；要求 git.operate。
- 复用 Git::create_worktree 的所有检查与 Git 自身行为；不复制实现，不自动清理失败残留，不新增 force 或 switch。
- 通过现有 Client.call 调用，不自动重试写操作。响应丢失时先查询注册 worktree 并核查现场；本切片不承诺幂等创建或断电恢复。

## 权限

当前 Scope 没有 Git 写操作权限。新增 GitOperate（git.operate），不能让 files.read 执行有写入副作用的 Git 操作，也不把 Git 操作挂到语义不符的终端权限上。

经用户单独确认，core 仅修改 core/siphon/src/protocol.rs 的 Scope 枚举。应用的新客户端完整权限清单加入新 scope；既有 session 的 scopes 不回填、不扩大，须由后续明确授权获得新权限。扩展声明与权限回归测试、节点类型生成和客户端协议更新均在 core 外。

查询沿用 person_machine 的只读视图。创建操作校验 git.operate 后，通过 Volume.commands().open 使用主机写入能力，保留 Host.hidden 保护；不改变 Agent 工具的执行权限。Git 可以执行仓库配置的 hooks/filters，因此 git.operate 是可信客户端的执行权限，不是仅能修改某个目录的文件权限。

## 协议归属

暂不拆独立 protocol 包。Siphon 持有基础通信和鉴权机制，各扩展持有自己的请求和结果类型，Node 装配并生成 TypeScript。当前 Scope 仍由 core 枚举管理，不在本切片引入动态权限注册。

## 本切片验收

1. 真实临时仓库：查询现有 worktree，并从明确 base 创建新目录，返回真实分支与 commit。
2. 源目录未提交修改和未跟踪文件不被复制，新目录内容来自 base。
3. 只读客户端可以查询，创建请求被拒绝且没有创建目录或分支。
4. 重复创建/占用目录失败不删除既有文件；已有 Git 实现测试继续通过。
5. 生成的 TypeScript 与 Rust 协议一致，类型检查和依赖分层检查通过。

## 后续切片

本切片通过后，再设计目录准备与 Thread 创建/首条消息的可靠衔接：输入保留、状态可查询、重复提交、节点重启和部分失败；仍优先复用 Siphon 公共原语，任何新增 core 接口另行说明并确认。未解决这些条件前，不上线新建任务 worktree 选项，不宣称 R1a 完成。

## 验证记录

Git 测试 10 项通过；节点访问权限与协议生成测试 3 项通过。Clippy（siphon-git、octoii 全目标）、TypeScript 类型检查、Rust 依赖边界检查通过。重启测试验证旧 session 的权限保持原值。

## 第二切片：首条消息失败重试（5560d75，恢复流程已由第三切片替代）

已修复新建页面在 Thread 创建成功、附件上传或消息投递失败后，重试又创建空 Thread 的问题。页面中的 NewThread 保留按创建配置区分的 Thread；附件全部保存后才投递首条消息，成功后才导航和清理已提交草稿。上传顺序执行，失败返回时不会还有同一批上传在后台继续。并发发送共享同一次提交，修改草稿后的显式重试使用最新内容；不同目录或模型配置不混用 Thread。

复用 Client.create / deliver 已有的幂等键和断线重试机制，不改 core，不新增启动 RPC。客户端方法内部的连接重试仍使用原键；这次页面层仅处理这些调用明确失败后的用户重试。

边界：保留状态仅在同一新建页面实例内有效。离开页面、刷新、关闭客户端后的恢复尚未实现；已经失败的空 Thread 不自动删除。worktree 写操作仍不自动重试，也尚未接入新建界面。

验证：新增 5 项测试覆盖上传失败、投递拒绝、重复发送、目录切换和创建拒绝；Web 全套 215 项测试通过，类型检查和 lint 通过。本轮未做浏览器交互验收。

后续顺序：

1. 持久保留新任务输入、附件和创建/投递幂等键，并按节点隔离；明确刷新后的恢复入口，避免自动启动未确认任务。
2. 接入目录准备状态。worktree 创建结果不确定时先核查现场，不盲目重建、不自动删除；目录确定后才创建对应 Thread。
3. 衔接首条消息，验收节点重启、响应丢失、重复提交与部分失败，然后才开放新建 worktree 界面。

## 第三切片：已提交任务的本地恢复

新建页面在首次远程写操作之前，将配置、原始消息、附件 Blob 和操作 key 写入 IndexedDB。创建、上传完成后的最终 content、投递回执分别保存；每次事务完成后才进入下一步。刷新后只读取待恢复快照，必须点击 Continue task 才继续。已收到投递回执的记录直接打开原 Thread，不再投递。

- 存储按 welcome 中的配对 session 与新建页面原始项目入口隔离。同一会话跨节点重启保持有效，重新配对不会自动读取旧会话记录；不是跨设备同步。改变文件夹选择后，恢复记录仍位于原新建页面入口，界面展示快照中的真实 workspace。
- 使用现有 Client.call 和 threads.create / threads.deliver 的 key，不改客户端协议或 core。创建与投递分别使用同一 UUID 的 -create / -deliver 后缀。请求单次发送，断线后等待用户恢复，不后台无限重试。
- 恢复始终发送保存的原始配置和内容；第二切片允许修改草稿再重试的方式不再适用于已持久化提交。未点击发送的普通草稿仍只保存在页面内存。
- 浏览器 Web Locks 串行化同一入口的恢复，IndexedDB 原子 begin 防止另一个页面覆盖待处理提交。清理按提交 key 比较，旧页面的回执不能清除新提交。
- 存储读失败有 Retry；存储写失败不继续后续远程写操作。丢弃只删除本地恢复副本，不停止或删除 Thread；已知 Thread 可直接查看。

### 幂等窗口

core/siphon/src/threads/idempotency.rs 的 RETENTION_MS 为 24 小时。本切片不修改该机制。恢复采用 23 小时窗口（预留一小时），未来时间、缺失时间和超期记录拒绝重放；最终投递前再次检查。此窗口依赖客户端与节点时钟正常推进，不是永久 exactly-once 保证。超期保留输入供人工核对，已知 Thread 可打开；若要长期、跨设备的恢复，必须另行设计节点侧持久任务记录，不能简单放宽客户端重试时间。

### 验证

Web 测试覆盖初次落盘失败、创建响应丢失、附件恢复、投递响应丢失、已完成记录、重复点击、过期拒绝和旧回执不清除新提交。真实隔离节点测试验证客户端及节点重启后，复用 key 返回相同 Thread 与 delivery。

Chrome 原生 IndexedDB + Web Locks 验证：保存文本与附件 Blob → 刷新 → 恢复，只有一次创建与投递；不同 session 互不读取；两个恢复实例并发执行仍只创建、投递一次；确认后清除记录。验证页仅使用临时 mock transport，不连接日常节点。本轮未执行完整产品界面的真人操作验收。

下一步仍是 worktree 准备状态的核查与 Thread 启动衔接，不能将这些有限窗口内的客户端恢复视作 R1a 全部完成。

## 第四切片：内核启动门

`docs/THREAD_START.md` 的第一阶段已实现：Thread 与首条输入原子接受但暂不执行，准备失败保留输入，
新 token 重试，最终目录绑定与放行原子生效，停止和删除防止迟到结果重新放行。
状态通过已有 Thread 查询暴露，启动控制仅提供 Rust 公共接口。

此切片尚未把 Git 创建、节点恢复调度与客户端串起来；R1a 仍未完成。下一步是 Git 扩展准备服务，
继续使用 Siphon 公共接口，不直接访问 Beak 或 runtime。

## 第五切片：Git 扩展的显式准备入口

新增 `git.start`，通过 `action` 表达四种操作。所有操作同时要求 `git.operate` 与
`threads.operate`，既有 session 权限不自动扩大。复用已有 Git 创建逻辑、Siphon 启动门和 Commands；
本批不修改 core。

| action | 输入 | 行为 |
| --- | --- | --- |
| accept | key、config、content、worktree | 原子保存 Thread、输入和准备请求；不运行 Git 或 Agent |
| prepare | thread、token | 执行已保存请求；成功绑定目录并放行，Git 失败保存失败状态 |
| retry | thread、token | 从失败状态换取新 token，再执行同一个已保存请求 |
| inspect | thread | 返回当前启动状态及源仓库的注册 worktrees；不修改状态或采用已有目录 |

结果为 `thread`、`start`，检查现场时另含 `worktrees`。普通 `threads.list` 已可查询状态；
停止与删除仍用现有 Thread 方法。失败后的 prepare 只返回失败状态，必须明确 retry 才开始新尝试。
已 ready 的 prepare 返回现状，不再次运行 Git；旧 token 被拒绝。

准备请求沿用 `CreateWorktree` 的字段，额外要求 source/destination 为绝对路径、base 为完整的
40/64 位十六进制 commit ID。不能传 HEAD、分支或标签等移动引用；调用方先取得所选 commit，
这样重启或重试也不会改变基准。config.workspace 留空，最终 workspace 只从 Git 成功回执绑定。
普通 Git 工具和 `git.create-worktree` 仍保留原有 revision 与 tracking 语义。

同一节点的这些准备操作通过扩展实例持有的锁串行处理，普通 Git 查询、Thread 查询和停止不受此锁阻塞。
它只避免活跃节点内重复执行，不能代替持久状态或跨机器调度。

### 故障和恢复边界

- accept 与 prepare 分开，客户端收到 Thread ID 后才能请求准备；响应丢失可用同一个 key 重放 accept。
- 节点重启不会自动执行 pending 请求。恢复入口应先 inspect 展示状态，再由用户明确选择继续或取消。
- Git 成功但放行回执不确定时，先查 start；若已 ready，直接打开原 Thread。
- 若仍 pending/failed，即使 path、branch、HEAD 完全匹配，也不自动认领或放行。再次显式准备时，
  现有分支/目录仍由 Git 拒绝，输入继续保留；现场由用户处理，不自动删除或强制覆盖。
- 准备中停止/删除会阻止放行，但不会撤销已发生的 Git 写入；本切片不承诺停止会立即终止 Git 进程。
- 当前没有后台 worker、自动恢复扫描或新任务 UI。附件接收顺序仍需补齐，见第六切片的缺口记录。

### 验证

Git 扩展全量 14 项测试通过，包括新增的重复准备、固定基准、已有 worktree 不被接管、输入保留、
停止后不执行、双权限检查、节点重启与失败后显式重试。Clippy 全目标、协议生成、客户端类型与依赖分层检查通过。
本批未改 UI，未进行浏览器验收。

下一步接客户端新任务入口和恢复交互，调用这一个扩展接口；再按实际需要评估节点后台调度。
不要同时引入另一套任务记录或自动采用已有 worktree 的捷径。

## 第六切片：Thread 内恢复界面

Git 待准备 Thread 显示已保存输入、目标目录/分支、准备失败原因和恢复操作，替换普通发送框。
先显式检查注册 worktrees，才启用继续或重试；检查结果默认折叠。打开 Thread 或重新连接不会自动执行 Git。
写入回执不确定时丢弃本地检查结果，要求重新检查。取消在准备期间仍可操作，并沿用现有 threads.stop。

侧栏和标题新增静态的「Awaiting preparation」，待准备 inbox 不再冒充 Agent 正在运行。
失败仍显示 Failed；待准备任务不会按闲置时间自动归入已完成。未取得两项操作权限时不开放恢复按钮。

### 新建入口的附件缺口

前一切片所写的「附件在 accept 前上传」不能直接用现有 API 实现：keepFile 必须绑定已存在的 Thread，
而 accept 同时创建 Thread 并保存完整首条输入；图片上传还可能经过拟合后生成不同的文件引用。
因此不能由浏览器预先计算 hash、先发缺附件的输入或创建一个额外临时 Thread 来绕过这个顺序。

本切片只开放已接受任务的恢复界面，尚未开放新建 worktree 选项。后续需先设计首条输入与附件的接收顺序，
必要的 Siphon 原语列明 core 文件范围后再单独确认；然后接新建入口和完整真实任务验收。

### 验证

Web 221 项测试通过，覆盖待准备状态、恢复按钮准入、准备中可取消和失败/取消展示；类型检查、lint 通过。
内置浏览器使用真实卡片组件的隔离 fixture，验证检查后启用继续、模拟 Git 失败后重新检查、取消后隐藏执行操作。
这是组件交互验收，没有连接真实节点执行 Git，也没有验收尚未接入的新建入口。
