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

不使用 `publish: true`、`send_to_model: false` 之类切换行为的标志。文件交付、模型读文件和 Agent 的预览自检是不同操作。尺寸、外观等模式值是数据，可以作为明确的字段。

参数中不让模型传当前节点、当前会话、调用者权限等宿主已经知道的信息。操作其他会话时必须显式给出目标。结果提供稳定 id/ref，临时 URL 由客户端访问层生成，不进入持久工具记录。

## 结果契约前置试验

现有工具输出与 execute 的设计试验见 [TOOL_RESULTS.md](TOOL_RESULTS.md)。先验证原生结果的可编程数据与资源输出，再实现文件交付。工具结果统一采用 content + structuredContent + metadata；P01 使用 content 中的 resource_link 表达已交付文件。

## P01：文件成果交付

### 用户行为

用户要求生成报告、截图或其他文件，Octoii 完成后调用：

```json
{"path":"reports/result.pdf","title":"验证报告"}
```

`path` 必需，相对当前工作区或为当前工具有权读取的绝对路径；`title` 可省略。首版一次一个普通文件。目录、打包、跨会话发送和互联网分享不包含在此接口中。

工具成功结果：

```json
{"content":[{"type":"input_text","text":"Presented result.pdf (12345 bytes)."},{"type":"resource_link","file":{"ref":"sha256:…","filename":"result.pdf","media_type":"application/pdf"},"size":12345}],"metadata":{"title":"验证报告"}}
```

复用现有 FileRef；resource_link 是持久展示依据，不重复放入 structuredContent。各模型协议将链接转换为文件描述，不读取文件字节。metadata.title 是调用展示标题，filename 保留原文件名。后续若需要在正文引用成果，复用文件引用，不建立另一套成果 id。

### 保存与展示语义

- 保存调用时实际读取的原始字节；源文件后续改变或删除不影响已交付成果。
- 不声称正在被其他进程写入的文件具有文件系统级原子快照；需要一致内容的生成器应先完成写入或原子替换。
- 首版复用 32 MiB 文件上限，读取过程中也执行上限，不能只检查 stat 后无界读取。
- 原图和模型用缩略图分离。不能使用当前 keep_image/show_image 的模型适配结果替代交付原件。
- 复用 thread/files 内容寻址；同一内容复用 blob，不等同于合并不同调用的展示记录。
- 不将所有成果字节自动写入模型上下文。ZIP 等模型不直接读取的文件仍可以作为下载成果。
- 界面复用现有图片预览；无法预览的文件提供下载，不伪装成可预览。PDF 先保证下载，内联预览另验收。
- HTML 通过独立隔离页面交互预览，可展开和下载；不在应用主文档上下文执行，下载保持原始字节。
- 文件必须经节点现有鉴权读取；持久记录不存宿主绝对路径作为唯一访问方式，不依赖桌面本机 file URL。

execute 自动保留成功嵌套调用的 resource_link，无需 text() 或 image()；其他文本、图片仍需显式输出。卡片在外层调用结果完成后出现；嵌套调用的 metadata.title 不自动汇入外层结果，卡片仍保留原文件名。尚不提供脚本运行中的即时交付展示，也不保证未形成结果的强制中止能展示文件。

### 权限与失败

文件读取通过当前工具的 machine/namespace，不能调用宿主文件 API 绕过限制。把内容加入会话是产品写操作，不能仅因为不改工作区就标为 Reads；首版纳入现有 Acts 权限判定，不增加旁路。

非法路径、非普通文件、超限、存储失败必须明确拒绝。当前 Beak 的 FileKind 无法区分宿主普通文件与 FIFO/device；Siphon 在 namespace.stat 允许访问后对带宿主 file_id 的对象补充文件类型拒绝检查，字节仍通过 namespace.read。该检查与读取并不原子，不能保证并发将普通文件替换为 FIFO 时仍不阻塞；完整保障需要后续 Beak read_regular 原语，不能把本轮检查描述为 race-free。文件存储提交完成不等于工具结果已经提交；只有持久工具结果出现后客户端才展示为已交付。复用现有工具中断与恢复语义，不因确认丢失重新读取可能已经变化的源文件并声称是原成果。

