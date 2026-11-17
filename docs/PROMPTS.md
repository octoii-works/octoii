# 提示词职责与精简方案

2026-11-17。状态：方案已收敛为必需的 siphon-base 扩展；P1–P4 已完成。

## 边界

目标：Core 不内置行为提示词。身份、上下文协议说明、授权边界、基础工作原则和汇报约定由 siphon-base 提供。功能相关规则与上下文由所属扩展通过现有 Teller 注入。工具 description/schema 只定义调用契约，不重复 Teller 中的说明。

不新增提示词框架、注册层或独立 prompt 包。沿用 Registrar::teller、Telling.sections、Shared 和现有 section 增量更新机制。扩展不必贡献空 section，也不因为支持 Teller 就增加模型已知的操作教程。

| 层 | 负责 | 不负责 |
| --- | --- | --- |
| siphon-base/base | 身份、系统上下文协议、授权和工作原则 | Git、记忆、技能、HTML、Thread 等功能规则 |
| siphon-base/instructions | 用户和项目指令的收集、来源与作用范围 | 产品功能说明 |
| siphon-base/environment | workspace、平台、权限、临时目录、日期等运行事实 | Git 仓库探测、扩展目录说明 |
| 扩展 Teller | 功能使用约定、产品协议、需要注入的上下文 | 重复 action 参数、通用领域教程 |
| Tool description/schema | 工具职责、操作参数、默认值和返回语义 | 长篇功能工作流 |
| 工具结果 | 本次操作的实际结果及必要后续信息 | 重复静态规则 |

base 扩展的所有 Teller 固定最先，内部及其余扩展按 Shared 稳定排序：公共内容 Machine，项目内容 Project，会话内容 Thread。不要把静态规则按 Thread 重复生成，也不要把易过期的 Git 状态加入稳定前缀。

## 当前实现与迁移清单

| 部分 | 当前情况 | 目标 |
| --- | --- | --- |
| base | 已由 siphon-base 注入；简洁的 Context/Work/Replies，Git 已迁出 | 通用卡片语法只保留一份，各功能扩展声明 kind |
| instructions | 已由基础扩展收集用户指令和逐层 AGENTS.md | 复用公开的 Instructions 标记格式，保持来源与范围规则 |
| environment | 已由基础扩展主动读取运行事实并组织文本 | 不自行推导权限；目录由同一注册集合提供，仓库信息由 Git 扩展注入 |
| Git | 已有独立 Teller，注入 Git 政策和 worktree 约定 | P2 核对仓库事实的归属；工具描述只保留职责 |
| Browser | 已迁到 browser Teller，schema 保留动作参数 | Teller 放当前页模型、引用/页面卡片约定；动作参数留 schema，删除重复动作目录 |
| Terminal | 已迁到 terminals Teller，描述保留返回语义 | Teller 放长任务与 shell 分工、完成通知和卡片约定；schema 负责操作参数 |
| Thread | thread 静态规则和 threads 动态目录分开注入 | 整合历史访问、线程间通信与 thread 卡片约定到同一扩展；删除重复 action 清单 |
| Delegate | delegate Teller 负责授权与隔离，schema 保留任务参数 | Teller 放委派政策和上下文隔离；schema 放 start/send、kind、background 参数；保留必要的动态结果提醒 |
| Files/HTML | HTML 规范已迁到 Files Teller，文件契约留工具描述 | Files Teller 提供呈现/生成约定；工具描述仅留文件保存与呈现契约；主题、字号、资源加载约定保留，避免重复维护 |
| Memory | 已有项目 Teller，注入规则、路径和索引 | 保留所属关系，精简重复写作建议，不把记忆正文全部注入 |
| Skills | 已有项目 Teller，提供技能目录和读取规则 | 保留；技能内容仍按需读取 |
| MCP | 已有项目 Teller 和外部工具目录 | 保留外部 schema，不重写第三方工具提示词 |
| Question | question Teller 负责提问政策 | 功能交互约定交给 Teller，选项和回答参数由 schema 表达；不新增空泛的提问教程 |
| Web search / Claude provider | Provider 原生能力或接入层 | 无产品专属规则时不新增 Teller |

产品卡片的通用语法应只维护一份，按实际展示协议归属确定共享文本的位置，各扩展仅声明自己提供的 kind。暂不为了提示词迁移新增 UI 包或修改 core 的运行机制。

## 基础文本

当前实际文本见 [base.md](../extensions/siphon-base/prompts/base.md)。它解释上下文与授权边界、路径和调用顺序、工作范围及准确汇报。Git 约定见 [git.md](../extensions/siphon-git/src/git.md)。

精简不改变权限执行规则。文件链接语法归 Files；通用资源卡片语法由 base 维护一份，Browser、Terminal、Thread 仅声明自己的 kind 与标识含义。暂不为这一行共享语法新增 UI 扩展包。

## 验收

