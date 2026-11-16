# Octoii 产品工具设计

日期：2026-11-16。状态：首版实现方案；接口名称为实现目标，尚未发布。

基线：`0ce7fd6`，已快进推送到 main。后续在 `codex/mvp` 推进，每个完成的任务单独提交；本方案不恢复暂停的 goal。

## 产品目标与边界

Octoii 是 run anywhere 的 Agent。产品工具让它操作用户能够看见的会话、页面、终端、工作区，并把成果交给桌面和手机上的用户。当前只围绕自己的 Octoii，不重启多外部 Agent 产品线。

保留 Beak、Siphon runtime、现有 Tool/Extension/Registrar、协议与前端扩展机制。不引入产品工具总线、Artifact 数据库、通用工作流框架或另一套权限系统。内置实现无需绕经 MCP；MCP 是需要时的接入或对外适配边界。

工具、客户端协议和界面是同一产品操作的不同入口。共享业务规则，不让 Agent 通过模拟点击来完成已有产品操作，也不让工具直接改内部存储。扩展仅使用 Siphon 公共接口；缺少能力时补具体原语，不能暴露任意 volume 写入。

## 当前能力与复用

| 能力 | 现有实现 | 本轮改变 |
| --- | --- | --- |
| 文件和命令 | read/write/edit/glob/grep/shell | 保留职责与名称 |
| 工具组合 | execute | 保留；不创建另一套批量执行接口 |
| 浏览器 | siphon-browser，节点侧 Chromium、页面流、Agent 与用户共用页面 | 增加反馈上下文与必要的明确动作 |
| 终端 | siphon-terminals，画面、输入、命令退出通知 | 保留；不复制为 task 工具 |
| 会话与子任务 | thread、delegate | thread 补整理能力；delegate 继续只负责子任务 |
| 会话文件 | threads/files.rs，内容寻址、现有授权读取 | 原样成果保存与交付展示 |
| 工具结果 | Told.content / structuredContent / metadata | 给模型简短引用，给界面明确的文件信息 |
| Git | siphon-git，为 Diff 提供仓库、补丁、版本读取 | 后续增加新任务工作区准备；不包装全部 git 命令 |

## 工具命名与接口原则

保留以产品对象为中心的 `thread`、`browser`、`terminal`；同一对象中的离散操作采用现有的 tagged action 参数，各操作各自定义必需字段，不使用“大对象加一批可选字段”。

成果交付首版命名为 `present_file`：把当前机器上可读取的文件保存为当前会话的成果，并展示给用户。它不发起新任务、不投递用户输入、不公开发布互联网链接。不要称为 `deliver`，避免与已有 inbox 投递混淆。

不使用 `publish: true`、`send_to_model: false` 之类切换行为的标志。文件交付、模型读文件和未来 HTML 展示是不同操作。尺寸、外观等模式值是数据，可以作为明确的字段。

参数中不让模型传当前节点、当前会话、调用者权限等宿主已经知道的信息。操作其他会话时必须显式给出目标。结果提供稳定 id/ref，临时 URL 由客户端访问层生成，不进入持久工具记录。

## 结果契约前置试验

现有工具输出与 execute 的设计试验见 [TOOL_RESULTS.md](TOOL_RESULTS.md)。先验证原生结果的可编程数据与资源输出，再实现文件交付。工具结果统一采用 content + structuredContent + metadata；下方 presented_file 示例仍是后续产品工具的草案。

## P01：文件成果交付

### 用户行为

用户要求生成报告、截图或其他文件，Octoii 完成后调用：

```json
{"path":"reports/result.pdf","title":"验证报告"}
```

`path` 必需，相对当前工作区或为当前工具有权读取的绝对路径；`title` 可省略。首版一次一个普通文件。目录、打包、跨会话发送和互联网分享不包含在此接口中。

工具成功结果的目标形态：

```json
{"type":"presented_file","file":{"ref":"sha256:…","name":"result.pdf","media_type":"application/pdf","size":12345},"title":"验证报告"}
```

这只是契约示例，复用现有 FileRef 的字段命名，不增加同义引用类型。工具结果中的结构化内容是持久展示依据；模型文本只需说明已交付及文件引用。后续若需要在正文引用成果，复用文件引用，不建立另一套成果 id。

### 保存与展示语义