文件读取与保存属于有边界的业务原语。首版优先沿用现有 show_image 的先保存、再返回结果模式；两次提交间崩溃可能留下未引用 blob，需要如实记录并沿用 thread 生命周期处理，不额外建设资源事务框架。具体公开原语放在 Context/Volume 最合适的位置，由实现时对照现有文件访问接口确定。

### 归属

- Siphon：文件原样保存原语、结果契约与权限路径。
- 新的 `extensions/siphon-files`：注册 `present_file`，只实现具体工具业务。
- Web：文件成果卡片与打开/下载行为，复用现有鉴权资源访问。
- Electron：仅处理必要的原生下载交互，不承载成果存储。

### 验收

生成 PDF、PNG、文本及 ZIP，交付后关闭重开仍可读取；修改/删除源文件后交付字节不变；原图字节不被模型压缩；授权失败不能读取；超限不造成大内存读取；失败的 present_file 不产生交付卡片；execute 抛错仍保留此前已成功调用的交付链接。远程 Web 使用节点资源路径验证，iPhone 真机验收另行记录，不能以桌面通过代替。

P01a/P01b 自动化：Rust 工作区 212 项通过、3 项忽略，Web 183 项通过；工作区 Clippy、TypeScript 类型检查、lint 与格式检查通过。下载测试覆盖鉴权客户端返回的文件名、类型、字节及拒绝响应；尚未覆盖浏览器保存对话框、实际落盘或下载 URL 的定时回收。Electron 壳策略已做源码核对，桌面与 iPhone 实测保持待验收。

## P01c / P01d：HTML 交付与自检

HTML 独立实现为 `extensions/siphon-html` 普通 Rust 库；不注册额外工具，不拥有 browser 生命周期，不创建成果数据库。Siphon 只提供通用文件读取、类型识别、权限和持久化原语。

- `present_file` 仍负责原样保存并返回 `resource_link`。客户端按 `file.media_type` 选展示，不直接依赖文件后缀。
- MIME 识别：已有图片/PDF/ZIP 内容签名优先；有效 UTF-8、无 NUL 的 `.html`/`.htm` 文本标为 `text/html`，扩展名忽略大小写。这是文件用途识别，不是 HTML 语法校验或安全证明。
- `browser { action: "preview", path: "report.html" }` 负责交付前自检，使用当前调用的 Machine namespace 读取；随后复用截图、控制台和交互能力。预览不等于正式交付。首版隔离子页面使用截图、坐标点击和控制台检查；read/CSS selector 尚未支持其独立 frame 内部节点，跨 frame 元素定位作为 browser 通用能力另行推进。
- `siphon-html` 提供读取验证和唯一的预览宿主页。Web 通过现有鉴权客户端取得保存原件，再将文本交给宿主页；browser 加载同一宿主页，交给它同样的原始文本。隔离与展示注入只发生在预览内，不写回文件。

宿主页为真正独立文档，避免 `srcdoc`/Blob 继承 hosted 主应用 CSP 后阻止脚本；Web 构建将相同静态资源复制到本地、桌面和 hosted 输出。主应用不开放 inline script。宿主页创建独立 sandbox 子 frame，允许内嵌 JS/CSS 和内嵌媒体，不允许读取父页面、Cookie、localStorage 或调用 Octoii API。外部资源加载与 fetch 请求默认禁止；宿主页的 frame-src 限制子页面跳到外部地址，不能仅以 sandbox 宣称禁止联网。

父子通信仅包含加载原文、准备完成、加载完成及尺寸信息；校验消息来源，尺寸受界面上限约束。不开放工具执行、权限申请或任意导航桥。HTML 中需要真实网站或在线服务的任务使用普通 browser 页面。原件本身不要求修改配色、去掉 vh 或填写 iframe 高度。这里采用浏览器 CSP/sandbox，未提供操作系统级网络隔离，不宣称封锁 WebRTC 等所有网络通道。

验收覆盖：真实 Chromium 中内嵌脚本与交互工作，外部图片/fetch/自导航被阻止，应用凭据与父 DOM 不可读；原件下载字节不变，历史预览不依赖源路径；构建含宿主页，hosted 主策略仍禁止 inline script。手机 Safari 和 Electron 实测分别记录，不以 Chromium 自动化代替。多文件相对资源与网络访问不在首版范围。

