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

- 非 HTML 保存调用时实际读取的原始字节；HTML 将引用的本地图片内联到交付副本。源文件不修改，后续改变或删除不影响已交付的 HTML 与内联图片；远程 URL 不自动缓存。
- 不声称正在被其他进程写入的文件具有文件系统级原子快照；需要一致内容的生成器应先完成写入或原子替换。
- 首版复用 32 MiB 文件上限，读取过程中也执行上限，不能只检查 stat 后无界读取。
- 原图和模型用缩略图分离。不能使用当前 keep_image/show_image 的模型适配结果替代交付原件。
- 复用 thread/files 内容寻址；同一内容复用 blob，不等同于合并不同调用的展示记录。
- 不将所有成果字节自动写入模型上下文。ZIP 等模型不直接读取的文件仍可以作为下载成果。
- 界面复用现有图片预览；无法预览的文件提供下载，不伪装成可预览。PDF 先保证下载，内联预览另验收。
- HTML 通过独立隔离页面交互预览，可展开和下载；不在应用主文档上下文执行，下载返回已保存的交付副本。
- 参照 t3code，HTML 默认直接展示内容，不额外显示文件标题栏；桌面悬停或键盘聚焦时显示右上角展开与更多操作，触屏保持入口可见。下载已保存文件收进更多菜单，不要求用户发现右键或长按手势。
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

- `present_file` 负责保存并返回 `resource_link`；HTML 先内联本地图片，其他文件原样保存。客户端按 `file.media_type` 选展示，不直接依赖文件后缀。
- MIME 识别：已有图片/PDF/ZIP 内容签名优先；有效 UTF-8、无 NUL 的 `.html`/`.htm` 文本标为 `text/html`，扩展名忽略大小写。这是文件用途识别，不是 HTML 语法校验或安全证明。
- `browser { action: "preview", path: "report.html" }` 负责交付前自检，使用当前调用的 Machine namespace 读取；随后复用截图、控制台和交互能力。预览不等于正式交付。首版隔离子页面使用截图、坐标点击和控制台检查；read/CSS selector 尚未支持其独立 frame 内部节点，跨 frame 元素定位作为 browser 通用能力另行推进。
- `siphon-html` 提供读取验证、本地图片内联和唯一的预览宿主页。Web 通过现有鉴权客户端取得已保存 HTML；browser.preview 使用同一图片准备逻辑。主题与隔离注入只发生在预览内，不写回源文件。

宿主页为真正独立文档，避免 `srcdoc`/Blob 继承 hosted 主应用 CSP 后阻止脚本；Web 构建将相同静态资源复制到本地、桌面和 hosted 输出。主应用不开放 inline script。宿主页创建独立 sandbox 子 frame，允许内嵌 JS/CSS 和内嵌媒体，不允许读取父页面、Cookie、localStorage 或调用 Octoii API。HTTP/HTTPS 图片、字体、样式、脚本、媒体和 fetch 请求允许加载，CORS 仍由浏览器执行；宿主页的 frame-src 限制子页面跳到外部地址，弹窗与表单提交保持禁止。

父子通信仅包含加载 HTML、主题同步、准备完成、加载完成及尺寸信息；校验消息来源，尺寸受界面上限约束。不开放工具执行、权限申请或任意导航桥。需要完整网站导航、登录与弹窗的任务仍使用普通 browser 页面。原件本身不要求修改配色、去掉 vh 或填写 iframe 高度。这里采用浏览器 CSP/sandbox，未提供操作系统级网络隔离，不宣称封锁 WebRTC 等所有网络通道。

验收覆盖：真实 Chromium 中内嵌脚本与交互、远程脚本/样式/图片/fetch 可用，父 DOM 不可读且弹窗受限；保存后的本地图片不依赖源路径。构建含宿主页，hosted 主策略仍禁止 inline script。手机 Safari 和 Electron 实测分别记录，不以 Chromium 自动化代替。多文件 JS/CSS 打包仍不支持。

