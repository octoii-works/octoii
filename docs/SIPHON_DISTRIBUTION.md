# Siphon 闭源边界与分发方案

AI 已发布为 crates.io 的 octoii-ai 0.0.2，主仓库已改用公开包。Siphon 仍以私有源码 submodule 接入，
公开接口仓库已建立。当前发布状态和源码切换顺序见 [SIPHON_CUTOVER.md](SIPHON_CUTOVER.md)。

2026-11-17。用户已同意“执行与持久化内核闭源，Octoii、产品扩展与界面保持开放”的方向。
本文是基于当前代码的边界盘点与实施提案，不表示迁仓库、改许可证、拆包或发布已获确认或已完成。

## 当前仓库状态

个人私有 Siphon 和组织公开 AI 仓库已创建并推送，主仓库改用固定提交的 submodule。
这按用户后续指示先于机制边界收敛执行；下文早期“尚未迁库”的描述保留为方案背景。
现状与操作方式以 [SOURCE_REPOSITORIES.md](SOURCE_REPOSITORIES.md) 为准。

## 最新实施顺序

[SIPHON_CUTOVER.md](SIPHON_CUTOVER.md) 统筹当前契约清理、公开接口、三平台分发与源码移除。
早期拆分过程见 [SIPHON_EXTRACTION.md](SIPHON_EXTRACTION.md)。下文 S1–S7 记录早期设计。

## 目标

Siphon 负责可靠运行：状态提交、消息投递、执行生命周期、取消、恢复和权限执行。
Octoii 负责 Agent 行为：上下文组织、模型调用、工具调用策略、标题与压缩等 Agent 工作。
产品扩展负责具体能力、提示词和工作流；不因闭源将 Git、浏览器或任务启动的产品逻辑搬进内核。

保持 Rust，保留 Beak 与 Siphon 的执行语义，不重新设计存储，不引入新的 Workspace 管理层。
仓库可见性、Rust 包边界、二进制接口和产品授权分别设计，不能用移动目录替代接口工作。

## 当前事实

| 代码位置 | 当前实际职责 | 对拆分的影响 |
| --- | --- | --- |
| vendor/beak | Rust 绑定、C 头文件、预编译 libbeak_c.a；当前分发目录仅见 aarch64-apple-darwin | 本仓库没有 Beak 内核实现源码，已有可复用的二进制分发范式；不能把绑定也当成内核实现一并隐藏 |
| core/siphon-runtime | 基于 Beak 的 Program、请求处理、执行与恢复；没有 octoii-ai 依赖 | 最清楚的闭源实现候选，但当前 Rust 接口不等于可独立分发的二进制接口 |
| core/siphon | 同时包含 Thread、权限、协议、Agent 循环、基础工具、账号和模型装配 | 不能整体搬走后仍宣称 Octoii 已开源独立；必须识别 Agent 与执行机制的边界 |
| core/siphon/src/agent | thread/turn、gather/render/model、title/compaction/review 等 | 同一目录混合持久程序与 Agent 策略，不能机械按文件夹迁移 |
| core/siphon/src/tools | 工具声明、权限、Plan 和 read/write/edit/glob/grep/shell/execute | 具体工具行为与执行授权机制要分清；不是全部闭源或全部迁出 |
| core/siphon-{script,shell,text,image} | 基础处理库，其中 script 包装 QuickJS | 暂不因为位于 core 就闭源或迁仓库；按职责、依赖和第三方组成单独归类 |
| core/octoii-ai | 模型词汇、供应商和传输 | 保持公开，独立仓库为目标，不先做搬迁 |
| extensions | 基于 Siphon 公共接口的产品能力，base 已持有基础、环境和辅助模型提示词 | 保持公开；当前是静态编译进宿主的扩展，不是动态插件 |
| apps/node | 初始化、账户入口、配对、服务与扩展装配 | 直接依赖 siphon、runtime 和 Beak；在闭源实现移走前必须解决构建入口 |
| packages/client、apps/web、apps/desktop | 客户端、Web、Electron | 保持公开，继续走协议与服务通信，不引入 Rust 实现依赖 |

