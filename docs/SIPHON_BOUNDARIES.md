# Siphon 闭源准备：上层依赖边界清单

2026-11-17；代码基线 `6b0b4f6`。配合 [分发方案](SIPHON_DISTRIBUTION.md) 使用。
本轮只读盘点并写文档，不修改 core、Node、扩展、测试、Cargo 或 AGENTS.md。
所有替代接口均是职责建议，不是已存在接口或已确认的签名。

## 盘点口径

检查 apps/node、extensions 的 Cargo 依赖与 Rust 生产/测试代码，并向下核对 Siphon 公开接口。
区分直接访问、通过 Siphon 转导出的底层类型、以及测试底层构造。
浏览器 DOM snapshot、工具结果快照等同名概念不是 Beak 存储访问，不计入清单。
packages/client、apps/web、apps/desktop 使用客户端协议；本次未发现它们直接使用 Rust Beak/runtime。

## 生产调用边界

| 编号 / 位置（仓库相对路径） | 当前用途与依赖 | 目标与缺口 | 批次 / 验收 |
| --- | --- | --- | --- |
| B01 `apps/node/src/serve.rs` 的 building / serve | 构造 Beak、runtime Options，传入 Siphon builder；停止直接 runtime.close | Node 选择目录、机器与产品配置；Siphon 管内部创建和关闭。需宿主配置与生命周期入口，不能只转导出 Options | S2：启动失败、目录互斥、停止和重开；明确关闭后后台任务/资源归属 |
| B02 `apps/node/src/files.rs::sent` | runtime.snapshot + Changes + files::keep_given + commit | HTTP 鉴权/限额留 Node，有类型附件接收与原子保存归 Siphon。现有 keep_file 不拟合图片、keep_image 专用于图片，不能直接用其中一个替换 keep_given | S1：普通文件、有效/无效图片、拟合后的引用、重复上传、Thread 删除竞态、权限和错误响应 |
| B03 `apps/node/src/files.rs::kept` | runtime Snapshot 交给 threads::files::read | 用受控 Thread 文件读取接口；HTTP 缓存、缩略图响应仍在 Node。当前 Volume 没有等价公开读取方法 | S1：原始内容、缺失文件、无权限、缩略图与 immutable 缓存行为不变 |
| B04 `apps/node/src/access.rs` | 配对/会话直接用 Runtime、Changes、Beak Snapshot/Range/layout/Error；ID、时间、过期、枚举和提交 | 配对策略、secret 摘要、scope、cookie/bearer 属 Node；需要受控持久化契约。不能把 Access 全搬入内核，也不能包装裸 runtime.commit | S3：兑换只有一个赢家、到期和撤销生效、重启持久、错误不泄密；决定接口和既有记录迁移后才改 |
| B05 `apps/node/src/pairing.rs::add` 及 serve.rs 装配 | Access::new(siphon.runtime.clone()) | 随 B04 迁移受控存储句柄，配对服务与鉴权流程保持 | S3：迁移全部构造入口，旧会话不能隐式扩大权限 |
| B06 `apps/node/src/thread_start.rs::accepted` | Volume.snapshot → settings::environment，检查 Git 扩展禁用状态 | 公开有效设置/扩展可用性查询；不让 Node 解释存储。保留方法实际注册及 scope 检查 | S1：禁用、未装配、缺权限分别拒绝，不接收半成品 Thread |
| B07 `extensions/siphon-mcp/src/servers.rs` | volume::Snapshot/Result；catalog/configured/project/listed 取得快照调用 trust::decided | 配置解析、摘要、环境变量和列表属扩展；信任决定由公开查询提供。需保留一次查询的一致性，不能简单逐条读取改变语义 | S1：未信任、拒绝、同意及配置摘要变化；扩展不再接收原始 Snapshot |
| B08 `extensions/siphon-browser/src/browser.rs`、`extensions/siphon-terminals/src/lib.rs` | 错误变体 Beak 使用 siphon::volume::Error，无直接底层导入 | Siphon 定义自己的公开错误边界；扩展不依赖转导出的 Beak 错误结构，保留必要分类及原因 | S4：错误转换、显示与取消路径保持，不全变成无区别字符串 |

现有例外不是永久许可。B01 初始化受现有构建 API 限制；B02/B03/B04 缺少完整的受控公开能力，
目前仍有直连。新增功能不得仿照这些入口扩大依赖。

multipart 接收主体已使用 Siphon::volume 和 Git 扩展公开 accept；Git 扩展通过 Volume 启动门与 Commands
执行，不直接操作 Beak/runtime。这些是保留方向，不重新设计。
扩展生产源码未发现直接 beak::/siphon_runtime:: 或 .runtime 使用；B07/B08 表明无直接导入不等于隔离完成。

