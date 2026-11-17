# 提示词分层真实任务验收

2026-11-17。被验收提交：`03d6ff7`（P1–P4 及卡片协议归属调整）。

从当前源码构建节点和 Web，在原隔离 Home 上更新测试节点，保留端口 64910 和既有浏览器配对。没有操作日常节点。界面操作使用内置浏览器；这不是 Electron 原生或 iPhone 真机验收。

临时项目：`/private/tmp/octoii-prompts-live/workspace`。主 Thread：`01a1e7fc-72ce-77fc-9234-fe4a0abf7f48`。模型为 ChatGPT / GPT-6.1 Sol，代码首轮 Medium，Default 权限。项目只有无依赖的计算器与 Node 测试，没有使用真实业务数据。

## 代码任务

- 初始缺陷：total 只累加 price，没有乘 quantity。原测试仅覆盖 quantity=1。
- 给模型的任务只描述需求与范围，不指定工具调用方式，不允许委派、安装依赖或提交。
- 模型读取项目，新增数量大于 1、数量为 0、空列表、多项输入测试；先运行测试复现缺陷，再修复。
- 模型报告 5/5 通过；独立执行 `node --test` 同样 5/5 通过。只修改 total.mjs 和 total.test.mjs。
- 界面完成后显示 `Worked for 43s`，工具过程默认折叠；最终回复简洁列出修改和验证。

## HTML 交付

- 模型生成 invoice-report.html，主动使用 browser.preview 自检，再调用 present_file；Default 模式出现正常审批，本轮批准临时报告交付。
- 独立点击内联报告按钮，显示合计 ¥23.00；计算为 5×3 + 2×4。浏览器常规点击接口无法定位双层 iframe 内按钮，改用 CDP 鼠标输入完成真实点击，没有改写页面内容。
- 读取实际样式：主标题 16px、正文 14px。桌面内容宽度/scrollWidth 均为 705px；390px 模拟视口下均为 330px，无横向溢出。这是桌面响应式检查，不是 iPhone 真机验收。
- 深色媒体条件下，报告与会话文字颜色均为 oklch(0.95 0.0024 264)，截图确认主题跟随；已恢复媒体与视口覆盖。
- 页面刷新后，原交付正常加载，按钮状态重置为待计算。工作区文件仍在，本轮没有额外验证删除源文件后的恢复。
- 截图保存在 /private/tmp/octoii-prompts-live/report-light.png 与 report-dark.png，已发送用户。

## Thread 与 Delegate

- 明确授权后台创建 explorer（只读审阅两个文件）与 general_purpose（仅新建 acceptance-note.md）；分别进入两条子会话，界面均确认 GPT-6.1 Sol / Max。
- Explorer：01a1e803-2b00-7dac-ae47-c2c6dc3e349c；General purpose：01a1e803-4d6b-7613-8283-cb55dd98e079。
- 两个子任务完成后消息自动送回父会话，父会话恢复执行；无需轮询。父会话先报告中间状态，第二个完成后读取文件并复跑测试，最终汇总。
- Thread search/read 在当前会话内完成，最终会话引用渲染为可点击链接。独立读取 acceptance-note.md，内容符合任务；独立 node --test 再次 5/5 通过。
- 完成后工具过程默认折叠，子任务结果卡片和父会话引用可见；截图 subtasks.png 已发送用户。

## 发现与修正

Files 提示词把文件呈现范围写得过宽。父会话完成检查后额外请求 present_file(acceptance-note.md)，触发不必要的审批。浏览器自动审批拒绝该交付，理由为本轮只明确授权创建和核查笔记、未授权资源交付。随后主动 Deny；模型没有重试，保留本地文件并准确说明。

收紧 Files Teller：standalone deliverables 使用 present_file；项目文件和内部笔记默认链接路径，只有明确要求作为文件接收时才呈现。创建/修改文件本身不等于交付请求。没有改变执行权限、core 或工具 schema。节点上下文 3 项测试通过，快照已更新。

修正后的节点再次在同一隔离 Home 启动。新建独立会话 01a1e807-3481-7e16-9bb2-94026dd3914c，要求新增三行项目内部笔记，没有显式提示模型避免 present_file。真实 GPT-6.1 Sol 完成后只链接 internal-check.md；工具历史为读取、搜索和写入，没有呈现调用或审批，未修改已有文件。这是单次实测结果，不代表模型行为的确定性保证。

本轮未发现需要修改 core 的问题。未 push；保留临时项目、会话和截图供复查，隔离节点保持运行。浏览器交付页面标记被自动审批拒绝，已跳过，不影响本地会话数据。
