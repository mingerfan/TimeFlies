# TimeFlies 通知系统三阶段改造记录（v0.1）

- 日期：2026-03-05
- 状态：Implemented
- 范围：通知统一（展示层 -> 数据层 -> 错误语义层）

## 1. 改造目标

将“休息建议、命令反馈、接口错误”等原本分散的提示路径统一成同一通知系统，形成以下演进：

1. 第一阶段：统一前端展示入口。
2. 第二阶段：统一后端通知数据模型。
3. 第三阶段：统一错误语义与分级能力。

---

## 2. 第一阶段（展示层统一）

## 2.1 目标

把页面内各处 `errorMessage`、命令反馈、休息弹层统一到一个全局通知中心。

## 2.2 主要改动

1. 新增通知 store：`src/lib/notifications.ts`
2. 新增通知组件：`src/lib/components/NotificationHub.svelte`
3. `src/routes/+layout.svelte` 挂载 `NotificationHub`，并将休息建议转成统一通知。
4. 页面级错误条移除，改为调用 `notifyError(...)`。
5. 命令反馈改为调用 `notifyCommandResult(...)`。

## 2.3 结果

前端仅保留一条通知通道，休息建议与错误提示在同一容器展示。

---

## 3. 第二阶段（数据层统一）

## 3.1 目标

将通知从“仅 UI 状态”升级为“后端可持久化数据”，统一通知来源，支持后续扩展。

## 3.2 主要改动

1. SQLite migration v3 新增 `notifications` 表（`src-tauri/src/infra/sqlite.rs`）。
2. 历史 `rest_suggestions` 数据回填到 `notifications`。
3. 新增后端通知实体：`NotificationRecord`（`src-tauri/src/domain/mod.rs`）。
4. `OverviewResponse` 增加 `notifications` 字段。
5. `service.rs` 在“创建休息建议/响应休息建议”时同步维护 `notifications` 状态。
6. 前端 `src/lib/api.ts` 增加 `NotificationRecord` 类型并对接。
7. `+layout.svelte` 优先消费 `overview.notifications`，同时保留 `overview.rest_suggestion` 兼容兜底。

## 3.3 结果

休息建议通知已具备统一持久化入口，通知来源从“前端拼装”升级为“后端统一提供”。

---

## 4. 第三阶段（错误语义统一）

## 4.1 目标

将后端 `Result<T, String>` 变为结构化错误，前端根据错误码自动分级通知。

## 4.2 主要改动

1. 新增 `AppError { code, message, detail }` 与 `AppResult<T>`：
   - `src-tauri/src/infra/sqlite.rs`
   - `src-tauri/src/infra/mod.rs`
2. 命令层统一数据库锁错误返回 `internal`：
   - `src-tauri/src/commands/mod.rs`
3. 业务层错误分类落地：
   - `validation`：参数/输入非法
   - `conflict`：状态冲突
   - `not_found`：资源不存在
   - `internal`：数据库或内部异常
   - 文件：`src-tauri/src/app/service.rs`
4. 前端错误解析升级：
   - 新增 `extractErrorPayload(...)`（`src/lib/ui.ts`）
   - 通知中心按 `code` 分级（`src/lib/notifications.ts`）
5. 页面调用改为传递原始 error 对象，避免提前字符串化丢失错误码：
   - `src/routes/+page.svelte`
   - `src/routes/timer/+page.svelte`

## 4.3 分级策略

1. `validation/conflict/not_found` -> `warning`
2. 其他或未知错误 -> `error`

---

## 5. 最终通知流（当前）

1. 业务触发（任务切换/子任务结束）生成休息建议。
2. `rest_suggestions + notifications` 同步写入。
3. `get_overview` 返回待处理通知。
4. `+layout` 拉取后推入 NotificationHub。
5. 用户“接受/忽略”调用 `respond_rest_suggestion`，后端同步更新两张表状态。
6. 若接受且存在运行任务，后端自动将该任务 pause 并写事件。

---

## 6. 验证记录

1. `cargo check`：通过
2. `npm run check`：通过（svelte-check 0 errors / 0 warnings）
3. `cargo fmt`：当前存在一次写入失败记录（`service.rs` 拒绝访问，os error 5）

---

## 7. 兼容性与迁移策略

1. 仍保留 `rest_suggestion` 字段作为兼容兜底，避免前端一次性切断旧路径。
2. 新逻辑优先使用 `notifications`，后续可在确认稳定后移除 `rest_suggestion` 旧字段依赖。

---

## 8. 后续建议

1. 在 Tauri 命令边界补充结构化错误的契约测试（code/message/detail）。
2. 增加通知审计与历史查询接口（已处理通知分页）。
3. 在前端加入按错误码的本地化文案映射（避免直出后端 message）。