初版验证（当时外部资源尚未开放）：Rust 工作区 215 项通过、3 项忽略；Web 186 项通过。真实 Chrome 使用临时 profile 和随机端口验证共享宿主页及 React 文件卡片，覆盖脚本、点击、展开、尺寸钳制、来源校验、外部资源/自导航限制、读取拒绝与加载超时。验证未使用或停止用户原有 7600 服务。

## 本轮工具重构顺序（2026-11-16）

以下顺序优先于原 P02—P04 排期，保留已完成的文件交付能力；不恢复 goal，不改写 Beak、Siphon runtime 或 turn 生命周期。每项通过验收后单独提交。

| 顺序 | 范围 | 验收 |
| --- | --- | --- |
| T01 | browser / terminal / thread / delegate 原生结构化结果 | 输出 schema 与实际业务数据一致；文本、权限、分页、通知和 Plan 行为保留；前端不把扩展结果误认成内核文件结果 |
| T02 | execute 嵌套调用展示 | 默认一行真实调用摘要；展开后源码、输出独立折叠，各子工具保留原有展示；失败状态准确，成果不重复 |
| T03 | explorer / general_purpose 子任务 | 类型创建时确定；explorer 无修改工作区及 shell/MCP 旁路；通用子任务不超父权限；两类均不能再派生 |
| T04 | 浏览器元素反馈（P02） | 选取只生成输入草稿，用户发送；证据可持久读取；旧定位不当作当前有效定位 |
| T05 | 会话整理（P03） | 工具和界面共享重命名、置顶、归档规则；实时更新及重启恢复；权限不扩大 |

T02 的首版从 execute 的持久完成结果读取真实子调用记录，脚本失败也保留已结束的调用。默认只展示 execute 一行摘要，展开后可分别打开 Source code、Output 和各子工具原有详情；文件成果仍只在外层交付区显示一次。尚未完成或被强制停止、没有形成最终结果的脚本，不伪造子调用完成记录；执行中实时子调用及权限等待展示另行补齐。无真实子调用起止时间时不显示编造的耗时。文件操作按记录结果路径（没有时取调用参数路径）去重，重复操作显示如 `edited 1 file (4 edits)`；路径大小写保持原样，未知路径只计操作次数，不声称文件数。失败、拒绝、中断不计入成功文件操作。

read/write/edit/glob/grep 保留；shell 用于会结束的命令，terminal 用于持续或交互进程。execute 只组合现有工具，不增加批量任务体系。worktree（P04）排在上述项目后。

## T03：两类子任务

只增加 `explorer` 和 `general_purpose`，继续使用 delegate 和现有子 thread，不创建新的 Agent 生命周期。

- 新建时 `kind` 缺省为 `general_purpose`；`explorer` 用于工作区阅读、搜索和分析。续接时省略 kind 保留原类型，显式要求不同类型则拒绝，不投递任务。
- 类型随 Parent 持久保存，不属于可编辑 Config。既有未标类型的子线程沿用原本的 general_purpose 语义；非法类型不能静默放行。
- explorer 只可调用 read、glob、grep、execute。execute 内仍执行相同准入；shell、写文件、browser.evaluate、MCP 和跨会话工具不可用。FullAccess 或已有授权不能绕过这一工具可用集。内部保存调用结果与模型图片不等于修改工作区。
- general_purpose 继续继承父线程的模型、模式、工作区和权限规则；两类子任务均不可再次委派，也不可借 thread 访问其他会话。
- 模型工具声明、execute bindings 与实际调用准入使用同一规则；explorer 不触发 MCP catalog 连接或信任询问，不提供 provider 原生工具作为旁路。

验证创建与重启后类型保持、续接不升级、直接调用及 execute 内越界均无工作区副作用或审批请求、通用子任务权限继承，以及 Plan 层对子线程再次派生的拒绝。

## P02：浏览器反馈

浏览器继续运行在节点。Agent 工具与客户端操作同一 page，不新建 Electron 专属浏览器，也不复制 Browser 的状态。

