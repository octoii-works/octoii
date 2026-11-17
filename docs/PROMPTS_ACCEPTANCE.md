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

Thread/Delegate 验收继续进行，尚不作为已通过项目。