- 保存调用时实际读取的原始字节；源文件后续改变或删除不影响已交付成果。
- 不声称正在被其他进程写入的文件具有文件系统级原子快照；需要一致内容的生成器应先完成写入或原子替换。
- 首版复用 32 MiB 文件上限，读取过程中也执行上限，不能只检查 stat 后无界读取。
- 原图和模型用缩略图分离。不能使用当前 keep_image/show_image 的模型适配结果替代交付原件。
- 复用 thread/files 内容寻址；同一内容复用 blob，不等同于合并不同调用的展示记录。
- 不将所有成果字节自动写入模型上下文。ZIP 等模型不直接读取的文件仍可以作为下载成果。
- 界面基于类型提供图片、文本等已有预览能力；无法预览的文件提供下载，不伪装成可预览。PDF 先保证下载，内联预览另验收。
- HTML 首版仅作为文件下载，不在应用主文档上下文执行。
- 文件必须经节点现有鉴权读取；持久记录不存宿主绝对路径作为唯一访问方式，不依赖桌面本机 file URL。

### 权限与失败

文件读取通过当前工具的 machine/namespace，不能调用宿主文件 API 绕过限制。把内容加入会话是产品写操作，不能仅因为不改工作区就标为 Reads；首版纳入现有 Acts 权限判定，不增加旁路。

非法路径、非普通文件、超限、存储失败必须明确拒绝。文件存储提交完成不等于工具结果已经提交；只有持久工具结果出现后客户端才展示为已交付。复用现有工具中断与恢复语义，不因确认丢失重新读取可能已经变化的源文件并声称是原成果。

文件读取与保存属于有边界的业务原语。首版优先沿用现有 show_image 的先保存、再返回结果模式；两次提交间崩溃可能留下未引用 blob，需要如实记录并沿用 thread 生命周期处理，不额外建设资源事务框架。具体公开原语放在 Context/Volume 最合适的位置，由实现时对照现有文件访问接口确定。

### 归属

- Siphon：文件原样保存原语、结果契约与权限路径。
- 新的 `extensions/siphon-files`：注册 `present_file`，只实现具体工具业务。
- Web：文件成果卡片与打开/下载行为，复用现有鉴权资源访问。
- Electron：仅处理必要的原生下载交互，不承载成果存储。

### 验收

生成 PDF、PNG、文本及 ZIP，交付后关闭重开仍可读取；修改/删除源文件后交付字节不变；原图字节不被模型压缩；授权失败不能读取；超限不造成大内存读取；工具失败不展示成功卡片。远程 Web 使用节点资源路径验证，iPhone 真机验收另行记录，不能以桌面通过代替。

## P02：浏览器反馈

浏览器继续运行在节点。Agent 工具与客户端操作同一 page，不新建 Electron 专属浏览器，也不复制 Browser 的状态。

第一步是用户反馈闭环：点击“指出问题”进入选取模式，选择元素，生成输入上下文草稿，用户填写要求并发送。选择动作不等于提交，也不自动启动任务。

反馈至少含 page id、当时 URL、选取时间、元素描述、可用的定位信息、当时视口与截图引用。首版只捕获解决定位问题所需的有限内容，不保存整个 DOM。截图按会话文件保存；页面关闭后历史反馈仍可阅读。

反馈里的旧 locator/ref 是历史证据，不保证可直接执行；页面导航或引用失效后 Agent 必须重新读取。选取模式下用户点击只选取元素，不触发页面按钮。取消选择不提交消息。

先完成桌面点击选取；触摸选择与手机端交互单独验收。视口已有 resize 能力，客户端提供固定尺寸预设时应明确谁控制当前视口，不能让多个客户端自动布局相互覆盖。Agent 如需设置尺寸/外观，补 browser 的明确 action，复用同一实现。

归属为现有 siphon-browser 协议与 Web browser/composer 扩展。暂缓录屏、自由绘制、源码框架追踪、账号导入。

验收：选取元素加文字后模型能读到证据；选取不触发页面行为；发送前可删除反馈；导航后旧证据仍显示但旧引用不能误指新元素；重开历史截图可读。

## P03：会话整理

扩展现有 thread 工具，保留 list/search/read/start/send/stop。首版增加明确的 rename、pin、unpin、archive、unarchive、snooze、unsnooze action。rename 使用 title，snooze 使用带时区的明确时间；不把用户模糊时间静默转换成任意期限。

修改操作显式指定目标 thread，当前会话也使用宿主提供的当前 id。只在用户要求整理时使用，不赋予 Agent 自发隐藏、归档任务的产品策略。已有跨会话权限和子任务限制必须继续生效。