第一步是用户反馈闭环：点击“指出问题”进入选取模式，选择元素，生成输入上下文草稿，用户填写要求并发送。选择动作不等于提交，也不自动启动任务。

反馈至少含 page id、当时 URL、选取时间、元素描述、可用的定位信息、当时视口与截图引用。首版只捕获解决定位问题所需的有限内容，不保存整个 DOM。截图按会话文件保存；页面关闭后历史反馈仍可阅读。

反馈里的旧 locator/ref 是历史证据，不保证可直接执行；页面导航或引用失效后 Agent 必须重新读取。选取模式下用户点击只选取元素，不触发页面按钮。取消选择不提交消息。

先完成桌面点击选取；触摸选择与手机端交互单独验收。视口已有 resize 能力，客户端提供固定尺寸预设时应明确谁控制当前视口，不能让多个客户端自动布局相互覆盖。Agent 如需设置尺寸/外观，补 browser 的明确 action，复用同一实现。

归属为现有 siphon-browser 协议与 Web browser/composer 扩展。暂缓录屏、自由绘制、源码框架追踪、账号导入。

T04 实施边界：

- `browser.pick` 是用户侧 RPC，不增加 Agent 工具。请求明确所属 thread、page、已绘制画面的 CSS 坐标、预期 URL 和视口；服务端核验页面归属及 browser.operate、threads.operate 权限。
- 复用 `Volume::keep_image` 保存模型可读取的截图，现有 `Context::show_image` 使用同一原语。返回截图 FileRef、捕获时间、文档标识、视口、选取点及有长度上限的元素描述；不保存完整 DOM、outerHTML 或输入框值。
- 首版选取主文档元素，子 frame 明确拒绝。捕获前后核对文档、URL、视口及目标元素；无关 DOM 更新不使选取失败。截图与用户此前看到的流式帧并非原子快照，反馈卡片供发送前检查，不声称还原任意旧帧。
- 选取模式复用当前画布，截住网页鼠标、滚轮、键盘和输入法事件；切换不重建页面。Escape 取消，点击只捕获，不触发目标页面操作。
- 草稿保留可删除的截图与元素上下文。发送时复用 input_text/input_image，成功仅清除本次提交的内容，发送期间新增的草稿保留；失败不清空。截图随会话文件读取，历史定位不自动重放。

验收：选取元素加文字后模型能读到证据；选取不触发页面行为；发送前可删除反馈；导航后旧证据仍显示但旧引用不能误指新元素；重开历史截图可读。

## P03：会话整理

扩展现有 thread 工具，保留 list/search/read/start/send/stop。首版增加明确的 rename、pin、unpin、archive、unarchive、snooze、unsnooze action。rename 使用 title，snooze 使用带时区的明确时间；不把用户模糊时间静默转换成任意期限。

修改操作显式指定目标 thread，当前会话也使用宿主提供的当前 id。只在用户要求整理时使用，不赋予 Agent 自发隐藏、归档任务的产品策略。已有跨会话权限和子任务限制必须继续生效。

共享 Siphon 的配置、Marks 规则，扩展不可直接写 annotations。需要补的操作进入公开 Plan 或专用 Volume 原语；优先让整理效果与工具成功结果在同一提交中成立，不从工具绕过协议去调用内部 Runtime。

T05 采用两个具体 Plan 步骤：Rename 只投递 title 配置，Mark 复用现有 MarksChange。重命名返回 `rename_requested` 和 delivery，表示请求已入队，标题由线程主进程按原有顺序应用；不能把入队说成已生效。标记变更与工具结果一起提交，不复活已结束的主进程。工具不开放已读、已处理等标记。当前会话可整理，跨会话仍核对目标存在性和运行模式，子任务不可使用这些操作。

snooze 必须是带 UTC offset 的未来时间，工具调用与 Plan 应用时均核验；过期时间明确拒绝，不隐式变成 unsnooze。unsnooze 使用独立 action。列表同时返回置顶和提醒截止时间，供后续调用直接读取。

