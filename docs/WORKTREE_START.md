# 新任务工作目录：首个实现切片

2026-11-17。对应 ROADMAP R1a；已完成 worktree 查询、创建 RPC 和权限回归；目录准备与 Thread 启动的可靠衔接尚未实现。

## 复用现状

siphon-git::Git 已实现 repository、worktrees 和 create_worktree，Agent 的 git 工具已使用这些实现。客户端协议原先只提供仓库、diff、版本和提交等只读方法，本切片补齐 worktree 查询和创建。新 Thread 已支持在创建时指定 Config.workspace，但目录准备和首条消息目前没有统一的失败恢复流程。

不新增 Workspace 管理层、worktree 池、自动合并或运行中 Thread 迁移。本切片只提供客户端创建任务所需的 Git 原语，不把创建目录包装成启动任务。

## 客户端协议

- git.worktrees：参数 path（仓库内的目录），返回 worktrees；沿用 Git 的解析与真实注册状态，要求 files.read。
- git.create-worktree：参数 path（源仓库内的目录）、destination（绝对路径）、branch（新分支）、base（明确的基准 revision），返回 worktree（实际路径、新分支、实际 base_commit）；要求 git.operate。
- 复用 Git::create_worktree 的所有检查与 Git 自身行为；不复制实现，不自动清理失败残留，不新增 force 或 switch。
- 通过现有 Client.call 调用，不自动重试写操作。响应丢失时先查询注册 worktree 并核查现场；本切片不承诺幂等创建或断电恢复。

## 权限

当前 Scope 没有 Git 写操作权限。新增 GitOperate（git.operate），不能让 files.read 执行有写入副作用的 Git 操作，也不把 Git 操作挂到语义不符的终端权限上。

经用户单独确认，core 仅修改 core/siphon/src/protocol.rs 的 Scope 枚举。应用的新客户端完整权限清单加入新 scope；既有 session 的 scopes 不回填、不扩大，须由后续明确授权获得新权限。扩展声明与权限回归测试、节点类型生成和客户端协议更新均在 core 外。

查询沿用 person_machine 的只读视图。创建操作校验 git.operate 后，通过 Volume.commands().open 使用主机写入能力，保留 Host.hidden 保护；不改变 Agent 工具的执行权限。Git 可以执行仓库配置的 hooks/filters，因此 git.operate 是可信客户端的执行权限，不是仅能修改某个目录的文件权限。

## 协议归属

暂不拆独立 protocol 包。Siphon 持有基础通信和鉴权机制，各扩展持有自己的请求和结果类型，Node 装配并生成 TypeScript。当前 Scope 仍由 core 枚举管理，不在本切片引入动态权限注册。

## 本切片验收

1. 真实临时仓库：查询现有 worktree，并从明确 base 创建新目录，返回真实分支与 commit。
2. 源目录未提交修改和未跟踪文件不被复制，新目录内容来自 base。
3. 只读客户端可以查询，创建请求被拒绝且没有创建目录或分支。
4. 重复创建/占用目录失败不删除既有文件；已有 Git 实现测试继续通过。
5. 生成的 TypeScript 与 Rust 协议一致，类型检查和依赖分层检查通过。

## 后续切片

本切片通过后，再设计目录准备与 Thread 创建/首条消息的可靠衔接：输入保留、状态可查询、重复提交、节点重启和部分失败；仍优先复用 Siphon 公共原语，任何新增 core 接口另行说明并确认。未解决这些条件前，不上线新建任务 worktree 选项，不宣称 R1a 完成。

## 验证记录

Git 测试 10 项通过；节点访问权限与协议生成测试 3 项通过。Clippy（siphon-git、octoii 全目标）、TypeScript 类型检查、Rust 依赖边界检查通过。重启测试验证旧 session 的权限保持原值。

## 第二切片：首条消息失败重试

已修复新建页面在 Thread 创建成功、附件上传或消息投递失败后，重试又创建空 Thread 的问题。页面中的 NewThread 保留按创建配置区分的 Thread；附件全部保存后才投递首条消息，成功后才导航和清理已提交草稿。上传顺序执行，失败返回时不会还有同一批上传在后台继续。并发发送共享同一次提交，修改草稿后的显式重试使用最新内容；不同目录或模型配置不混用 Thread。

复用 Client.create / deliver 已有的幂等键和断线重试机制，不改 core，不新增启动 RPC。客户端方法内部的连接重试仍使用原键；这次页面层仅处理这些调用明确失败后的用户重试。

边界：保留状态仅在同一新建页面实例内有效。离开页面、刷新、关闭客户端后的恢复尚未实现；已经失败的空 Thread 不自动删除。worktree 写操作仍不自动重试，也尚未接入新建界面。

验证：新增 5 项测试覆盖上传失败、投递拒绝、重复发送、目录切换和创建拒绝；Web 全套 215 项测试通过，类型检查和 lint 通过。本轮未做浏览器交互验收。

后续顺序：

1. 持久保留新任务输入、附件和创建/投递幂等键，并按节点隔离；明确刷新后的恢复入口，避免自动启动未确认任务。
2. 接入目录准备状态。worktree 创建结果不确定时先核查现场，不盲目重建、不自动删除；目录确定后才创建对应 Thread。
3. 衔接首条消息，验收节点重启、响应丢失、重复提交与部分失败，然后才开放新建 worktree 界面。