## Siphon 公共接口仍暴露的实现

这里只读 core，列出设计审查入口，不是本次修改范围。

| 入口 | 暴露内容 | 收敛原则 |
| --- | --- | --- |
| `core/siphon/src/lib.rs` | builder 要求 Beak；Setup.options 是 runtime Options；Siphon.runtime 公开 | 提供产品需要的打开、配置、关闭职责，内部句柄不作为上层通行证 |
| 同上 / threads.rs | Threads 的 ThreadProgram 泛型把具体执行程序带进公开类型 | 先盘点实际 Thread 操作，再隐藏程序类型；不新建 Thread 系统 |
| `core/siphon/src/volume.rs` | 转导出 Beak Error/Result/Snapshot，公开 snapshot | 按文件、设置、信任和产品记录需求逐步替代，不另建通用读写框架 |
| `core/siphon/src/extension.rs` | Registrar/Caller 与闭包、异步 handler 构成静态扩展契约，部分结果带底层错误 | 保留源码级调用习惯，之后用分发试验验证跨边界能力；现有 trait 不等于二进制接口 |
| `core/siphon/src/testing.rs` | Beak 转导出、Calling/commands 等夹具依赖底层构造 | 提供公开测试宿主；真实内核私有测试留实现侧，公开扩展测试不依赖私有源码 |

收紧 API 后同步迁移调用方，不保留仅换名称的兼容层。公开签名的完整传递类型审计在 S4 完成；
本表是已确认的关键入口，不声称已完成全部泛型与返回类型的 ABI 审计。

## 测试与 Cargo 边界

| 类别 | 已确认位置 | 迁移方向 |
| --- | --- | --- |
| Node Access/关闭夹具 | apps/node/tests/{access,tunnel,files,pairing,thread_start}.rs | 跟随 B01/B04 用公开宿主和 Access 入口，保留持久化及并发断言 |
| 设置与 ID 夹具 | apps/node/tests/told.rs、extensions/siphon-base/tests/context.rs | ID 可复用 Volume.new_id；设置使用公开入口或真实 RPC，不再传 runtime |
| 扩展关闭夹具 | extensions/siphon-{mcp,terminals,git,browser}/tests 中 runtime.close | 随 S2 迁移；关闭与释放语义支持重开测试，不用 sleep 重试锁 |
| 浏览器附件断言 | extensions/siphon-browser/tests/pick.rs | 用 B03 的公开读取验证截图，保持不存在文件断言 |
| 底层测试构造 | extensions/**/tests 的 testing::Beak、Calling、commands 和 builder | S4 统一迁移公开测试宿主，不为各扩展复制私有夹具 |
| Node Cargo | apps/node/Cargo.toml 的 beak、siphon-runtime | B01–B05 迁移后移除直接依赖，不先删 manifest 再用别名导入 |
| Workspace Cargo | 根 Cargo.toml、tools/check-rust-layers.py | 暂保留内核所需 workspace 依赖；后续收紧 Node 例外，检查正常/开发依赖；Cargo 检查不能代替源码/API 审查 |

## S3 需要先决定的最小契约

Access 需要的不是普通 get/set：兑换链接须原子验证并消费链接、创建 session；失败或并发输家
不能留下 session。枚举、过期、撤销和重启持久也须有对应语义。

先基于这一个真实调用方设计受控记录能力，限定所属范围与原子条件；能否复用已有原语需进一步设计。
本轮不指定新引擎、不迁到单独数据库、不承诺通用事务接口，不把 cookie、配对时长等产品规则搬进 core。
既有 octoii.session / octoii.link 记录纳入后续迁移验收。

## 推进与验收门槛

1. S1 的接口、具体 core 范围和验收见 [SIPHON_S1.md](SIPHON_S1.md)，原范围已确认；当前优先拆分本地 workspace，S1 暂缓。
2. S2/S3 各成小批次，调用迁移与相关测试同批提交；可直接复用现有公开接口的部分不必等完整分发设计。
3. S4 清除生产和公开测试中的实现依赖；核对 Cargo、源码导入及公开签名，不用 pub use 隐藏依赖。
4. S5 隔离调用方只拿公开源码与产物，跑真实工具回调、取消、恢复、错误与资源释放。
5. S6/S7 才移动 Octoii 实现、拆构建和迁仓库；正式操作另行确定，不动 archive 分支。

不暂停 R1 主线故障验收；触及相关边界时独立提交，未触及的抽象不提前重写。
本轮验收为源码/文档交叉核对、相对链接和 diff 检查，不把文档更新当成边界已收紧。