执行结束、用户已读、用户标为已处理与归档分别保留语义。首版不开放 Agent 自行 mark_seen/settle。归档不隐含停止，停止不隐含删除，重命名不改变历史内容。

暂缓队列编辑/重排、跨 thread 上下文合并、跨机器迁移。现有 inbox 不因参考 t3code queued run 而改写。

验收：工具与界面操作同一状态；客户端实时更新；重启保留；越权及子任务调用被拒绝；重复 pin/archive 不产生新的执行任务；归档不会停止原任务。

## P04：新任务工作区隔离

首版只支持创建新任务时选择当前目录或创建新 worktree，不迁移正在运行的 thread。沿用 thread.start，新增互斥的 workspace 选项，而非再造 task/agent 创建工具：current 或 worktree（基准分支、新分支）。缺省保持当前目录。

工作区准备复用 Git 能力；任务创建只有在工作区准备完成后发生。新的工作区路径、分支与任务配置一致。部分失败要返回具体状态，不能把 Git 创建成功等同于任务创建成功。恢复不能删除已被用户或任务使用的目录。

用户和 Agent 查看同一工作区信息。任务删除不默认删除 worktree。初始化脚本、脏目录修改转移、子任务自动 worktree 与运行中 handoff 后置。

验收：两个任务的文件修改互不干扰；目录/分支冲突明确拒绝；创建失败不出现虚假的可执行任务；删除任务不删除工作文件。实现前单独核对 Git 准备与任务创建间的失败处理，不提前引入事务协调框架。

## 桌面验收后的 UI 交互修复（2026-11-16）

以下四项已实现，并通过内置浏览器在隔离节点的真实历史会话中复查。

1. **展开工具时显示左侧垂直导轨。** 导轨应清楚关联工具摘要与其展开内容，并体现嵌套子调用的层级；折叠后不留下多余导轨。
2. **codemode / execute 脚本独立折叠。** 脚本默认收起，可手动展开和折叠；打开工具组或子调用详情，不应顺带展开整段脚本。区分脚本源码与执行结果，不将源码常驻显示。
3. **回复结束后收起工作过程。** 每轮结束后，工作过程归入可展开的 `Worked …` 摘要，最终回复保持可见，不能把整个会话或最终答案一起折叠。

4. **Thread details 保持浮窗外观并参与布局，窄屏自动收起。** 右上角保持自然高度的浮窗卡片、圆角和阴影，不铺满高度，也不绘制整列分隔线。展开时为详情预留右侧空间，对话与输入框在剩余区域内对齐，不能遮挡消息和操作；当线程区域不足以容纳可读的对话与详情时，自动收起详情并保留标题栏入口，按需打开。判断依据是扣除导航栏、浏览器等工作面板后的实际可用空间。自动收起不覆盖用户手动显示/隐藏的偏好。参考本地 t3code `3ffd38ef7f` 的 `ChatRightPanel` 并排/抽屉布局与 `usePreviewPanelInlineSize` 对相邻对话的空间保留；其窄屏切换抽屉不等同于本产品要求的自动收起，不直接照搬断点。

验收时分别检查运行中、刚结束、手动展开/收起和重开历史会话；包含普通工具、execute 嵌套调用及多轮对话。运行中的工作默认展开，未手动操作时随完成自动收起；读者手动选择的展开/收起状态在组件挂载期间保留，重新打开历史会话默认收起。布局验收覆盖连续缩放窗口、导航栏与浏览器/文件面板开关、不同对话宽度设置，检查详情不遮挡内容、输入框与消息对齐、自动收起及手动再次打开。日常界面验收优先使用内置浏览器，Electron 专属行为另行验收。

