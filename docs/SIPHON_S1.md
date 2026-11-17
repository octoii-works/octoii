# S1：文件与只读查询的受控接口

2026-11-17。基于 7840c60；用户已确认本方案的 core 范围，尚未修改代码；随后决定优先拆分本地 workspace，S1 暂缓。
对应 [边界清单](SIPHON_BOUNDARIES.md) 的 B02/B03/B06/B07。

## 目标与接口草案

只复用已有存储与执行实现，将快照、Changes 和 commit 收回 Siphon 内部。
不改 HTTP/客户端协议，不新建存储、通用事务或查询层。

| 拟提供接口 | 行为 | 复用实现 |
| --- | --- | --- |
| Volume::keep_given(thread, bytes, media_type, name) → FileRef | 接收用户附件；图片按模型限制拟合，其余保持原字节；返回已提交引用 | file_snapshot、threads::files::keep_given、现有 Changes/commit |
| Volume::read_file(thread, reference) → Option<Bytes> | 在有效 Thread 中读取已保存附件；缺失或非法引用返回 None，Thread 不存在/移除中明确拒绝 | file_snapshot、threads::files::read |
| Volume::environment() → settings::Environment | 返回已有有效环境设置；Node 继续检查禁用列表、实际方法注册与会话权限 | settings::environment，读取一次内部快照 |
| Volume::project_trust(folder) → Option<trust::Decision> | 返回这个项目在一次读取时的不可变信任决定；没有或无有效记录则 None | 当前 trust annotation 和 Decision 反序列化 |
| trust::Decision::decided(digest) → Option<bool> | 摘要匹配返回同意/拒绝，摘要不同返回未决定 | 当前 trust::decided 的匹配规则 |

Decision 复用已有结构，公开类型但字段保持私有，只提供只读匹配操作；不暴露 Snapshot、路径、annotation 或写入口。
现有 trust::decided(snapshot, ...) 在 core 内复用同一读取/匹配逻辑，避免两套解析。
MCP 每次 catalog/call/listed 取得一次项目决定，configured/project 共用；不能在同一次 listed 的两处分别读取最新决定。
它是一个项目的数据值，不是通用 Snapshot 替身。配置文件的读取仍由 MCP 扩展负责，不宣称文件系统也具备存储快照语义。

新增 Volume 查询可使用既有 anyhow::Result 习惯；附件沿用 files::Refused，保留可分类错误。
本批不统一全仓错误体系，现有 Refused 中仍有 Beak 来源的事实留待 S4 处理；不能宣称 S1 已达成完整 ABI 隔离。

## 拟修改的 core 文件（仅以下 4 个）

| 文件 | 修改内容 | 为什么扩展/Node 单独修改不足 |
| --- | --- | --- |
| core/siphon/src/volume.rs | 新增上述 4 个受控方法；复用现有内部快照与附件处理，已有 keep_file/keep_image 保持原职责 | 上层不能自行提交 Changes；现有方法没有完整的有类型附件接收、附件读取、设置及单项目信任读取能力 |
| core/siphon/src/trust.rs | 公开字段私有的 Decision 和摘要匹配方法；抽取复用的内部读取；在本文件补信任回归测试 | 上层若只调用当前 decided 就必须持有 Beak Snapshot；信任存储格式应仍由 core 解释 |
| core/siphon/src/tests/files.rs | 新增受控接收/读取回归；迁移适合直接验证新接口的断言 | 需要直接验证核心原语，不能仅以 HTTP 成功推断提交及图片处理正确 |
| core/siphon/src/tests/settings.rs | 验证 Volume 查询与既有有效设置一致，包括默认值、变更及禁用配置 | 确保公开查询复用当前规则，没有另造有效设置解释 |

不修改 core 的 lib.rs、settings.rs、threads/files.rs、测试模块装配、Cargo、协议、runtime、Beak。
信任测试放 trust.rs 的测试模块，不新建 core 测试文件。
若发现附件删除竞态需要新增提交前置条件或修改其他核心文件，先报告证据和追加范围，不能自行扩大。

## 上层同步迁移

- apps/node/src/files.rs：POST 调 Volume.keep_given，GET 调 Volume.read_file；移除直接 Runtime/Changes/快照访问。
  鉴权、请求上限、缩略图与缓存留 Node；明确映射 NoSuchThread 为 404、附件格式/尺寸错误为 422、存储失败为 500。
  现有正常响应不变，删除竞态不会伪装成成功上传。
- apps/node/src/thread_start.rs：用 Volume.environment 替代 snapshot → settings::environment；其他接受/准备规则不变。
- extensions/siphon-mcp/src/servers.rs：用不可变 Decision 代替 Snapshot 参数；同步已有错误传播，不引入低层存储访问。
- apps/node/tests/files.rs、tests/thread_start.rs：覆盖实际 HTTP 错误映射、附件、禁用/未装配和权限。
- extensions/siphon-mcp/tests/mcp.rs、tests/extension.rs：覆盖信任变化、摘要变化及一次查询复用同一决定。
- extensions/siphon-browser/tests/pick.rs：可同步以公开 read_file 检查截图，消除该处 runtime.snapshot。

Node 的 Access 和生命周期依赖留 S2/S3，因此本批不删除其 Cargo 中的 Beak/runtime 依赖。
不移除全仓 snapshot 导出，不迁移无关测试，不调整 UI。

## 拆分提交与验收

S1a：文件接收/读取原语及 Node、浏览器测试调用迁移。
验证普通文件原字节与媒体类型/文件名、图片拟合后引用、无效图片不落盘、大小限制、重复接收、
缺失 Thread/文件/非法引用、移除中拒绝、重开后读取、HTTP 权限与缩略图/缓存行为。
并发删除测试必须证明不复活 Thread、不返回虚假的保存成功；若现有提交语义不足，先停在该缺口请求追加范围。

S1b：环境与信任查询以及 Node/MCP 迁移。
验证默认和修改后设置、Git 禁用/未注册仍拒绝；信任无决定/同意/拒绝/摘要改变均正确；
旧 Decision 在后续信任变更后仍保持原读取结果，新读取才看到新决定，不能产生半新半旧的列表结果。

两批分别运行相关 Siphon、Node 和扩展测试、Clippy 与格式检查；最后运行依赖分层检查。
只对真实语义和权限边界补测试，不为每个转发方法做重复镜像测试。

## 待确认

上述 4 个 core 文件及职责已获确认；待本地 workspace 拆分完成后再按迁移后的路径核对实施范围。
本次不涉及 S2 生命周期、S3 Access 存储、S4 全量类型清理或二进制分发。
