# 工具结果契约试验

日期：2026-11-16。状态：设计原型，尚未接入或替换运行协议。

## 结论

用 read、grep、shell、browser 截图、present_file 五种场景检验后，建议原生工具采用 `ToolResult<T> { value: T, attachments: Attachment[] }`。execute 的 `tools.*` 返回完整结果，而非只返回 value。此处的 attachments 是工具产出的资源，不意味着向会话投递新的用户输入。

此方案替代之前讨论中的通用 value + content 初稿：原生工具不保存一份面向模型的说明文字再保存一份相同业务数据；格式化交给各消费方。暂不增加 show()、资源注册中心或通用 JSON 路径引用。

## 概念原型

```ts
type ToolResult<T> = {
  value: T;
  attachments: Attachment[];
};

type Attachment =
  | { kind: "image"; file: FileRef; width: number; height: number }
  | { kind: "file"; file: FileRef };
```

FileRef 继续使用已有内容寻址引用，示例所需名称、媒体类型和大小按真实公共类型确定，不创建同义的 ArtifactId。图片尺寸与文件元数据只在 attachment 中持有，不再复制进 value。

原生工具各有静态返回类型；工具注册边界再做 JSON 类型擦除。定义返回 schema 用于脚本声明和协议类型，不依靠动态 JSON 猜类型。不要把所有工具硬塞成一个不断增长的巨型结果枚举。

`value` 允许字符串或对象，不要求所有工具都有 status/message/data 三层包装。attachments 为空时脚本仍能直接访问空数组；序列化是否省略是传输细节，不改变脚本契约。

## 五个场景

### read：保留原始文本与可继续的位置

```json
{
  "value": {
    "kind": "text",
    "path": "src/main.rs",
    "lines": [{ "number": 1, "text": "fn main() {}", "clipped": false }],
    "next": null
  },
  "attachments": []
}
```

使用逐行记录可以准确标记长行截断。next 表示继续位置，clipped 表示本行返回的内容并不完整，两者可以同时存在。空文件返回空 lines，而不是伪造第 0 行。不要承诺总行数，除非实际读到了文件结尾。

代价是逐行 JSON 较大；这是原型中需要进一步比较的设计点。迁移前可对照“text + 起始行号 + 被截断行号集合”的体积与处理便利性，不现在就把示例当成最终格式。目录、图片和二进制使用 read 的其他 tagged 返回分支，不虚构统一行号。

### grep：返回匹配而非展示字符串

```json
{
  "value": {
    "matches": [{ "path": "src/main.rs", "line": 1, "text": "fn main() {}" }],
    "next": null
  },
  "attachments": []
}
```

脚本通过 `result.value.matches` 处理。真实格式仍需覆盖上下文行、截断和跳过的二进制文件；本例只证明基本匹配无需解析 `path:line:text`。offset 分页不保证文件变化时结果稳定，不能借这次结果改造默默承诺快照搜索。

### shell：退出码属于命令结果

```json
{
  "value": {
    "output": "one test failed",
    "ended": { "type": "exited", "code": 1 }
  },
  "attachments": []
}
```

保留现有 stdout/stderr 合并的行为，不伪造两个原实现没有独立捕获的输出流。非零退出不等于工具无法执行。超时、信号、输出超限等沿用实际 CommandEnd 语义。

目前实现对部分异常结束不保留输出；结果格式不能虚构丢失的数据。是否改善这些执行行为另立改动，不能与字段迁移混为一谈。原始输出不嵌入 system-reminder 标记，模型适配层在组装上下文时执行现有转义和边界标记。

### browser 截图：图像只保存一份

```json
{
  "value": { "page": "page-id" },
  "attachments": [{
    "kind": "image",
    "file": { "ref": "sha256:…", "name": "page.png", "media_type": "image/png", "size": 12345 },
    "width": 1280,
    "height": 800
  }]
}
```