依据：根 Cargo.toml、各包 Cargo.toml、core/siphon/src/lib.rs、agent.rs、tools.rs、extension.rs，
以及 vendor/beak/crates/beak-sys/build.rs。现有包依赖规则仍以 ARCHITECTURE.md 为准。

## 目标归属

闭源实现：Beak 内核、siphon-runtime 的持久执行实现，以及 Siphon 的 Thread 状态与执行、权限执行和恢复机制。
公开内容：Octoii 的 Agent 策略与基础工具行为、产品扩展、提示词、octoii-ai、客户端与产品界面。
公开接口：扩展注册、工具与 Teller 契约、受控执行/存储句柄、错误与协议定义、文档和必要的安全包装。

“审核规则如何表达”和“授权结果如何执行”分开：前者由公开 Agent/扩展提供，后者由内核保证。
“如何准备工作目录”和“未放行的任务不能执行”分开：前者归产品扩展，后者才是通用执行机制。
启动、失败、取消与恢复的语义不能因源码边界变化而改变。

目标仓库形态（尚未创建或迁移）：

- octoii-siphon 私有：内核实现及其内部测试、构建与发布流水线，内部继续保留独立 Cargo 包。
- octoii 公开：Octoii、产品扩展、Node、UI、客户端与公开绑定；初期公开绑定可随此仓库发布。
- octoii-ai 公开：模型协议与适配；是否立即独立迁移不影响闭源工作。

不预先新增 protocol/types/sdk 三套包。公开的 siphon crate 最终可以是安全包装和扩展契约，
而私有实现处于它背后；具体实现 crate 名称和 Agent 包名待接口试验后决定。
现有 apps/node 的 Cargo 包名已是 octoii，不能直接给新 Agent 包取同名造成冲突。

## 分发必须先于迁移解决

公开源码、外部可独立编译、扩展可独立装载是三个不同目标。当前只拥有源码级 Rust 接口，
没有独立装载扩展的能力；Extension/Registrar 使用 trait、泛型、闭包和 async handler。
不能仅公开 trait 定义，再让外部代码直接链接任意版本的私有 Rust rlib，就宣称已提供稳定 SDK。
Rust Reference 明确说明 Rust ABI 没有稳定性保证。

| 方案 | 能解决什么 | 代价与限制 |
| --- | --- | --- |
| 私有 CI 编译完整 Node，公开发行二进制 | 最快让公开 UI 继续运行，保留当前进程结构 | 外部不能独立编译修改后的 Rust Agent/内置扩展；只是过渡发布方式，不能作为开放扩展目标的最终验收 |
| C ABI + 公开 Rust 安全包装 + 预编译实现 | 延续 Beak 分发方式，可让公开 Rust 上层独立构建 | 必须设计所有权、异步调用、取消、错误和版本契约；现有 trait 不能直接跨边界 |
| 独立内核服务 + IPC/RPC | 进程与语言边界明确 | 新增连接、身份、生命周期和扩展调用协议；现有客户端 WebSocket 不是内核驱动/扩展宿主协议 |

建议优先验证 C ABI 包装能否承载一个真实的执行闭环；失败时再评估进程边界。
这不是批准大规模 FFI 改造。没有验证结果前，不承诺全部 Siphon 直接复用 Beak 的构建脚本即可闭源。
固定 Rust 工具链的预编译 rlib 可以是内部构建实验，不能替代面向外部开发者的长期接口。
私有 Git 或私有 registry 分发源码只限制访问，并不让拿到源码的依赖方变成二进制消费者。

官方参考：