本轮验证：内置浏览器检查真实历史会话默认折叠、手动展开、嵌套工具导轨、execute 源码独立开关及 HTML 交付卡片仍可见；1440px、1000px 和 390px 窗口检查详情占位、自动收起、窄屏按需打开及无页面横向溢出。工作面板开关会重新分配空间；手动关闭详情后缩放窗口不会重新打开它。运行到完成的状态变化另由组件交互测试覆盖，本轮没有重新发起模型任务。浮窗外观修正后再次通过内置浏览器检查：1440px 下卡片自然高度且不遮挡对话/输入框；1000px 下自动收起、按需打开；300px 高窗口内卡片限制高度并独立滚动。

## HTML 视觉回复（2026-11-16）

沿用“生成文件 → browser.preview 自检 → present_file 交付”，不新增 HTML 专用工具。生成指导放在 `extensions/siphon-files/src/present_file.md`，作为 `present_file` 的工具描述，在生成前可见。

- HTML 与回复正文同宽、同起点，不使用工具缩进，也不叠加外层卡片边框。具体报告内部可有自己的分区。
- 对话内高度跟随内容增长和收缩，随对话滚动；80px 是最小展示高度，20,000px 仅为异常防护上限。展开到独立对话框后使用视口内滚动。
- 基础预览机制参考 t3code，排版与 Octoii 对话统一：HTML 14px / 1.625，根节点 8px 可覆盖内边距，body 零外边距，--radius 为 0.625rem。正文与 HTML 共享 --text-body、--text-body--line-height、--text-heading-1/2/3 和 --text-heading--line-height；字号使用 px，避免 iframe 根字号与应用 rem 基准不同导致缩放。注入语义色、对应前景色/状态底色、六种图表色及 sans/mono 字体。
- 首次加载传入主题变量；宿主明暗主题或配色改变时原位更新样式，不重载脚本或清空交互状态。默认背景、文字、字体由宿主提供，生成内容使用语义颜色和图表颜色变量。
- `siphon-html` 的同一隔离运行时供对话预览和 browser.preview 使用。没有宿主主题的浏览器预检使用系统明暗主题默认值。预检不能被描述为已经读取了用户当前配色。
- 页面根节点使用自然高度，不使用视口高度单位或百分比高度。用户明确要求的网页原型可保留自己的视觉设计；不强行覆盖文件中的显式样式。下载仍返回未经主题注入的原始文件，生成内容应为 CSS 变量提供可独立使用的默认值。
- 原有 sandbox、CSP、父子窗口消息来源检查保持有效；主题消息只提供样式，不授予宿主访问或执行工具的能力。

参考本地 t3code `3ffd38ef7f` 的 `HTML_RENDER_LAYOUT_GUIDE`、`HTML_RENDER_THEME_GUIDE`、主题消息同步和 `htmlRenderFrameHeight`。借鉴其运行时与生成指导一致的约定，保留现有 present_file/browser.preview 工具入口；远程资源与本地图片内联按用户确认开放。

本轮验收使用真实 `HtmlPreview` 组件的隔离样例（非模型新生成会话）：内置浏览器验证明暗切换、计数状态保留、内容展开超过 1,700px 后收回、390px 宽下重排且无页面横向溢出。Web 208 项测试及 `siphon-files` 3 项测试通过；共享预览运行时的 3 项真实 Chromium 测试覆盖默认主题、原位主题更新、增长/回缩和隔离限制。

## Thread 正文排版（2026-11-16）

参考本地 t3code `ChatMarkdown.tsx` 的 `text-sm leading-relaxed` 和 `index.css`，正文从 15px 改为 14px / 1.625；标题按产品确认采用 16/15/14px 层级，段落、列表、代码与表格区块间距统一为 0.65rem，强调文字为 700 字重，行内代码为正文 0.85 倍。用户消息和 Agent 回复共用这套 Markdown 排版；界面辅助文字和工具行保留 11–13px，代码块保留现有 13px。

