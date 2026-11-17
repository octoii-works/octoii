# 内置扩展与 Agent 工具接口

确认：2026-11-17。本文区分目标规则和当前实现，不把设计视为已完成。

## 边界

内置能力默认装配，按需启动资源，不提供产品扩展开关。MCP 配置、账号登录、工具权限、项目信任和子任务限制继续保留。移除现有开关尚待实施，涉及 core 的部分必须单独确认。

上层使用 Siphon 公共接口；具体扩展不依赖彼此的工具实现。共享产品实现放 `extensions/common/`，各自保留独立 Cargo 包，不建立万能 common crate。首个迁移对象是 siphon-html，尚未迁移。UI 留在 apps/web，不独立拆包。

## 工具入口

一个产品对象尽量对应一个 Agent 工具，通过必需的 action 区分操作。每个 action 使用独立参数类型，拒绝不属于该操作的字段；不使用大参数对象隐含选择行为。

- action 使用 snake_case；每个操作分别判断权限和效果。
- 当前 Thread、节点和权限由宿主提供。
- 沿用 content / structuredContent / metadata，区分请求接受、进行中、完成。
- 多步组合交给 execute，不增加通用 batch 或 extension.call 总入口。
- 单操作工具不强加 action；客户端 RPC 和事件订阅不要求压成单入口。
- 工具数量少不等于 schema 小，不为减少入口而隐藏具体参数契约。

| 扩展 | Agent 接口与边界 |
| --- | --- |
| browser | 一个 browser 工具；type 在光标处输入，fill 替换指定文本元素内容，press 负责按键；拒绝旧 clear/submit 参数 |
| terminals | terminal 的 open/read/write/close；与一次性 shell 分工 |
| thread | thread 的查询、历史、通信与整理 action；不承载 Git |
| delegate | start 创建、send 继续；两者保留等待/后台通知，send 不接受类型或标题 |
| git | 计划一个 git 入口，优先 status/list_worktrees/create_worktree/remove_worktree；目前尚未注册 Agent 工具。普通 commit/push/merge 继续 shell |
| files | present_file 单操作；不承担基础 read/write/edit |
| question | question 单操作；不是权限审批 |
| memory | 记忆上下文与管理，暂不新增 Agent 工具 |
| skills | 技能发现与上下文，暂不新增 Agent 工具 |
| web-search | Provider 原生工具，不包装通用 action |
| mcp | 保留外部工具名称和 schema，复用现有目录与调用机制 |
| claude-code | Provider 接入，不新增管理工具给 Agent |
| html | 共享库，零工具、零独立服务 |

Git 创建 worktree 不等于切换 Thread。目录切换、项目归属、上下文重新收集及权限必须先核对 Siphon 的能力；需要 core 修改时单独确认。不得把“已创建”报告成“已切换”。

## 顺序与验收

1. Delegate 显式 action：schema 与反序列化一致，拒绝混合字段，start 默认 general_purpose，send 保留类型与父子权限；结果契约和等待机制不变。
2. Browser 参数整理：保留页面操作能力，迁移调用方和测试；不同时改生命周期。
3. Git/worktree：先确认目录切换边界，再实现实际需要的 action，UI 只显示环境与操作结果。

Delegate 已完成显式 action 迁移，包含参数校验、真实调用测试、模型提示词快照和 UI 摘要；等待与后台通知沿用原实现。Browser 已完成 type/fill/press 拆分，真实 Chromium 验证覆盖输入、替换、清空、只读拒绝和显式提交。Git 尚待实施。

移除扩展开关和 common 目录迁移分别提交，不混入工具参数变更。冻结的 core 中自包含执行测试夹具不是扩展真实契约的替代；真实扩展 schema、权限和调用行为在扩展自身测试中验证。