本轮验证：Rust 工作区 215 项通过、3 项忽略；Web 186 项通过。真实 Chrome 使用临时 profile 和随机端口验证共享宿主页及 React 文件卡片，覆盖脚本、点击、展开、尺寸钳制、来源校验、外部资源/自导航限制、读取拒绝与加载超时。验证未使用或停止用户原有 7600 服务。

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

HTML 后续：多文件资源打包、主题联动、PDF 内联展示分别验收。首版 HTML 自包含；不自动把网络资源当成已持久保存。

工具发现：工具描述体积成为实际问题后，复用现有 execute/MCP catalog 补渐进发现，不提前建立第二个注册中心。

## 工具契约实施进度

- 已完成：统一 content / structuredContent / metadata；execute 完整结果重放及显式文本、图片输出。
- 已完成：glob/grep 结构化结果与输出 schema；搜索 UI 直接消费数据，去除展示文字解析。保留原有分页与权限机制。
- 已完成：execute 失败状态准确报告；所有调用统一在完成边界裁切模型文本，并原子保存完整正文。
- P01：原样文件保存、resource_link、成果卡片与鉴权下载已实现；自动化覆盖原件、源文件删除、持久结果及鉴权下载读取；浏览器实际保存、Electron 与 iPhone 下载实测仍待验收。
- P01c/P01d：独立 HTML 模块、交互预览与展开、browser.preview 自检已实现。截图、坐标操作、控制台与隔离策略通过真实 Chromium 验证；iPhone Safari / Electron 原生壳实测仍待验收。展开会创建新预览，不保证保留临时交互状态。
- 下一项：P02 浏览器反馈。两类子任务的权限与执行方式单独落地。

## 执行顺序与完成标准

| 任务 | 交付 | 完成标准 |
| --- | --- | --- |
| P00 | 本设计与路线图入口 | 职责、名称、权限和验收清楚，单独 commit |
| P01a | 原样文件保存与 present_file 工具 | 原件、上限、权限、持久结果测试通过，单独 commit |
| P01b | 成果卡片与下载 | 真实文件交付/重开/源文件变化验收，单独 commit |
| P01c | HTML 独立模块与交互预览 | 原件、脚本隔离、网络限制、构建与界面检查，单独 commit |
| P01d | browser HTML 自检 | 使用同一宿主页，权限读取、截图与交互验收，单独 commit |
| P02 | 浏览器元素反馈闭环 | 选取、草稿、发送、历史读取验收，单独 commit |
| P03 | thread 整理操作 | 与界面规则一致、权限及恢复测试，单独 commit |
| P04 | 新任务 worktree | 隔离、冲突、部分失败验收，单独 commit |

每项仅扩展具体需要的公共原语，先复用已有实现。记录代码检查、自动化、桌面实测与手机实测各自覆盖范围。里程碑推送按用户授权进行，不将未完成的新能力自动合入 main。

## 参考源码与适用范围

初次研究基于 t3code `e8bfabaa`（2026-11-15）；HTML 方案于 2026-11-16 复核 main `dfeaf7b13786712cc350fa35abea17715fdef987`，所引用的相关核心实现无变化。源码研究与 Octoii 的实际运行验收分别记录。

- [附件 handler](https://github.com/pingdotgg/t3code/blob/e8bfabaa/apps/server/src/mcp/toolkits/attachment/handlers.ts)：附件发送属于会话输入，不等于成果展示。
- [HTML 发布](https://github.com/pingdotgg/t3code/blob/e8bfabaa/apps/server/src/htmlRender/HtmlRender.ts)：资源化交付、结构化引用。
- [浏览器调用](https://github.com/pingdotgg/t3code/blob/e8bfabaa/apps/server/src/mcp/toolkits/preview/handlers.ts) 与 [标注](https://github.com/pingdotgg/t3code/blob/e8bfabaa/apps/web/src/lib/previewAnnotation.ts)：操作与用户反馈两条路径。
- [会话工具](https://github.com/pingdotgg/t3code/blob/e8bfabaa/apps/server/src/mcp/toolkits/thread/tools.ts)：工具与产品对象一致，但 queued run 等语义不直接照搬。
- [Worktree 服务](https://github.com/pingdotgg/t3code/blob/e8bfabaa/apps/server/src/mcp/WorktreeMcpService.ts)：运行中迁移复杂，首版取新任务隔离这个子集。
