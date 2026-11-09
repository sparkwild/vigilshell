# UI 组件与设计规范

## 组件选型

项目使用 shadcn/ui 的 Base UI 版本，配置为 base-nova／neutral，启用 CSS variables。配置入口是 [components.json](../components.json)。

| 场景 | 组件与技术 |
| --- | --- |
| 按钮、输入、菜单、弹窗、标签、卡片 | `src/renderer/components/ui/` 中的 shadcn/ui 公共组件 |
| 树与表格 | React Aria Components |
| 表单与校验 | React Hook Form、Zod；FormDialog／DialogForm |
| 确认操作 | ConfirmationDialog |
| 信息提示 | Tooltip、HoverCard、Alert、Empty、Spinner |
| 图表与指标 | shadcn Chart、Recharts、Meter、Progress |
| 面板布局 | react-resizable-panels |
| 自动布局动效 | AutoAnimate |
| 通用图标 | Lucide |
| 品牌图标 | 本地 SVG，统一由 AgentLogo 以 CSS mask 渲染 |
| 终端 | xterm.js |

业务功能优先组合公共组件和变体，不另建同用途控件，不复制焦点、遮罩、关闭或键盘处理。公共组件不包含业务状态。AgentLogo 位于 [McpClientPicker.tsx](../src/renderer/features/mcp/McpClientPicker.tsx)。

## 样式与设计令牌

全局入口为 [index.css](../src/renderer/styles/index.css)，颜色、排版与布局令牌位于 [tokens.css](../src/renderer/styles/tokens.css)。业务样式引用语义令牌，兼容深浅主题和系统辅助功能，不硬编码一套页面配色。

- 正文 14px、辅助文字 12px、控件文字 13px；终端默认 12px、1.4 倍行高。
- 字体使用本地 Fontsource Inter、JetBrains Mono、Noto Sans SC；代码与数值使用等宽字体。
- Badge 使用 radius-sm 圆角矩形；分段切换复用 Tabs 和 TabsIndicator。
- 普通交互使用公共 hover、focus、disabled 状态；透明背景上的按钮可使用 ghost-contrast。
- 图表使用统一 Chart 容器、尺寸和悬浮提示；无数据用占位，不以零值代替未知值。
- 动效使用公共组件默认行为，遵循减少动态效果设置。

## 窗口与布局

主窗口初始 1280×820，最小 960×640。macOS 左栏采用 sidebar vibrancy，减少透明度时使用实体背景。面板尺寸只在运行期保留。

工作区由左侧连接导航、中间终端与文件区、右侧工具区构成。连接列表独立滚动；智能体权限、智能体活动和设置入口固定在侧栏底部。权限和活动页面在中间工作区切换，保留终端挂载；关闭页面返回工作区，不关闭终端。

页面明确区分固定页头与内容滚动区，避免多层滚动。连接编辑弹窗默认高 560px，上限为视口高度减 80px。主工作区使用独立层叠上下文；菜单与浮层复用 Portal，固定表头不能覆盖导航预览或全局工具区。

## 交互与反馈

表单聚焦首个可编辑项，确认默认聚焦取消，长内容预览聚焦正文；中文输入法组合中的 Enter 不提交。图标按钮使用公共 IconButton，并提供可访问名称及 Tooltip。页面、弹窗和浮层的右上角关闭入口统一使用 CloseButton（32px 点击区域、20px 图标、2.5px 线宽），页面只设置位置，不单独覆盖尺寸或样式。

操作反馈归属对应组件：加载使用 Spinner／Skeleton，空内容使用 Empty，错误使用 Alert 或局部结果区。应用不使用 Toast，不为成功操作增加重复提示。系统通知由 Main 统一管理，页面只报告实际可见内容。

状态栏菜单使用“打开 VigilShell”“设置”“退出”文案；“设置”不带省略号，“退出”不附加应用名称。

关闭展示不等于取消任务；表单离开统一经过草稿守卫，失败保留用户输入。界面展示可理解的原因和处理建议，内部错误码与请求编号留在诊断层。

文件区共用目录树、路径栏、列表、右键菜单和任务展示。树选中项、路径和列表保持同步；右键目标独立于浏览位置，多选保持明确。长名称与路径截断显示，完整内容通过 Tooltip／HoverCard 提供。

Agent 操作复用普通终端和文件界面。连接信息与控制状态共用 TerminalFloatingNotice／TerminalNoticeOverlay，标签和控制浮窗中的 AgentLogo 统一使用 sm 尺寸（16px）；顶部浮窗显示操作者和解除入口；等待、失败及结束沿用同一展示结构。活动页使用摘要表格，不提供专用终端或回放界面。

## 国际化与启动

界面支持 system／zh-CN／en。消息源位于 `messages/`，Paraglide 生成代码不手改；业务错误在显示时翻译，用户输入、保存名称和远端输出保持原文。切换语言和主题保留当前输入及界面状态。

启动占位由 [HTML 入口](../src/renderer/index.html)与 [startup.css](../src/renderer/startup.css)提供，不依赖 React 或 Backend。透明背景上使用现有 S 形 Logo 遮罩，玻璃效果只作用于图案笔画，无方形底板。Logo 下方的启动文案和左侧加载圈同一行显示，工作台就绪后移除启动占位。减少透明度与动态效果时使用对应回退。

## 关于与更新

设置统一提供“关于”类别。上方居中展示应用图标、产品名称、包版本、许可标签、面向 Agent 的 SSH 终端简介和检查更新入口；检查按钮使用刷新图标。下方列表展示项目主页与问题反馈，页尾展示版权。自动检查设置和页内检查状态保持隐藏，相关实现保留；检查结果统一在弹窗展示。新版通过设置入口与关于类别的角标提示。

页面或应用菜单的手动检查打开同一更新弹窗，复用公共 Dialog 的遮罩、焦点、关闭和主题样式。弹窗采用公共默认宽度 384px，窄视口保留两侧至少 16px 的空间，高度不超过视口减 64px。弹窗展示检查过程、检查结果、纯文本发布说明和下载入口。更新弹窗的操作区不设分割线或独立背景，单按钮居中，多按钮沿用原有排列。获取版本失败统一显示“检查更新失败”，底部仅提供“确定”按钮关闭，不展示具体失败原因、重试入口或检查时间。长版本号和连续文本可换行；长说明仅在弹窗正文滚动，标题和操作保持可见。关闭弹窗不取消 Main 中的请求，重复检查复用同一请求。下载安装仍在浏览器与系统中完成。