调整标题层级之前，已有真实会话的内置浏览器检查确认正文计算样式为 14px / 22.75px，HTML 预览与其父内容区同宽（736px），内容自然高度约 1,500px。已有报告的显式配色继续保留。ChatGPT 重新登录后，新建验收会话已真实生成并交付 `delivery-review.html`：内置浏览器确认预览宽 736px、自然高度 733px、主题变量生效、计数状态为 1；最终回复正文 14px / 22.75px、二级标题 18px、表格 14px。嵌套 iframe 的折叠控件受浏览器自动化坐标限制，本轮未完成该控件的手工复核；增长/收起仍以共享预览运行时真实 Chromium 测试为依据。

生成指导要求普通 HTML 报告继承上述变量，指标及计数器采用 16px，通过字重和颜色突出；仅明确要求海报、网站设计等场景才采用大字号。浏览器预览的独立运行默认值与对话变量一致；原有文件中的显式字号不被强制覆盖，重新生成后才应用新的指导。

同一真实 thread 再生成并纠正变量名后，实测 HTML 与正文的 h1 均为 16px、h2 均为 15px，HTML 正文为 14px / 22.75px，指标和计数器为 16px；成品引用实际 --text-* 变量。预览宽度与回复区一致（721px），自然高度 671px，已保存 HTML 与正文同屏截图。Web 208 项测试、类型和 lint 检查、3 项真实 Chromium 预览测试及 Rust 格式检查通过。

## Codemode 摘要与折叠验收（2026-11-16）

真实 thread 中一个 execute 对 `codemode-review.txt` 做一次写入、三次编辑，显示 `Edited 1 file (4 edits)`。默认摘要下没有脚本输出；展开摘要后 Source code 和 Output 仍各自折叠，点击后分别显示源码及 JSON，子工具保留独立详情和导轨；再次收起摘要成功。已保存摘要及展开截图。Web 210 项测试、类型检查、lint 和构建通过，覆盖重复路径、结果路径优先、大小写区分、未知路径及失败调用排除。

## HTML 资源支持（2026-11-16）

按用户确认开放 HTTP/HTTPS 图片、字体、样式、脚本、媒体和 fetch；远程 URL 保持原样，不自动缓存，不保证离线可用。真实 Chromium 回归验证远程脚本执行、样式应用、图片加载、跨源数据请求，以及父 DOM 隔离和弹窗限制。

本地图片由 siphon-html 在 browser.preview 和 present_file 中统一准备：支持带引号的图片文件路径、CSS url(...)，相对路径以 HTML 所在目录为基准，按实际字节识别图片格式，受 Machine 隐藏路径和32MiB展开后上限限制。源文件不变，已交付 HTML 保存内联图片；不支持 srcset、任意本地 JS/CSS 打包。真实 thread 已生成并交付带本地 SVG 的 HTML，内置浏览器确认交付图片已转为 data:image/svg+xml;base64 且40px图标加载成功。

本轮相关验证：HTML/files 10 项、核心文件保存4项、真实 Chromium预览3项通过；两个扩展 all-targets Clippy、Rust格式、Web与节点构建通过。额外核心库全量运行86项通过、10项失败、1项忽略：其中8项命令/调用/命名空间测试报告嵌套 sandbox-exec 的 sandbox_apply Operation not permitted，另2项线程进程清理超时；不将这次全量运行记录为通过。

## 后续候选

PR：先关联/查询，之后才考虑监听和唤醒；普通 git/gh 操作仍走 shell。

HTML 后续：多文件资源打包、PDF 内联展示分别验收。主题联动已实现，隔离样例已验证原位切换，真实生成会话已验证当前主题渲染。HTML 可引用远程资源；不自动把远程资源当成已持久保存。

工具发现：工具描述体积成为实际问题后，复用现有 execute/MCP catalog 补渐进发现，不提前建立第二个注册中心。

## 工具契约实施进度