- 节点快照中的 base / instructions / environment 依次位于最前，只出现一次。
- 指令来源和读取范围保持一致；用户输入与检索数据不因迁移获得更高权限。
- 环境文本与实际模式、沙箱能力和当前启用目录一致；不在基础段注入 Git 功能内容。
- 扩展禁用和 explorer 子任务的可见性分别核对；Teller 的扩展过滤不替代工具权限过滤。
- 不改变权限执行、存储或模型接入。后续核心改动仍需按具体文件另行确认。

## 已确认的 P2 核心范围

lib.rs、agent.rs、删除 agent/sections.rs、agent/writable.rs、extension.rs、machine.rs、volume.rs、tools.rs、testing.rs、tests/agent.rs 和 tests/live.rs。Teller 签名及 gather.rs 未改；没有新增 Environment 汇总对象。

## 当前实施切片

P1：新增 siphon-base，固定扩展标识 base；启动时必须恰好注册一次，所有 base Teller 优先于其他扩展，不允许关闭，旧禁用配置不能使其失效。无数字优先级系统。迁出 BASE 并精简，Git 政策迁给 Git 扩展；暂保留展示协议，待所属功能 Teller 接管时删除重复。

P2：将 instructions/environment 移入基础扩展的内部模块，不再单独建包。

P3：基础扩展提供标题、压缩、审核的独立调用提示词；不把它们注入主会话。

P4：整理其他扩展的 Teller 与工具契约。

P1 实际变更：基础规则迁到 siphon-base/prompts/base.md，Git 规则迁到 siphon-git 的独立 Teller。运行时标识为 base；启动必须恰好装配一次，所有 base Teller 先于其他扩展，设置拒绝关闭并忽略旧数据中的 base 禁用项；UI 显示 Always enabled。已获单独授权修改 lib.rs、agent/sections.rs、agent/gather.rs、settings.rs，以及 connection 和 machine 两处启动测试夹具；随后单独获批更新 agent.rs 中一处旧 BASE 数量断言。P1 完成时 Instructions、Environment 仍在 core，由 P2 继续迁出；标题/压缩/审核待 P3。

## P2 实际边界

Teller 签名不变。siphon-base 自行读取指令、日期、平台、宿主路径和 Thread 配置；core 不创建面向提示词的 Environment 对象。删除 core 的 agent/sections.rs 和 Setup.config，指令目录改由应用传给 BaseExtension。

只读公共能力：Volume::thread_config 按 ID 读取配置；machine::confines 查询沙箱能力；Registrar::writable_folders 获取构建结束时初始化的只读句柄。目录来自执行机制使用的同一注册集合，读取时复用现有扩展启用过滤，不复制授权规则。该句柄没有写入接口，也不赋予写权限。

P2 验证：工作区 287 项通过、3 项忽略；Clippy、格式和分层检查通过。新增验收覆盖指令来源顺序和去重、base 先注册仍可读取后注册目录、关闭扩展后目录不再注入、运行模式展示、基础三段的顺序。Git 仓库位置迁为 Git 的项目级 repository section。核心测试不再假设有隐式系统上下文。

## P3 实际边界

标题、压缩和审核提示词分别存放于 siphon-base/prompts/title.md、recap.md、review.md。三份文本原样迁移，保持标题要求、摘要结构和审核政策；不进入主会话的 Teller 上下文。

基础扩展通过 Registrar::prompts 提供固定的 agent::Prompts { title, recap, review }。启动要求恰好注册一组，所有字段非空且注册者是 base；没有默认提示词、按名称查找或覆盖机制。core 将内容交给现有标题、压缩程序与审核处理函数，保留触发时机、持久化协议、输出解析和审核失败后的人工审批路径。

核心测试显式提供测试提示词，不依赖产品扩展。回归覆盖缺失、重复、非 base 注册和各字段空白的启动拒绝，并通过现有标题、压缩和审核流程验证提示词传递。

P3 验证：工作区 288 项通过、3 项忽略；siphon/siphon-base 全目标 Clippy、格式和 24 包分层检查通过。三份生产提示词与迁移前文本逐字一致。

## P4 实际边界

Browser、Terminal、Thread、Delegate、Files、Question 各自提供 Machine 级静态 Teller。工具描述不再复述 action 目录；参数语义仍由原 schema 提供。Thread 的历史目录保持 Thread 级动态 section，且没有宿主历史目录时静态使用规则仍会注入。HTML 布局、主题变量、字号和资源加载规范原样保留。

Memory、Skills 已通过项目 Teller 注入，保留现有按需读取结构；MCP 保留第三方 schema，Provider 不增加无实际用途的 Teller。没有新增提示词框架、共享扩展包或 core 改动。

Teller 按扩展启用状态过滤，不按子任务工具集合过滤，因此静态规则明确以工具可用为前提；它们不授予权限。节点集成测试核对子任务实际工具集合，并验证禁用扩展时工具、静态规则与动态目录一并消失。快照记录模型实际收到的完整上下文。

P4 验证：工作区 289 项通过、3 项忽略；节点及六个扩展全目标 Clippy、格式与分层检查通过。HTML 生成规范与迁移前逐字一致。
