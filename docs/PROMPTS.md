# 提示词职责与精简方案

2026-11-17。状态：方案已收敛为必需的 siphon-base 扩展；P1 已实现并通过工作区测试、Clippy、前端类型和依赖分层检查；其余阶段尚未实施。

## 边界

目标：Core 不内置行为提示词。身份、上下文协议说明、授权边界、基础工作原则和汇报约定由 siphon-base 提供。功能相关规则与上下文由所属扩展通过现有 Teller 注入。工具 description/schema 只定义调用契约，不重复 Teller 中的说明。

不新增提示词框架、注册层或独立 prompt 包。沿用 Registrar::teller、Telling.sections、Shared 和现有 section 增量更新机制。扩展不必贡献空 section，也不因为支持 Teller 就增加模型已知的操作教程。

| 层 | 负责 | 不负责 |
| --- | --- | --- |
| siphon-base/base | 身份、系统上下文协议、授权和工作原则 | Git、记忆、技能、HTML、Thread 等功能规则 |
| siphon-base/instructions（P2） | 用户和项目指令的收集、来源与作用范围 | 产品功能说明 |
| siphon-base/environment（P2） | workspace、平台、权限、临时目录、日期等运行事实 | Git 仓库探测、扩展目录说明 |
| 扩展 Teller | 功能使用约定、产品协议、需要注入的上下文 | 重复 action 参数、通用领域教程 |
| Tool description/schema | 工具职责、操作参数、默认值和返回语义 | 长篇功能工作流 |
| 工具结果 | 本次操作的实际结果及必要后续信息 | 重复静态规则 |

base 扩展的所有 Teller 固定最先，内部及其余扩展按 Shared 稳定排序：公共内容 Machine，项目内容 Project，会话内容 Thread。不要把静态规则按 Thread 重复生成，也不要把易过期的 Git 状态加入稳定前缀。

## 当前实现与迁移清单

| 部分 | 当前情况 | 目标 |
| --- | --- | --- |
| base | 已由 siphon-base 注入；简洁的 Context/Work/Replies，Git 已迁出 | 后续迁出产品卡片协议，保留通用规则 |
| instructions | 核心收集用户指令和逐层 AGENTS.md | P2 迁入基础扩展内部模块，保持来源与范围规则 |
| environment | 运行事实混有 Git 仓库位置、扩展 writable 目录 | Git 信息迁给 Git Teller；权限范围继续由核心统一表达，扩展目录用途由所属 Teller 解释 |
| Git | 已有独立 Teller，注入 Git 政策和 worktree 约定 | P2 核对仓库事实的归属；工具描述只保留职责 |
| Browser | 工具描述复述大量 action，含 page 卡片协议 | Teller 放当前页模型、引用/页面卡片约定；动作参数留 schema，删除重复动作目录 |
| Terminal | 工具描述混合生命周期、通知、卡片及 action 说明 | Teller 放长任务与 shell 分工、完成通知和卡片约定；schema 负责操作参数 |
| Thread | 已有线程历史目录 Teller，工具描述仍很长 | 整合历史访问、线程间通信与 thread 卡片约定到同一扩展；删除重复 action 清单 |
| Delegate | 工具描述含委派授权、隔离上下文、种类、等待通知 | Teller 放委派政策和上下文隔离；schema 放 start/send、kind、background 参数；保留必要的动态结果提醒 |
| Files/HTML | present_file 描述包含完整 HTML 生成规范 | Files Teller 提供呈现/生成约定；工具描述仅留文件保存与呈现契约；主题、字号、资源加载约定保留，避免重复维护 |
| Memory | 已有项目 Teller，注入规则、路径和索引 | 保留所属关系，精简重复写作建议，不把记忆正文全部注入 |
| Skills | 已有项目 Teller，提供技能目录和读取规则 | 保留；技能内容仍按需读取 |
| MCP | 已有项目 Teller 和外部工具目录 | 保留外部 schema，不重写第三方工具提示词 |
| Question | 目前只用工具描述 | 功能交互约定交给 Teller，选项和回答参数由 schema 表达；不新增空泛的提问教程 |
| Web search / Claude provider | Provider 原生能力或接入层 | 无产品专属规则时不新增 Teller |

产品卡片的通用语法应只维护一份，按实际展示协议归属确定共享文本的位置，各扩展仅声明自己提供的 kind。暂不为了提示词迁移新增 UI 包或修改 core 的运行机制。

## 基础文本

当前实际文本见 [base.md](../extensions/siphon-base/prompts/base.md)。它解释上下文与授权边界、路径和调用顺序、工作范围及准确汇报。Git 约定见 [git.md](../extensions/siphon-git/src/git.md)。

精简不改变权限执行规则。当前仍保留文件链接和资源卡片语法，待功能扩展接管时再迁出，避免展示协议丢失。

## 顺序与验收

1. 确认 core 的具体改动范围，再把 base 中 Git 规则与 environment 的 Git 信息迁给 Git Teller；同一提交完成迁入和迁出，避免重复或短暂缺失。
2. 精简 base，迁移展示协议；检查 system/extension/skills/instructions 的措辞不冲突。
3. 按 Browser、Terminal、Thread、Delegate、Files、Question 整理 Teller 和工具定义。Memory/Skills/MCP 只处理实际重复，不扩展功能。
4. 更新节点模型输入快照，验证实际注入顺序、规则没有重复、必要参数和产品约束仍然存在。
5. 验证扩展禁用、explorer 子任务和上下文窗口变化时的可见性：不能注入不可调用工具的操作指令。特别是 Teller 的扩展过滤不等同于工具的子任务权限过滤，需要逐一核对。

验收以信息边界正确为先，不以固定字数作为目标。不改工具行为、权限实现或模型接入，不新增运行期抽象。

## Core 修改范围

后续拟修改 core/siphon/src/agent/sections.rs：精简 BASE，迁出 Git 和产品展示专属文字，调整 Environment 的 Git 展示。仅扩展侧无法删除现有 core 固定字符串，因此需要这一个核心文件的变更。

先检查引用 BASE 和 environment 的核心测试；若要改 core 下的测试，必须先列出具体文件另行确认。节点快照位于 apps/node，不属于 core。本方案不授权修改 gather、Teller、权限、执行或存储机制。

## 当前实施切片

P1：新增 siphon-base，固定扩展标识 base；启动时必须恰好注册一次，所有 base Teller 优先于其他扩展，不允许关闭，旧禁用配置不能使其失效。无数字优先级系统。迁出 BASE 并精简，Git 政策迁给 Git 扩展；暂保留展示协议，待所属功能 Teller 接管时删除重复。

P2：将 instructions/environment 移入基础扩展的内部模块，不再单独建包。

P3：基础扩展提供标题、压缩、审核的独立调用提示词；不把它们注入主会话。

P4：整理其他扩展的 Teller 与工具契约。

P1 实际变更：基础规则迁到 siphon-base/prompts/base.md，Git 规则迁到 siphon-git 的独立 Teller。运行时标识为 base；启动必须恰好装配一次，所有 base Teller 先于其他扩展，设置拒绝关闭并忽略旧数据中的 base 禁用项；UI 显示 Always enabled。已获单独授权修改 lib.rs、agent/sections.rs、agent/gather.rs、settings.rs，以及 connection 和 machine 两处启动测试夹具；随后单独获批更新 agent.rs 中一处旧 BASE 数量断言。Instructions、Environment 与标题/压缩/审核仍在 core，不能将 P1 视为全部迁移完成。