- 已完成：统一 content / structuredContent / metadata；execute 完整结果重放及显式文本、图片输出。
- 已完成：glob/grep 结构化结果与输出 schema；搜索 UI 直接消费数据，去除展示文字解析。保留原有分页与权限机制。
- 已完成：execute 失败状态准确报告；所有调用统一在完成边界裁切模型文本，并原子保存完整正文。
- P01：原样文件保存、resource_link、成果卡片与鉴权下载已实现；自动化覆盖原件、源文件删除、持久结果及鉴权下载读取；浏览器实际保存、Electron 与 iPhone 下载实测仍待验收。
- P01c/P01d：独立 HTML 模块、交互预览与展开、browser.preview 自检已实现。截图、坐标操作、控制台与隔离策略通过真实 Chromium 验证；iPhone Safari / Electron 原生壳实测仍待验收。展开会创建新预览，不保证保留临时交互状态。
- T01 已完成：四个扩展提供原生结构化结果与输出 schema，前端文件结果转换限定到内核工具。30 项受影响 Rust 测试通过（含真实 Chromium 与终端进程）、186 项 Web 测试通过；严格 Clippy、类型、lint 与格式检查通过。通用 Calling 测试夹具补建 inbox，生产通知机制不变。
- T02 已完成：execute 持久化真实已结束子调用；默认摘要，源码与输出独立折叠，展开工具详情。14 项 execute 测试、193 项 Web 测试、严格 Clippy、类型、lint、格式与构建通过。真实 Chromium 组件示例验证折叠、子命令详情、失败/拒绝徽标与 390px 触屏布局；不等同于 Electron/iPhone 真机验收。
- T03 已实现：子任务类型持久保存，创建缺省 general_purpose，续接不改变类型；explorer 在声明、脚本绑定与执行入口均限制为 read/glob/grep/execute，MCP 收集在连接和信任询问之前退出。界面显示实际存储的类型。验证覆盖恢复审批、已有授权和 FullAccess 不绕过、重开与续接保留类型、原生工具及 MCP 不暴露；102 项受影响 Rust 测试通过、1 项忽略；193 项 Web、5 项客户端测试、协议生成、严格 Clippy、类型与格式检查通过。一次已有停止测试的日志尾部断言未通过，隔离及随后完整运行通过；保留原断言并增加日志诊断，未改停止逻辑。
- T04 已完成：browser.pick、会话持久截图、画布选取和可移除的反馈草稿接通，复用现有消息与图片预览。浏览器 18 项测试通过（含新增 6 项真实 Chromium 捕获测试），核心 91 项通过、1 项既有 live 测试忽略、4 项核心集成测试通过；Web 203 项、客户端 6 项、协议生成、严格 Clippy、类型、lint、格式与 Web 构建通过。真实 Chromium 组件示例验证无输入转发、无重挂/视口改变、取消迟到捕获、图片展开、删除反馈、发送失败保留及成功仅清提交快照；该示例使用隔离客户端，不等于全应用端到端或 Electron/iPhone 真机验收。主文档之外的 frame 选取与手机触摸仍暂缓。
- T05 已完成：thread 新增 rename、pin/unpin、archive/unarchive、snooze/unsnooze；配置请求与标记复用原有规则，列表文本和结构化结果同时提供标记状态，界面显示对应动作。114 项受影响 Rust 测试通过、1 项既有 live 测试忽略；严格 Clippy、类型、lint 和格式检查通过。覆盖自己会话重命名、重开保持、已删除目标拒绝、模式及子任务限制、同计划操作顺序、重复唤醒去重和失败不提交任何部分变更。归档及提醒不停止或启动工作；重命名只报告请求入队。
- 本轮 T01—T05 已完成，Rust workspace 的 all-targets 编译检查通过。后续候选为 P04 新任务 worktree；尚未开始实现。

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
- [Chromium DOM 协议](https://github.com/ChromeDevTools/devtools-protocol/blob/master/json/browser_protocol.json) 与 [Runtime 协议](https://github.com/ChromeDevTools/devtools-protocol/blob/master/json/js_protocol.json)：T04 核对命中节点的 frameId、整数坐标、无 userGesture 的对象读取与对象释放；协议资料不代替实际 Chromium 验收。
