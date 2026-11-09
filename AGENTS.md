# VigilShell

## 项目规则

- 开发、调试和原生验证复用现有 VigilShell Dev；正式数据不参与测试，保留已有未提交修改。
- 按架构维护模块职责和单一状态所有者，入口只负责组合。应用端口定义在 `vigilshell-application-contracts`，技术内部端口归所属组件。
- 通信合同采用 Rust DTO → JSON Schema → TypeScript/Ajv；生成文件不手改，Zod 仅用于表单。
- 代码、配置或命令改变时同步相关文档与检查约束；文档仅维护当前技术栈、UI 设计和系统架构；技术选型变化时直接更新，不记录验收、迁移、需求讨论或开发历史。不通过通用豁免绕过检查。

## 文档入口

按任务读取当前技术与设计：

- 技术与依赖：[技术栈](docs/TECH_STACK.md)。
- 模块分层与状态归属：[系统架构](docs/SYSTEM_ARCHITECTURE.md)。
- 组件库、样式与交互：[UI 组件与设计规范](docs/UI_DESIGN.md)。

## 项目技能

- shadcn/ui 组件与配置：[shadcn](.agents/skills/shadcn/SKILL.md)。
- React 编写、审阅与优化：[vercel-react-best-practices](.agents/skills/vercel-react-best-practices/SKILL.md)。
- React 组件 API 与 Context 组合：[vercel-composition-patterns](.agents/skills/vercel-composition-patterns/SKILL.md)。
