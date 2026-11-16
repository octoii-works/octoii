# 工具结果契约

日期：2026-11-16。采用 Pi 的职责划分，将 details 命名为 metadata。本方案替换此前 value/data + attachments 的试验。

## 统一结果

```ts
type ToolResult<T = unknown, M = unknown> = {
  content: Content[];
  structuredContent?: T | null;
  metadata?: M | null;
};
```

- content：模型消费的文本、图片、文件资源。复用现有 InputContent（input_text、input_image、input_file、resource_link）和 thread 内 FileRef，不另设 attachments 或资源注册中心。
- structuredContent：程序读取的业务结果。原生工具用类型定义其数据并声明输出 schema；有 schema 的 MCP 工具保留服务器提供的 schema。无结构化结果时该字段可以缺省或为 null，不从展示文字猜测业务数据。
- metadata：可选日志和界面信息。目前界面读取 title 作为工具结果的展示标题；默认名称、图标及 renderer 仍由工具注册定义决定。状态、耗时、调用 ID 与父子关系属于执行记录。

Rust 内部的 Told 继续携带 instructions 和 plan，用于上下文发现及原子执行；它们不暴露给脚本，也不混入 metadata。Rust 字段 structured_content 在 JSON 中序列化为 structuredContent。

不提供旧 output/structured 字段别名，不回退到字符串工具返回值，不兼容旧版运行中脚本 checkpoint。提供给模型的 FunctionCallOutput 是模型协议的封装，保留其自身 output 字段，不与工具结果字段混淆。

## 直接调用与 execute

直接调用将 content 交给模型，structuredContent 和 metadata 随调用结果保存，供程序和界面使用。界面可读取业务数据，例如退出码和改动行数，无需把这些数据再复制到 metadata。

execute 中 tools.* 返回完整的结果对象。脚本显式选择模型输出：

```js
const read = await tools.read({ path: "src/main.rs" });
text(read.structuredContent.text);

const shot = await tools.read({ path: "shot.png" });
image(shot.content.find(part => part.type === "input_image"));

const ran = await tools.shell({ command: "cargo test" });
return { ended: ran.structuredContent.ended };
```

text() 和 console.log 写入文本输出；return 保持现有 JSON 返回语义。image() 接受当前脚本工具调用返回的 input_image 内容块，保留其资源引用，不让脚本搬运 base64，不接受任意未返回的资源引用。当前展示顺序为文本和返回值在前、选中的图片在后。

嵌套调用的 resource_link 自动保留；它代表已发生的文件交付，各模型协议只发送引用描述，不读取原件。普通文本、图片不自动追加到模型上下文。资源按原有 thread 文件机制保存；metadata 不自动输出。脚本失败仍保留此前显式输出的文本和图片；已完成工具调用的副作用不回滚。

调用失败继续通过已有 Refusal/CallStatus 处理，脚本收到 rejection；命令非零退出属于命令结果，脚本检查 ended。execute 抛错、耗尽步骤或无法恢复结果时，调用状态为 tool-error，已显式输出的内容仍保留。程序完成返回 Finished（状态与 Told），不能仅因底层进程 Completed 就视为工具成功。这里不额外建立错误代码体系，也不改变 turn 的停止语义。

所有调用在 Calls 的完成边界统一执行现有 30,000 字符文本预算，包括普通工具、程序工具和失败结果。超限正文保留首尾；完整正文与完成结果在同一次提交中写入 thread 的 outputs。图片及 structuredContent 不因文本裁切丢失。若宿主挂载了 threads 视图，结果给出 read 可读的完整输出路径；未挂载时明确说明，不把内部 volume 路径伪装成可读文件路径。该预算是模型文本预算，不是所有业务数据或图片的统一 token 配额。完整正文按原文保存；read 仍遵守既有分页与单行长度上限，大单行 JSON 通过 read 看到的内容仍可能裁切，此次不新增字符范围读取接口。

## 已落地的业务数据

- shell：structuredContent 包含 type=ran、原始合并输出 output 和 ended。模型 content 中的提醒和转义不会污染原始 output。
- read 文本：type=read、path、text、from、to、可确定的 total、下一页 next、被截断的行号 clipped。text 是当前页的行内容，按换行连接；不声称是文件原始字节。
- write/edit：type=changed、path、created、additions、deletions。
- glob：type=glob、files、next、denied。files 为当前页的实际路径，next 为下一页的 1-based offset 或 null。
- grep：type=grep、files（path 与 lines）、next、skippedBinary、denied。每行含 number、text、matched、clipped，保留上下文行但与匹配行明确区分；next 按匹配数计算，不把上下文行计入分页。skippedBinary 是本次搜索实际跳过的二进制文件数，不保证覆盖未扫描文件。
- 搜索 UI 直接读取上述结构化结果，不再从模型文本解析路径或行号；展示文本只用于模型与说明。
- MCP：保留服务器 structuredContent；_meta 映射到 metadata，与转换后的模型 content 分开。
- 其他工具通过相同结果封装返回 content，按实际能力逐步增加业务数据；不制造空的结构化“成功”结果。

present_file 通过 content.resource_link 交付原始文件，metadata.title 提供调用标题；execute 抛错仍保留已成功调用的交付链接。详见 [PRODUCT_TOOLS.md](PRODUCT_TOOLS.md)。完整 MCP 资源内容支持、统一嵌套调用 UI 也单独推进，不借字段改造重写执行内核。

## 验收

1. 工作区调用方与 TypeScript 协议使用新字段，旧字符串工具返回值调用方全部迁移。
2. read/shell 数据可直接计算，不必解析 content 中的提示文字。
3. metadata 随调用结果保留，默认不进入模型上下文。
4. execute 重放恢复完整结果，不重新执行已结算的调用。
5. 图片只有显式选择才进入 execute 的模型输出，失败保留已输出图片，拒绝未返回的资源引用。
6. 保留权限检查、执行顺序、instructions/plan 和 turn 停止机制。