- [Rust ABI](https://doc.rust-lang.org/reference/items/external-blocks.html#abi)：Rust ABI 无稳定性保证；C ABI 仍需自行定义数据、所有权和版本契约。
- [Cargo dependencies](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)：Cargo 的 registry、Git、path 依赖来源。

## 当前优先级：先做本地 workspace 拆分

用户后续确认先在当前仓库拆成两个 Cargo workspace，构建验证后再迁往 devxiyang/octoii-siphon。
Siphon 六个包位于 siphon/crates，Beak 与 QuickJS 在 siphon/vendor；octoii-ai 位于 libs/octoii-ai。
本阶段只做目录/构建组织，暂缓 S1；以下边界收敛顺序仍作为后续计划。
早期事实表与边界清单的 core 路径是盘点时位置，当前路径以 ARCHITECTURE.md 为准。
私有仓库尚未创建，当前仍是一个 Git 仓库，不能宣称历史源码已闭源。

## 后续接口收敛顺序

2026-11-17 更新：先收紧上层依赖和公开接口，再验证二进制分发，最后分离实现与迁库。
当前不是第二轮内核重写。“分发必须先于迁移解决”仍成立，但不要求先做全面 FFI 才整理调用边界。

近期验收目标：产品层可以继续开发、编译和测试，不需要知道 Siphon 如何存储、调度和恢复。
只经由名为 siphon 的导入还不够：返回 Beak Snapshot 或接受 Runtime 的接口仍泄露实现。

详细调用位置、缺失能力和验收见 [SIPHON_BOUNDARIES.md](SIPHON_BOUNDARIES.md)。
清单基于代码 6b0b4f6，只是准备方案，不是 core 修改授权。

| 顺序 | 工作 | 完成标准 |
| --- | --- | --- |
| S0 边界盘点（本轮） | 区分生产直连、公共接口泄露、测试夹具、构建依赖 | 每项有调用位置、职责、替代方向、验收；仅修改文档 |
| S1 文件与只读查询 | 收敛 Node 附件接收/读取、扩展可用性与 MCP 信任查询 | 上层不取得原始快照或提交 Changes；图片、权限、信任与错误语义保持 |
| S2 宿主生命周期 | 明确 Siphon 打开、配置、关闭的宿主入口 | Node 启停不创建 Beak、不传 runtime Options、不访问 runtime.close；关闭完成语义明确 |
| S3 产品访问记录 | 为 Node 配对/会话确定受控持久化契约 | 一次性兑换原子性、有效期、撤销、重启和既有数据保持；产品规则仍属 Node |
| S4 上层构建与测试边界 | 迁移公开测试夹具，收紧导出，移除上层直接依赖 | Node/扩展无需底层实现类型；Cargo 与源码/API 审查共同保证边界 |
| S5 最小分发试验 | 隔离验证二进制边界，不迁库 | 公开调用方无私有源码可运行真实任务、公开工具回调、取消和重启恢复；验证错误及所有权 |
| S6 分离 Octoii | 按验证后的接口移动已有 Agent/工具实现 | 公开 Agent 行为与私有执行机制分开，原回归继续通过，不增加另一套运行系统 |
| S7 构建分离与迁库 | 私有 CI 产物、公开绑定、平台与版本检查，最后迁仓库 | 公开仓库独立构建上层，校验产物与离线路径；核对授权材料后执行具体迁移 |

S1 的文件与查询可以独立提交；S2/S3 不合成一次大改。每批进入实现前，列出实际 core 文件、
为什么现有公开接口不足并单独确认；本表不预先批准任何 core 文件。
不把底层依赖藏入 common 包，也不导出裸事务或私有路径给产品侧绕行。

macOS 先验证，其他目标分别验收。C ABI 安全包装仍是优先试验方向，进程边界是备选，
本轮不承诺最终 ABI、动态插件、通用 SDK 或新包名。私有 CI 编译完整 Node 只能作为过渡分发，
不能作为公开 Rust Agent/扩展可独立编译的完成证明。

## 与产品主线的关系

R1 已具备启动门、原子附件接受、首次自动准备和恢复入口，仍在补生命周期故障验收。
此前“暂停浏览器 worktree 编排、尚需评估启动原语”的描述已过时，本节替代它。
产品主线继续推进；闭源准备按实际功能触及的边界逐项完成，功能与边界改造分开提交。

保留 Beak/Siphon 的执行语义、已有存储和公开扩展机制。暂不迁库、不改许可证、不改写历史、
不创建第二套 Workspace/Job、不拆 protocol/types/sdk 三套包，也不因为位置在 core 就把处理库全部闭源。
正式分发前另行核对依赖、历史贡献与第三方材料；本方案不改变现有仓库或产物授权。
