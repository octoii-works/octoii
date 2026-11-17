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

HTML 交付和 Thread/Delegate 验收继续进行，尚不作为已通过项目。
