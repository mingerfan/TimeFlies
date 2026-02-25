# TimeFiles MVP 开发顺序（v0.1）

- 版本：v0.1
- 日期：2026-02-25
- 开发策略：先锁死不可逆决策，再做纵向可运行切片
- 说明：本文档不包含按周/按天排期，仅定义阶段目标与完成标准

## 0. 先决条件（启动前）

- 完成 ADR 对齐：`docs/ADR-Core-Decisions-v0.1.md`
- 初始化仓库骨架（Tauri v2 + Svelte 前端 + Rust 后端）
- 明确开发环境：Rust stable、Node LTS、pnpm/npm 二选一

完成标准：

- 能运行空白窗口（`tauri dev`）
- 前后端可通过 `invoke` 完成一个 `ping` 命令

## 1. 阶段 A：可信计时底座（必须完成）

目标：先让数据正确，再谈分析与体验。

### 1.1 存储与迁移

- 建立 SQLite schema（`tasks`、`tags`、`task_tags`、`time_events`）
- 存储绑定：`rusqlite`（启用 `bundled`）
- 引入 migration 机制和 schema version
- 约束：索引、外键、唯一性、软删除字段

完成标准：

- 新库可自动初始化
- 升级可执行 migration 且不会丢历史事件

移动端注意：

- Android/iOS 构建时统一使用 `bundled`，避免依赖系统 SQLite 版本差异。
- 优先维持 SQL 语句与 schema 的跨平台一致性，平台差异收敛在编译与打包配置。

### 1.2 计时状态机（Rust）

- 实现命令：`create_task`、`start_task`、`pause_task`、`resume_task`、`stop_task`、`insert_subtask_and_start`、`add_tag_to_task`、`remove_tag_from_task`
- 规则：单活动上下文、子任务插入自动 pause/resume

完成标准：

- 所有命令通过单元测试
- 任意路径下都不会出现双 `running`

### 1.3 事件重放与统计计算

- 从 `time_events` 计算每个任务 `inclusive/exclusive`
- 支持按时间窗口（日/周）聚合
- 统计实现与存储分层，避免耦合 UI

完成标准：

- 给定固定事件集，多次重算结果一致
- 能输出任务级耗时排行榜

## 2. 阶段 B：端到端 MVP 切片（必须完成）

目标：可真实使用一天以上。

### 2.1 最小前端界面

- 任务树视图：创建、展开、选择
- 计时控制：开始/暂停/恢复/停止
- 运行中插入子任务入口
- 标签编辑入口

完成标准：

- 从 UI 可完成一次完整链路：创建主任务 -> 开始 -> 插入子任务 -> 停止子任务 -> 回父任务 -> 结束

### 2.2 统计看板（MVP）

- 当前日/周总览
- 任务 `inclusive/exclusive` 列表
- 按标签聚合耗时

完成标准：

- 用户可以识别“本周最耗时的 3 个任务或标签”

## 3. 阶段 C：自适应休息 + 稳定性（建议完成）

目标：形成差异化体验并提高可持续使用。

### 3.1 规则版休息建议

- 触发点：子任务结束、任务切换
- 输入：连续专注时长、切换频率、历史偏差
- 输出：`0/3/8/15` 分钟建议

完成标准：

- 建议不强制中断
- 每次建议可查看命中规则（可解释）

### 3.2 稳定性与恢复

- 异常退出恢复最近活动任务上下文
- 去重与幂等处理（重复点击/重复命令）
- 基础性能优化（索引、批量读写）

完成标准：

- 模拟异常退出后可恢复
- 周级统计刷新在目标时间内

## 4. 模块与目录建议（初版）

- `src-tauri/src/domain/`：任务、标签、事件模型
- `src-tauri/src/app/`：用例服务（命令编排）
- `src-tauri/src/infra/`：SQLite、migration、repo
- `src-tauri/src/commands/`：Tauri `#[tauri::command]`
- `src/lib/`：前端状态与 API 封装
- `src/routes/`：页面与组件

约束：

- 业务规则只放 Rust 后端
- 前端只做展示与交互编排

## 5. 测试顺序（务实版）

1. 状态机单元测试（最高优先级）
2. 事件重放统计测试
3. 命令层集成测试（含数据库）
4. 前端关键路径冒烟测试

最低测试集（MVP 必须）：

- 插入子任务自动 pause/resume
- 无并发 running
- inclusive/exclusive 计算正确
- 标签聚合正确

## 6. 阶段验收 Gate

- Gate-A：数据正确性 Gate
- Gate-B：可用性 Gate（可连续使用 1 天）
- Gate-C：稳定性 Gate（异常恢复 + 性能）

若未通过 Gate：

- 停止新增功能，先修正基础层

## 7. 明确不做（防止范围漂移）

- 不做 AI 自动标签
- 不做语音输入
- 不做系统行为分神检测
- 不做多设备同步

以上全部进入 v0.2+ backlog。
