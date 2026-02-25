# TimeFiles 核心 ADR 清单（v0.1）

- 版本：v0.1
- 状态：Accepted（除特别标注）
- 日期：2026-02-25
- 作用范围：MVP 到 v0.2

## ADR-001 计时数据采用事件溯源（Event Sourcing）

- 决策：所有计时行为写入 `time_events` 事件表，统计值通过重放计算，不直接把“最终时长”作为唯一真相。
- 原因：支持修复、重算、审计和未来算法迭代。
- 结果：需要事件顺序和幂等处理；统计层复杂度上升，但可维护性更高。

## ADR-002 单活动上下文（Single Active Context）

- 决策：同一时刻只允许一个 `running` 任务。
- 原因：避免并发计时污染数据口径，优先保证统计可信。
- 结果：并行任务需求后置，未来若支持需引入“上下文槽位”模型。

## ADR-003 子任务插入语义固定

- 决策：运行中插入子任务时，父任务自动 `pause`；子任务 `stop` 后父任务自动 `resume`。
- 原因：把“任务切换”显式化，避免父子时间重叠。
- 结果：任务树路径清晰，`exclusive_time` 计算稳定。

## ADR-004 同时输出 Inclusive/Exclusive 两种口径

- 决策：每个任务节点必须同时展示 `inclusive_time` 与 `exclusive_time`。
- 原因：仅看总耗时无法定位“节点本体瓶颈”。
- 结果：统计表与 API 统一包含这两列。

## ADR-005 数据库选型 SQLite（rusqlite）+ 迁移版本控制

- 决策：本地存储使用 SQLite；Rust 绑定使用 `rusqlite`（默认启用 `bundled`，通过 `libsqlite3-sys` 编译并静态链接 SQLite）；采用 schema version + migration 脚本。
- 原因：TimeFiles 的核心查询是关系型（任务树、标签、时间区间聚合、事件重放），SQL 表达力和索引能力更匹配；相比自定义结构或纯 KV，MVP 可显著降低实现复杂度与统计出错风险；`bundled` 可减少系统 SQLite 版本差异造成的构建与运行问题（含移动端目标）。
- 结果：每次 schema 变更必须附 migration，不允许手工破坏式改表；这不是“pure Rust”存储栈（SQLite 内核是 C），以数据正确性和维护成本优先，接受该权衡。

## ADR-006 任务树使用邻接表（Adjacency List）

- 决策：`tasks` 使用 `parent_id` 建模，不在 MVP 引入 closure table/materialized path。
- 原因：MVP 写入简单，满足当前层级与移动操作需求。
- 结果：深层统计依赖递归查询或应用层 DFS。

## ADR-007 计时事件最小集合（MVP）

- 决策：MVP 固定事件类型：`start`、`pause`、`resume`、`stop`、`reparent`、`tag_add`、`tag_remove`。
- 原因：先满足核心追踪和统计，不过早扩展事件域。
- 结果：后续新增事件必须版本化并保持向后兼容。

## ADR-008 命令式后端 + IPC 边界清晰（Tauri v2）

- 决策：所有状态修改走 Rust `#[tauri::command]`，前端不直接操作底层存储。
- 原因：保持业务规则单点收敛，降低前后端状态漂移。
- 结果：每个命令必须在 `generate_handler![]` 注册并返回 `Result<T, E>`。

## ADR-009 能力权限最小化（Tauri Capabilities）

- 决策：`src-tauri/capabilities/default.json` 只开启 MVP 所需权限（先 `core:default`，按需增量）。
- 原因：Tauri v2 默认拒绝策略下，最小授权最安全。
- 结果：新增插件前必须先补 capability，再开放功能。

## ADR-010 自适应休息先规则后 AI

- 决策：MVP 用规则引擎给出 `0/3/8/15` 分钟建议，AI 仅作为后续增强。
- 原因：先验证可用性与行为闭环，降低实现和隐私风险。
- 结果：规则参数可配置并记录命中原因，便于后续模型替换。

## ADR-011（Proposed）并行任务支持策略

- 状态：Proposed
- 决策候选：v0.3 后引入“多上下文槽位”，默认仍单槽位。
- 待定原因：会显著改变统计语义与 UI 复杂度。

## ADR-012 存储实现可替换，但不在 MVP 过度抽象

- 决策：仅在仓储层保留最小 `Storage` 抽象边界，不为“可替换数据库”提前引入复杂架构。
- 原因：当前阶段追求可用与正确，过早为 KV/自定义存储做通用化会拖慢迭代。
- 结果：v0.2+ 若有明确需求，再评估 `redb/sled` 或自定义事件文件存储。

## 不可违反约束（Guardrails）

- 不允许出现两个并发 `running` 任务。
- 不允许任务树循环引用。
- 不允许跳过事件日志直接改统计值。
- 不允许无 migration 的 schema 变更。
- 不允许命令未注册即前端调用。
