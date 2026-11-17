# Siphon 闭源边界与分发方案

2026-11-17。用户已同意“执行与持久化内核闭源，Octoii、产品扩展与界面保持开放”的方向。
本文是基于当前代码的边界盘点与实施提案，不表示迁仓库、改许可证、拆包或发布已获确认或已完成。

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

## 实施顺序与验收

1. **边界盘点（本文）**：明确目标与现状差异；只写文档，不迁库、不改 AGENTS.md 或 core。
2. **最小分发试验**：拟定精确文件范围，获所需确认后，在隔离试验中验证公开 Rust 调用方无需私有源码，
   能执行一个真实任务、调用一个外部提供的工具、取消并恢复状态。必须覆盖回调生命周期与错误，不以 hello-world 链接成功代替验收。
3. **分离 Octoii 与运行机制**：根据试验结果列出逐文件范围；优先移动已有实现，不重写。
   模型调用、上下文与基础工具归公开侧，权限与恢复契约保持原语义；公开上层不反向接触 Beak/runtime。
4. **构建分离**：私有 CI 构建内核，公开构建只需要公开代码与声明的预编译产物；校验产物版本、目标平台与校验和，
   验证离线构建路径。没有预编译支持的平台明确报错，不能静默下载私有源码。
5. **最后迁库与发布**：核对源码与第三方材料归属、许可证清单、产物授权和仓库可见性；具体迁移操作再执行确认。
   不改写 archive 分支、不删除历史、不把暂时搬不出去的 Agent 代码隐含归为闭源。

macOS 先验证，Windows/Linux 等目标分别验收；目前 vendor/beak 只包含一个目标的产物，不能声称现有分发已覆盖所有平台。
每个实现切片单独提交，闭源调整与 worktree 产品功能分开提交。

## 对当前主线的影响

先暂停继续扩张浏览器 worktree 编排。t3code 调研揭示的服务端接受输入、延迟执行与失败状态归属，
可以作为上述边界盘点的真实用例；不因参考 t3code 引入第二套 Run/Workspace 体系。
已有浏览器恢复代码保留，当前功能不回退。是否补服务端启动原语，需要检查 Siphon 已有能力并单独确认具体 core 范围。

本方案不改变任何现有许可证或仓库可见性，也不证明所有依赖和历史贡献已可重新授权；发布前另行核对实际材料。