共享 Siphon 的配置、Marks 规则，扩展不可直接写 annotations。需要补的操作进入公开 Plan 或专用 Volume 原语；优先让整理效果与工具成功结果在同一提交中成立，不从工具绕过协议去调用内部 Runtime。

执行结束、用户已读、用户标为已处理与归档分别保留语义。首版不开放 Agent 自行 mark_seen/settle。归档不隐含停止，停止不隐含删除，重命名不改变历史内容。

暂缓队列编辑/重排、跨 thread 上下文合并、跨机器迁移。现有 inbox 不因参考 t3code queued run 而改写。

验收：工具与界面操作同一状态；客户端实时更新；重启保留；越权及子任务调用被拒绝；重复 pin/archive 不产生新的执行任务；归档不会停止原任务。

## P04：新任务工作区隔离

首版只支持创建新任务时选择当前目录或创建新 worktree，不迁移正在运行的 thread。沿用 thread.start，新增互斥的 workspace 选项，而非再造 task/agent 创建工具：current 或 worktree（基准分支、新分支）。缺省保持当前目录。

工作区准备复用 Git 能力；任务创建只有在工作区准备完成后发生。新的工作区路径、分支与任务配置一致。部分失败要返回具体状态，不能把 Git 创建成功等同于任务创建成功。恢复不能删除已被用户或任务使用的目录。

用户和 Agent 查看同一工作区信息。任务删除不默认删除 worktree。初始化脚本、脏目录修改转移、子任务自动 worktree 与运行中 handoff 后置。

验收：两个任务的文件修改互不干扰；目录/分支冲突明确拒绝；创建失败不出现虚假的可执行任务；删除任务不删除工作文件。实现前单独核对 Git 准备与任务创建间的失败处理，不提前引入事务协调框架。

## 后续候选

PR：先关联/查询，之后才考虑监听和唤醒；普通 git/gh 操作仍走 shell。

HTML 展示：在文件交付完成后独立评估隔离展示、资源引用和移动布局。HTML 下载不自动升级成执行网页的能力。

工具发现：工具描述体积成为实际问题后，复用现有 execute/MCP catalog 补渐进发现，不提前建立第二个注册中心。

## 执行顺序与完成标准

| 任务 | 交付 | 完成标准 |
| --- | --- | --- |
| P00 | 本设计与路线图入口 | 职责、名称、权限和验收清楚，单独 commit |
| P01a | 原样文件保存与 present_file 工具 | 原件、上限、权限、持久结果测试通过，单独 commit |
| P01b | 成果卡片与下载 | 真实文件交付/重开/源文件变化验收，单独 commit |
| P02 | 浏览器元素反馈闭环 | 选取、草稿、发送、历史读取验收，单独 commit |
| P03 | thread 整理操作 | 与界面规则一致、权限及恢复测试，单独 commit |
| P04 | 新任务 worktree | 隔离、冲突、部分失败验收，单独 commit |

每项仅扩展具体需要的公共原语，先复用已有实现。记录代码检查、自动化、桌面实测与手机实测各自覆盖范围。里程碑推送按用户授权进行，不将未完成的新能力自动合入 main。

## 参考源码与适用范围

本次研究基于 t3code `e8bfabaa`（2026-11-15），未以源码阅读替代运行验收。

- [附件 handler](https://github.com/pingdotgg/t3code/blob/e8bfabaa/apps/server/src/mcp/toolkits/attachment/handlers.ts)：附件发送属于会话输入，不等于成果展示。
- [HTML 发布](https://github.com/pingdotgg/t3code/blob/e8bfabaa/apps/server/src/htmlRender/HtmlRender.ts)：资源化交付、结构化引用。
- [浏览器调用](https://github.com/pingdotgg/t3code/blob/e8bfabaa/apps/server/src/mcp/toolkits/preview/handlers.ts) 与 [标注](https://github.com/pingdotgg/t3code/blob/e8bfabaa/apps/web/src/lib/previewAnnotation.ts)：操作与用户反馈两条路径。
- [会话工具](https://github.com/pingdotgg/t3code/blob/e8bfabaa/apps/server/src/mcp/toolkits/thread/tools.ts)：工具与产品对象一致，但 queued run 等语义不直接照搬。
- [Worktree 服务](https://github.com/pingdotgg/t3code/blob/e8bfabaa/apps/server/src/mcp/WorktreeMcpService.ts)：运行中迁移复杂，首版取新任务隔离这个子集。