脚本可以读取 attachments 中的引用和尺寸；模型适配层转换为图像输入；界面读取同一资源。模型侧若需缩放，应生成派生数据，不覆盖原件。资源引用按当前 thread 解析，不能跨 thread 仅凭哈希读取。

### present_file：展示不能依赖模型再写一次路径

```json
{
  "value": { "title": "验证报告" },
  "attachments": [{
    "kind": "file",
    "file": { "ref": "sha256:…", "name": "report.pdf", "media_type": "application/pdf", "size": 12345 }
  }]
}
```

工具成功记录是成果卡片的依据；文件元数据不同时复制到 value。图片成果也可作为 file 交付，UI 根据媒体类型预览原件，不意味着自动给模型发送原图。image 表示需要作为图像内容消费，file 表示文件资源；两者不是按扩展名机械区分。

## 三个消费方

- execute：每个 `tools.*` 返回完整 ToolResult，脚本选择使用 value 或 attachments。脚本 return 成为 execute 自身的 value。
- 模型：按工具结果类型渲染简洁说明，对图像生成适配后的输入，对普通成果文件提供引用与元数据。不能把 ZIP 等不可读内容塞进模型。
- 界面：调用记录展示状态，value 支撑具体工具视图，attachments 支撑预览/下载。展示不是靠分析模型说明文字。

内建工具应以一份业务数据为准。MCP 等外部工具可能同时给结构化值和文本/多媒体；边界适配必须保留这些真实内容，不因本契约丢弃提供方文本。首版以专用 MCP 返回类型保留其内容，脚本直接读明确的 structured 字段，不把外部协议兼容字段扩散到每一个内建工具。具体 resource link/embedded resource 映射必须在迁移 MCP 前补覆盖，不能只测原生五例就认为通用适配完成。

## execute 的保留、输出和失败

子调用仍各自记录。execute 聚合模型需要的图片/文件引用，按调用开始顺序稳定排列，而非完成顺序；界面仍归属到原子调用，不能再显示一份相同的成果卡片。重复引用不应重复存储字节，是否重复显示由原子调用的语义决定，不全局按哈希删除记录。

脚本只返回简短摘要也不能使已交付文件消失；脚本后续抛错时，已完成子调用的记录和成果仍保留。自动聚合必须有数量/大小预算；超限必须说明哪些内容未内联，并保留历史可访问引用，不能静默丢失。原型未验证这些运行行为。

不新增原生 fetch、任意文件系统、计时器能力，不改变现有恢复机制。每次子调用保留当前权限检查；explorer/general_purpose 的限制在调用路径落实。question/delegate 仍不进入 execute。

## 错误与内部机制

执行接口使用 `Result<ToolResult<T>, ToolError>`；参数错误、权限拒绝、目标缺失、执行失败要有稳定 code 与说明，不让脚本靠匹配错误文字判断。错误的结构化细节按实际需要增加，不先设计通用错误平台。

停止/中断继续由调用生命周期表达，不只当成普通可捕获错误：用户停止后，即使脚本尝试 catch，也不能启动新调用。结果未知不等于外部操作没有发生。不要添加一个让模型自行决定重放副作用的 retryable 标志。

Told 当前的 instructions 与 plan 不放进公开 ToolResult：它们分别是上下文发现与执行计划，内部执行封装仍可以一起携带以保持原子提交及恢复语义。不要求为每个字段单独创建服务或消息队列。

## 原型验证与限制

一次性 Bun 原型验证了：五类结果的脚本读取、JSON 往返、资源元数据单点保存、并行完成后的稳定输出顺序、分页与截断区分。脚本使用假资源引用，无实际 I/O，已运行通过。

它没有验证 Rust trait 迁移、模型效果、真实截图/文件传输、崩溃恢复、前端卡片、MCP 兼容、输出预算或运行性能。生产代码和协议没有改动。

建议下一步先做 read/grep 的具体返回类型及直接调用/execute 双路径对照，然后补截图。通过后再接 present_file；不一次性改完所有工具。
