# iOS 聊天模式改造计划（按 Codex Desktop 的真实行为）

> 状态：调研/设计稿，尚未实施。本文件把“要做什么、由哪个端做、为什么这样做”说清楚，供产品和工程共同审阅。
>
> 范围：在 KN iOS 中新增与现有终端并存的 **Codex 聊天模式**。目标是复刻 Codex Desktop 聊天窗口的结构化时间线、审批和交互体验；不是把终端 ANSI 输出换成聊天气泡，也不是在 iPhone 上直接运行 Codex。

## 先给结论

可以做，但需要新增一条“结构化聊天事件”链路，不能复用当前终端输出协议。

**首用前置条件：用户必须先在这台 Mac 启动 Codex Desktop 并完成登录。** 聊天入口检测到 Codex 未就绪时，不得直接创建 chat session；应展示“请先启动 Codex 桌面端并完成登录，然后返回重试”，并提供“重试”操作。检测通过后，实际聊天会话仍由 KN Agent 启动和管理 `codex app-server`；Codex Desktop 不承担 iOS 的消息转发或会话执行。

现有远程终端的本质是：iOS 发送按键，Mac Agent 把 PTY 的 ANSI 字节转发回来，iOS 用终端模拟器渲染。这对 shell 很正确，但它不知道一段文本究竟是助手回复、思考、命令、补丁、审批还是问题。

聊天模式的本质则是：Mac 上的 Agent 运行 Codex 的 `app-server`，把它给出的**结构化事件**标准化并转给 iOS；iOS 按事件类型显示不同卡片。终端仍完整保留，两个模式各自拥有会话和 UI，绝不互相“换皮”。

```text
终端模式（保留）
iOS Terminal UI ─ public terminal WSS ─ Cloud mapper ─ Agent PTY ─ Codex CLI
       ▲                                          │
       └──────────── ANSI output / keyboard ──────┘

聊天模式（新增）
iOS Chat UI ─ public chat WSS ───── Cloud mapper ─ Agent Codex app-server
       ▲                                           │
       └──── normalized timeline event / request ─┘
```

上图中的 `public` 是刻意的边界：iOS 只能认识稳定的 camelCase 聊天协议，不能直接认识 Codex app-server 的内部字段或 Agent 的 snake_case 消息。现有架构已明确规定 Cloud 是移动端公共协议与 Agent 内部协议之间的唯一适配层。[架构](architecture.md#职责与数据所有权)、[协议](protocol.md#跨端协议边界)

## 为什么不能“直接显示文本”

对当前 ChatGPT/Codex Desktop 安装包的静态逆向显示，它把会话条目分为 message、activity、结构化结果和 blocking request，而非统一文本流：

| Desktop 条目 | 移动端呈现 | 不是… |
| --- | --- | --- |
| `user-message`、`assistant-message` | Markdown 聊天气泡 | 终端行或纯文本标签 |
| reasoning、exec、patch、mcp-tool-call、web-search、subagent-activity | 可展开的“活动”卡；默认摘要、可看细节 | 助手正文气泡 |
| generated image、todo list、proposed plan、turn diff | 图片、任务、计划、Diff 等专用卡 | Markdown 代码块的伪装 |
| command/file/permission approval | 阻塞审批卡；操作按钮来自服务端允许的决定集合 | 本地固定“同意/拒绝” |
| user input、MCP elicitation、option/context picker | 阻塞表单或选择器 | 一段“请回答”文本 |
| 失败、中断、未知事件 | 清晰的状态/兼容性卡 | 伪装成助手成功回复 |

### Desktop 静态逆向结论（实现基线）

以下结论来自本机 ChatGPT/Codex Desktop **26.825.51511（build 7377）** 的静态代码，而不是对 UI 的主观模仿。目标为 `/Applications/ChatGPT.app/Contents/Resources/app.asar`（2026-08-30），其中 JS 已压缩为单行，故引用 archive 内文件名和字节偏移，不引用无意义的行号。升级时必须重新跑本节的兼容性检查；hash 文件名不是稳定 API。

| 观察到的逻辑 | 静态证据 | iOS 的实现要求 |
| --- | --- | --- |
| Desktop 先把 thread/turn/items 投影为展示条目，再渲染；并会滤除 `transcript_tail_flush`，用 `session-started/ended/failed` 与 BEM 事件补全进行中的 turn。 | `local-conversation-thread-CQimdxEP.js`，约 offset 32,000–42,000 | Agent 发送有序的规范化事件，iOS reducer 以 `chatSessionId + sequence`、`turnId`、`itemId` 归约；不得从 Markdown 或 websocket 到达顺序反推状态。 |
| 一个 turn 会先分类，而非按收到顺序直接平铺。分组器产出 `userItems`、一个最终 assistant item、agent activities、图片/工具输出、todo、plan、diff、approval、user input、MCP elicitation、remote/subagent/model 等独立槽位。 | `split-items-into-render-groups-ucS2t9A3.js`（全部 4,952 bytes）；`local-conversation-turn-BPH7L6Kk.js`，约 offset 30,400 | 建立版本化 presentation model 和 reducer，保留 item 的原始类型与稳定 ID；不要只实现 `message[]`。 |
| activity 有“连续可分组活动”与 standalone 两类。exec、patch、MCP tool、web search、动态工具依状态/元数据决定是否合并；生成图片、已拒绝/超时的自动审批等保持独立。最后一项、进行中、exploring 和 activity slice 状态会改变折叠/标题。 | `agent-activity-item-DVxcVQc0.js`，约 offset 30,500；`conversation-blocks-Bqf2uxPH.js`，约 offset 395,000–402,300、419,500 | 首期至少复刻“连续活动摘要可展开 + 独立结果卡”；不能把每次工具调用都渲染成一条普通助手气泡。折叠状态以稳定 group key 本地保存，新的进行中项默认可见。 |
| 渲染器按类型分派：assistant Markdown、图片预览/失败、patch/diff、todo、plan、命令、文件、web search、MCP、dynamic tool、subagent、system/error 都有独立分支。`userInput` 请求本身不进入普通 activity 流；已回答的 `user-input-response` 渲染为问答摘要。 | `conversation-blocks-Bqf2uxPH.js`，约 offset 395,000–408,000 | `ChatItemRenderer` 必须是 exhaustive dispatcher；未知 kind 显示安全兼容卡和原始摘要，不能丢弃或伪装成成功消息。 |
| 未完成的 permission request、user input、option/context picker 与 MCP elicitation 会阻塞 turn；UI 显示 “Waiting for your answer” 或 “Awaiting approval”，并抑制普通 thinking。 | `local-conversation-turn-BPH7L6Kk.js`，约 offset 35,000；`conversation-blocks-Bqf2uxPH.js`，约 offset 406,000–408,000 | request 优先于 composer/thinking。客户端只可提交当前 pending `requestId` 的服务端允许动作；resolve 前禁用新的发言，完成后保留结果卡。 |
| MCP elicitation 是 schema 驱动表单，不是纯文本。静态代码包含 enum/oneOf 单选、array/boolean 多选、string textarea、email/url/date、number/integer 的 min/max/step，且 Escape=cancel、Skip=decline、Continue=accept，并校验必填字段。 | `app-initial-B6Gk5KCN.js` 中 `replyWithMcpServerElicitationResponse` / `Complete this field to continue` | iOS 需按明确 schema 实现同等表单与无障碍标签；schema 不支持时显示不可完成/升级提示，不能猜字段或提交伪答案。 |
| 恢复使用 `resumeConversation`，带 workspace roots、permission default、collaboration mode 和已知 catalog entry；历史分页使用 `thread/turns/list` + `thread/items/list`，会检测重复 cursor。writer conflict 会变为 read-only、隐藏 composer，并提示“此任务正在另一处运行；关闭那里后重试”。 | `use-resume-conversation-if-needed-NVNitMZe.js`（全部 5,625 bytes）；`app-initial-B6Gk5KCN.js`，约 offset 1,754,066 | Agent 是 thread lease 的权威代理：恢复、分页、writer conflict、retry 都应有明确 public 命令/结果。冲突时 iOS 只读且可 Retry，绝不抢写或乐观发送。 |

**证据复现。** 可从 asar header 的文件表取各 asset 的 `offset` 与 `size`；payload 起点为 `8 + UInt32LE(app.asar, 4)`。这允许在不执行 Desktop 的情况下重读上述静态证据。静态证据说明渲染/状态分派，不足以证明 app-server 的运行时字段、异常时序或某个 RPC 已可用；这些必须用支持版本的 schema、capability probe 和录制事件 fixture 再验证。

### app-server：已逆向的真实 RPC 与字段（不是 UI 猜测）

本机 `codex-cli 0.148.0` 的 `codex app-server generate-json-schema --experimental` 和 `generate-ts --experimental` 可以生成该**正在使用的 CLI 版本**的完整 JSON-RPC contract。已实际启动 stdio app-server 并完成 `initialize`；运行时返回的 `userAgent` 是 `kn-protocol-research/0.148.0 (Mac OS 26.5.1; arm64)`，与 schema 的版本一致。下表的 raw 名称和字段来自该生成物，不是 Desktop renderer 的推断。升级 Codex 时必须重新生成并做 diff；它是版本化内部协议，不能直接作为 iOS 公共协议。

```text
iOS public camelCase DTO
  ⇅ Cloud mapper（不理解 Codex 内部字段）
Agent normalized command/event
  ⇅ Agent app-server adapter（唯一可接触 raw path、cwd、JSON-RPC 的位置）
Codex app-server raw JSON-RPC 2.0
```

| 聊天能力 | 已证实 raw 请求 | 请求/返回关键字段 | Agent 应输出的 KN 公共语义 |
| --- | --- | --- | --- |
| 初始化 | `initialize`，随后 client notification `initialized` | 请求：`clientInfo{name,title,version}`、`capabilities{experimentalApi,requestAttestation,…}`；返回：`userAgent`、`codexHome`、`platformFamily`、`platformOs`。 | Agent 仅记录协议版本与 capability；绝不把 `codexHome` 或登录信息发给 iOS。 |
| 新 thread | `thread/start` | 可传 `cwd`、`runtimeWorkspaceRoots`、`approvalPolicy`、`permissions`、`sandbox`、`model`、`historyMode`、`environments`、`dynamicTools`；返回 `thread`、有效 cwd/roots、model、approval/permission profile、reasoning effort。 | `chatStart` 只能暴露 Agent 白名单后的项目、模型和权限选择；不透传任意 path/config/instructions/dynamic tool。 |
| 恢复/历史 | `thread/resume`、`thread/read`、`thread/turns/list`、`thread/items/list` | resume 接受 `threadId`、可选 `excludeTurns`/`initialTurnsPage`；返回 `turnsBackwardsCursor`、`itemsBackwardsCursor`。分页请求使用 `threadId`、opaque `cursor`、`limit`、`sortDirection`；turn page 还可指定 `itemsView`。响应都给 `data`、`nextCursor`、`backwardsCursor`。 | `chatResume`、history page 与 timeline page；cursor 原样封装为 opaque token，iOS 不解释/拼接。 |
| 发言/中断 | `turn/start`、`turn/steer`、`turn/interrupt` | start 必填 `threadId`、`input[]`，可带 `clientUserMessageId`；返回 `turn`。interrupt 必填 `threadId + turnId`，空对象返回。raw `UserInput` 支持 text/image/audio/skill/mention，且某些 variant 直接含本机 `path`。 | `chatSend` 只接受已授权 attachment ID/文本，Agent 转换为 raw input；`chatInterrupt` 只允许 KN 管理的 `turnId`。本机 path 永不出公共 WSS。 |
| thread/turn/item 状态 | `thread/status/changed`、`turn/started`、`turn/completed`、`item/started`、`item/completed` | thread status 是 `notLoaded`/`idle`/`systemError` 或含 `activeFlags` 的 `active`；turn status 是 `inProgress`/`completed`/`interrupted`/`failed`；item lifecycle 均带 `threadId`、`turnId`、item 和毫秒时间。 | reducer 的权威状态源；不要从正文文本猜 running/完成。 |
| 文本和富活动增量 | `item/agentMessage/delta`、`item/reasoning/*`、`item/commandExecution/outputDelta`、`item/fileChange/outputDelta`、`item/fileChange/patchUpdated`、`item/mcpToolCall/progress`、`turn/plan/updated`、`turn/diff/updated` | assistant delta 是 `{threadId,turnId,itemId,delta}`；plan 是 `{explanation,plan[]}`；turn diff 是 `{diff}`；file patch 是 `{itemId,changes[]}`。 | 对应 assistant delta、activity、plan 与 diff event；大 output/diff 由 Agent 摘要/分页，不能逐帧直通 Cloud。 |
| 审批/回答（server → client JSON-RPC request） | `item/commandExecution/requestApproval`、`item/fileChange/requestApproval`、`item/permissions/requestApproval`、`item/tool/requestUserInput`、`mcpServer/elicitation/request` | 每个 request 有 JSON-RPC `id`；命令审批带 `approvalId`、command/cwd、`availableDecisions`；user input 带 `questions[] + isBlocking`；MCP form 带 `serverName`、`requestedSchema`，或是 `mode:url` 的 `{url,elicitationId}`。 | 固化为 `requestId` blocking card。Agent 保存 request/response ledger，iOS 只提交 server 声明的 decision/answer/action。 |
| 审批/回答回包 | 对应 server request 的 JSON-RPC response | 命令：`{decision}`，可为 `accept`/`acceptForSession`/policy amendment/`decline`/`cancel`；user input：`{answers:{questionId: answer}}`；MCP：`{action,content,_meta}`；dynamic tool：`{contentItems,success}`。 | `chatRequestResponse` 由 Agent 按 raw request type 编码；公共层不把 raw response union 交给 iOS，也不能固定成两个按钮。 |
| subagent | item `subAgentActivity`、`collabAgentToolCall`；thread/list 支持 `parentThreadId` 或 `ancestorThreadId` | activity 带 `agentThreadId`、`agentPath`；collab call 带 `senderThreadId`、`receiverThreadIds`、状态、prompt/model/effort。thread 本身有 `parentThreadId`、`agentNickname`、`agentRole`、`canAcceptDirectInput`。 | `chatSubagentOpen` 用 parent-child 关系校验和 `canAcceptDirectInput` 判定可交互；`agentPath` 不出 Agent。 |
| 资源/文件 | raw `fs/readFile`、`fs/getMetadata`、`mcpServer/resource/read`，以及 item 中的 `imageView.path`、command cwd、file changes | raw 协议天然包含绝对/本机路径，且可直接读写 fs。 | Agent 只把获授权的内容包装成 opaque `resourceId`、相对展示路径、MIME、分页/下载能力；**iOS 不调用 raw fs RPC**。 |

raw item 的已证实 union 是：`userMessage`、`agentMessage`、`plan`、`reasoning`、`commandExecution`、`fileChange`、`mcpToolCall`、`dynamicToolCall`、`collabAgentToolCall`、`subAgentActivity`、`webSearch`、`imageView`、`imageGeneration`、`enteredReviewMode`、`exitedReviewMode`、`contextCompaction` 等。因而此前的 `ChatItemRenderer` 不是凭空设计：它必须将这组 raw item 映射为稳定、无路径泄露的 public kind；遇到新 union 变体发送 `unknownActivity`，不丢数据也不自动可操作。

**可复现命令（只生成临时文件）。**

```bash
codex --version
codex app-server generate-json-schema --experimental --out /private/tmp/kn-app-server-schema
codex app-server generate-ts --experimental --out /private/tmp/kn-app-server-ts
```

每次支持新版本时，Agent CI/开发机都应将这两个输出与当前支持基线比较：新增 request/notification/item union 必须先进入 normalizer 的显式分支或 `unknownActivity`，再考虑开放 iOS action。

### 可点击对象：Desktop 已验证的行为与 iOS 对应方案

截图中的 subagent 标签、文件阅读行都不是装饰。以下是对当前 Desktop 静态实现补做的点击链路审计；iOS 必须实现同等的**信息可达性**，但不能照搬 macOS 的本地文件打开能力。

| 点击对象 | Desktop 的实际行为 | iOS 必须做什么 | 明确禁止 |
| --- | --- | --- | --- |
| inline subagent 标签/活动摘要 | inline row 仅在 `canOpen` 时绑定 `onClick`；打开的是父 conversation 的 Subagents side panel，并选择 `conversationId`。面板将子 agent 分为 Active/Done，显示名称、objective/状态摘要、waiting、耗时；选择后按需 hydrate 子线程和其 turns，再显示完整 transcript。只有 `canInteract` 时才允许交互。 | 点击进入 `SubagentDetailView`：显示 objective、状态、开始/结束时间、等待原因和**该 subagent 自己的分页 timeline**。请求必须带 `parentThreadId + subagentThreadId`，Agent 验证 parent-child 关系、当前设备和用户后才返回。默认只读；仅当 Agent 明确返回 `canInteract: true` 且没有 writer conflict/pending request 时显示输入与 action。 | 不把 subagent 当作公开链接；不根据显示名称猜 thread ID；不允许移动端绕过 parent thread 打开任意 Codex 历史；不在没有 lease 的情况下向子 agent 发言。 |
| subagent 内的文件、图片、diff、计划和工具活动 | 子线程复用正常 transcript renderer，所以其中的资源仍可点击；Desktop 会 hydrate 后再展示，而不是把父线程的一段文本复制过去。 | `SubagentDetailView` 复用同一个 `ChatTimelineRenderer` 与资源 action router；每个 item 保留其 `originThreadId`，文件详情请求也带该 ID，以便 Agent 做归属校验。 | 不把 subagent 输出合并进父 timeline 后丢掉来源；不因用户可见父线程就自动授予子线程文件访问权。 |
| assistant Markdown 中的本地文件路径、文件 citation | Desktop 的 Markdown renderer 将可解析且位于允许 cwd/project 范围的 path 转成 file link；点击走本机 host 的 side panel/文件定位逻辑，可附带 line/range。 | 发送 `openResource`：`resourceId`、`originThreadId`、可选 `line/startLine/endLine`；iOS 打开 `ResourcePreviewView`（文件名、相对路径、只读文本/代码预览、定位行、复制、经授权的下载副本）。Agent 只返回已授权项目内的内容或短时 signed download URL。 | 公共 WSS 中传绝对路径、`file:` URL 或任意本机路径；iOS 直接尝试访问 Mac 文件系统。 |
| end-resource / Outputs 文件卡、生成图片 | Desktop 会检查资源存在，图片可预览；其他文件可在 side panel 打开，并可选择原生 app、Finder 或下载副本。 | 统一走 `ResourcePreviewView`。图片全屏预览；文本/代码显示分页内容；二进制文件显示元数据和“下载副本/系统分享”（仅资源策略允许时）。 | “在 Finder 中显示”“以 Mac 原生 app 打开”等 Mac 专属操作；把资源路径暴露给 iOS。 |
| diff 中的文件名/位置 | 点击改动文件打开对应路径，并可定位到 `openLocation`；diff 本身有展开、收起、更多文件。 | `DiffDetailView` 支持同一安全 resource 引用、文件列表分页、定位行和展开状态；若源文件不可再读取，保留已持久化 diff 摘要并说明不可预览。 | 以 diff 文件名拼接路径后读取；把“文件已删除”伪装成可打开的当前文件。 |
| 外部 resource、Google Drive、网站/App 产物 | Desktop 根据类型在内置浏览器、MCP App/原生集成或本地预览中打开。 | 只接收经 Agent/Cloud 允许的 HTTPS resource URL 或受控 resource ID；使用 iOS 的安全浏览器/原生预览，并显示来源域名与外部跳转确认。 | 信任 assistant 文本里的任意 URL、`javascript:`/`file:` scheme，或让 Cloud 代替 Agent 解析本地路径。 |

**已定位的 Desktop 证据。** inline subagent 的点击和 `canOpen` gate 位于 `local-conversation-turn-BPH7L6Kk.js`；side panel 的 Active/Done、详情返回和 `canInteract` 位于 `local-conversation-subagents-panel-tab-D9HxLOXH.js`，打开/按需 hydrate 位于 `open-local-conversation-subagents-panel-NpKl1_tU.js`。文件 Markdown link、资源卡和 diff 打开逻辑位于 `conversation-blocks-Bqf2uxPH.js`；该代码还表明 Desktop 对本地文件会传 cwd/host，并可选择本机 side panel、原生 app、Finder 或下载副本。因此 iOS 的正确等价物是受授权的预览/下载服务，不是远程 `file://`。

为使上述交互可实现，公共协议增加下列只读能力（字段名可在 Phase 0 冻结时调整）：

| 命令 | 输入 | 成功结果 |
| --- | --- | --- |
| `chatSubagentOpen` / `chatSubagentTimelinePage` | `parentThreadId`、`subagentThreadId`、`cursor`、`clientRequestId` | 子 agent 元数据、`canInteract`、独立 timeline snapshot/page；未授权时返回明确的不可访问状态。 |
| `chatResourceOpen` | `originThreadId`、opaque `resourceId`、可选行范围、`clientRequestId` | 类型、显示名、相对展示路径、内容页或短时下载引用、可用 action；不返回绝对路径。 |
| `chatResourcePage` / `chatResourceDownload` | `resourceId`、cursor 或一次性下载确认、`clientRequestId` | 分页内容或受鉴权、短时且可撤销的下载引用。 |

Agent 必须在发任何 resource reference 前将它绑定到设备、用户、项目根、origin thread 和允许动作；所有页面/下载再次校验。`resourceId` 不得可由路径推导。若 resource 已删除、超出项目授权范围、父子关系改变或 Agent 无法读取，返回 `unavailable` 卡，而不是静默失败或回退为原始路径。

### 开工门槛：聊天窗口内完整交互清单，而非“主要页面能显示”

以下清单只覆盖 **Codex 聊天窗口 timeline、其 blocking interaction，以及从 timeline 打开内容所必需的详情页**；不覆盖整个 Codex Desktop。每项在 iOS 交付前只能处于三种状态之一：**已实现且有 fixture/E2E**、**明确显示为不可用并解释原因**、或**不在发布范围且聊天入口整体保持关闭**。不得把未实现按钮隐藏后宣称“与 Desktop 一致”。

| 交互族 | 已确认的 Desktop 行为 | iOS 发布要求 |
| --- | --- | --- |
| 会话打开、恢复、历史分页、writer conflict | 自动/手动 resume、分页 cursor 防重复；冲突只读、可 Retry，composer 不显示。 | 完整实现；这是所有可写入口的前置。 |
| composer 与 turn 控制 | 正常发送、运行中状态、interrupt、等待审批/回答时禁发；read-only 时不显示 composer。 | 完整实现，状态只能由 Agent 权威事件驱动。 |
| 用户消息 | 长消息展开/收起、复制、编辑上一条消息；编辑受 turn/状态限制。 | 展开/复制必须实现；编辑仅在 Agent 支持的 thread 状态下开放，否则明示不可用。 |
| 助手消息 | Markdown、代码、复制、从该处 fork/branch、时间/统计/goal/hook 详情；文件 citation 与文本选择 action。 | Markdown、代码复制、文件 citation、详情必须实现；fork/branch 只有 protocol 和 writer lease 都支持时开放，否则显示明确不可用。 |
| Sources 与 thread goal | Sources 侧栏记录附件、读取/创建/更新的资源、网站、Web 搜索、MCP 调用；thread goal 可富文本编辑、保存和 Revert。 | 来源清单和不可访问状态必须可读；goal 一期只读。只有具备 `chatThreadGoalUpdate` 的 revision/lease/幂等链路时才允许编辑。 |
| agent activity | 连续活动折叠/展开、更多/更少、命令/patch/MCP/web/dynamic tool 的独立详情；running、cancelled、failed 的视觉状态不同。 | 完整实现可读详情与折叠 reducer；不可执行的 tool action 不能伪装为可点击。 |
| diff、todo、plan、review | diff 文件展开、更多文件、打开文件定位；todo/plan 有专用卡与折叠状态。diff 还有视图/换行切换、上下文菜单、评论/回复/hunk action；Desktop 某些本地 diff 可 undo/reapply。 | 阅读、展开、文件预览、视图/换行与安全复制必须实现；评论、request-changes、undo/reapply 均为变更操作，除非另有带审批、revision anchor 和 Agent action，iOS 一律只读并说明。 |
| 审批、用户输入、option/context picker、MCP elicitation | requestId 关联、可选决定不同、表单校验、cancel/decline/accept、已完成摘要、阻塞 composer；其中 connectorAuth、urlAction、toolSuggestion 是独立 request kind。 | 全量实现其**状态机**，不允许仅支持“同意/拒绝”两个按钮。对 iOS 不承载的 connector/plugin 动作，必须提交该 request 的服务端允许 decline/cancel 或显示“在 Mac 上继续”，不能让 turn 无限阻塞。 |
| 图片 | 预览、全屏/编辑、复制、加入 composer、下载副本；Desktop 可打开 Finder。 | 安全预览、复制/加入 composer（能力可用时）、受权下载必须实现；编辑和 Finder 需分别明确支持或不可用。 |
| 文件、输出资源、外部链接 | Markdown 路径、资源卡、diff 路径、生成物、外链、Drive/MCP App/网站都可被不同 router 打开。 | 统一 resource/link router；所有本地资源用 opaque ID，外链需 scheme/域名策略与跳转确认。 |
| subagent 与后台任务 | inline 活动、Subagents 面板、Active/Done、目标/摘要/等待、按需加载子 thread；background terminal/agent 可单独打开。 | subagent 详情与其资源必须实现；后台 terminal 若一期不支持，显示“仅可在 Mac 查看”，不能把它混进聊天 timeline。 |
| MCP App、动态工具、生成应用 | 某些 timeline item 可打开交互式 app/resource、automation 详情或第三方连接器 UI。 | 每一 kind 在 capability 公告中逐项声明：有安全 renderer 才实现；否则显示只读摘要 + “此设备暂不支持交互式内容”。不得执行未知 app payload。 |
| 消息/资源上下文菜单与文本选择 | copy、打开/复制 link、下载副本、图片 add-to-chat；Desktop writing block 还可能可编辑表格、checklist、富文本。 | 只实现被 Agent 声明的安全 action；富编辑 writing block/表格/checklist 不可假装为普通 Markdown，未支持前保持只读说明。 |
| 无障碍与输入方式 | 展开控件、菜单、表单、复制、返回子 agent 均有 aria label/键盘路径。 | VoiceOver 标签、Dynamic Type、焦点返回、触控目标和 Reduce Motion 是发布门槛；每一种 blocking request 要有无障碍 E2E。 |

**审计边界。** 本节的结论来自当前 Desktop build 的压缩 renderer/route 静态代码，并由 archive 内 asset 路径与偏移复现；它用于定义 iOS 的事件模型、渲染分派和安全边界。app-server 的真实 schema、异常时序和 capability 仍须在 Phase 0 通过无副作用 probe 与版本化 fixture 验证，不能只凭 Desktop UI bundle 猜测。

### 复查补充：此前不能遗漏的侧栏、Review 与第三方交互

下面项目是在对 action-bearing asset 再次逐项扫描后补入。它们不是“可有可无的附加页面”：Desktop 会从聊天页或其侧栏进入这些交互；若 iOS 聊天入口对外开放，就必须按本表处理。

| 交互族 | Desktop 静态实现确认 | iOS 的等价边界与方案 |
| --- | --- | --- |
| Sources / provenance 侧栏 | Sources 侧栏汇总用户附加文件，以及资源在会话中的 `provided`、`read`、`created`、`updated`；还显示外部网站、WebMCP 调用的输入/输出（可截断）、Web 搜索次数、搜索词和已打开页面。文件能打开，页面 URL 能点击，MCP app source 可打开。 | 新增只读 `ChatSourcesView`，展示不可伪造的来源和活动记录；文件复用 `chatResourceOpen`，MCP 调用复用安全 tool-detail DTO。外部 URL 必须经 scheme/域名策略和二次确认；来源本身不可访问时仍显示 `unavailable`，不得悄悄从记录中删除。绝不把本机 `fsPath`、完整 tool secret 或未脱敏 input/output 交给 iOS。 |
| Thread goal | 线程 Goal 是独立 side panel：富文本输入、保存、Revert、保存中禁编辑；保存会 materialize 草稿并更新 conversation objective，失败显示 toast。 | 一期至少 `ThreadGoalDetailView` 只读。若要编辑，新增 `chatThreadGoalUpdate`：`threadId + expectedRevision + objective + clientRequestId`，Agent 必须持 writer lease、校验设备控制和 revision，并回传 `saved/staleRevision/writerConflict/denied`。没有这整条链路时显示“仅可在 Mac 修改”，不能把 goal 伪装成可编辑 Markdown。 |
| Git / branch / PR | Git action 模块包含 Commit、Push、Create branch、Create PR、Create draft PR、查看 PR；创建 PR 可选择 “Commit and push local changes”，运行中可 cancel，并会根据 workflow phase、工作树及能力状态禁用按钮。 | **范围外。** 这是 KN 已有的 Git/项目能力，不是 iOS Codex 聊天窗口的一部分；本聊天模式不展示或实现这些按钮。若将来产品要从聊天跳转 KN 的 Git 页面，另立需求并复用 KN 自有协议，不能借用本聊天协议。 |
| Diff 阅读、上下文菜单与 review | Diff 页支持 unified/split 切换、rich preview 切换、展开文件；context menu 有 Request changes、Open in target/Open with、Open in GitHub、Copy selection/path/relative path、Toggle line wrap。代码 diff 还接入 hunk actions、评论草稿、评论/回复、提交中状态、加载失败重试及 readonly comments。PDF diff 使用独立预览，内部链接可跳页。 | `DiffDetailView` 首期可做只读文件/行定位、unified/split、换行、选择复制、已授权相对路径复制、PDF 分页/内部跳转。Mac 编辑器/Finder/“Open with”不支持，改为 resource preview；GitHub 必须外链确认。Request changes、评论、新建/回复 review comment 都是协作写操作：除非实现带 `diffRevision`、行/side/hunk anchor、writer/审批、幂等和冲突显示的 `chatReviewActionRequest`，否则只读且明确提示。评论不能只按可见行号定位，必须绑定 diff revision 与 hunk anchor。 |
| Background agent | Desktop 把 background agent 作为独立 durable tab 打开，使用 `conversationId`，并把 `canInteract` 传给 tab；它不是父 timeline 中一段可随意继续输入的文本。 | 与 subagent 同一安全模型：`chatBackgroundTaskOpen` / timeline page 返回独立 thread、状态、日志分页与 `canInteract`。聊天内只显示摘要和“查看详情”；若没有结构化日志与 lease，显示“仅可在 Mac 查看/在终端继续”。绝不把原始 PTY 输入框嵌进 chat。 |
| 生成图片 | 图片上下文菜单提供 Add to chat、Copy image、Open in file manager、Download a copy。 | Agent 先授权 preview，才能 Copy；Add to chat 必须通过 `chatAttachmentCreate` 产出新的可发送 attachment reference；下载经一次性授权流。Finder 不可搬运，图片编辑必须是另一项有 schema/版本/保存失败语义的 capability，不能以 WebView 偷渡。 |
| MCP tool、MCP App 与认证页面 | Tool item 能展示结构化 input/output、自动化条目及 MCP app resource；Desktop 还能打开 MCP app view。其一方认证页面是受控 WebView，监测主 frame 导航/加载失败、超时，并提供 Try again；找不到匹配 app view 会显示错误页。 | **默认不执行任意 MCP/HTML payload。** iOS 仅展示脱敏的只读 input/output 和由 Agent 声明的 `readOnlyHint`。交互式内容只能在三种条件同时满足时开放：受支持的显式 schema 或 allowlisted HTTPS renderer、资源/来源/版本已由 Agent 绑定、每个 mutation 都有专用 capability + requestId/审批/幂等。Desktop 的认证 WebView、cookie、插件登录态和任意 iframe 不能迁移；未支持时显示只读摘要 + “在 Mac 上继续”。 |
| connectorAuth、URL action、tool suggestion | renderer 按独立 elicitation kind 分派：connectorAuth 显示 connector 授权请求；urlAction 显示 URL 动作请求；toolSuggestion 可提示安装 plugin，并在完成后显示 installed/declined/completed 摘要。三者均附 requestId，且未完成时是 waiting/approval 流的一部分。 | `connectorAuth` 不迁移 Desktop cookie/OAuth WebView：显示来源和原因，允许服务端声明的 cancel/decline，或跳转经 allowlist 确认的系统认证页；无安全回调链路则“在 Mac 上继续”。`urlAction` 仅允许 HTTPS、显式 host allowlist、用户确认和 requestId 回执；`toolSuggestion`（尤其 plugin install）一期只读说明 + decline/cancel，绝不在 iOS 安装 Desktop plugin。完成、拒绝、过期和失败都必须写回 timeline，解除或维持阻塞完全由 Agent 事件决定。 |

**本轮静态证据。** Sources 位于 `local-conversation-sources-side-panel-tab-DLWPdK31.js`；Goal 位于 `thread-goal-side-panel-content-Du9gJ_0T.js`；Git/PR 位于 `local-conversation-git-actions-CpaElZq_.js`；Diff context menu、rich/PDF preview 位于 `use-code-diff-context-menu-DRdtQJQD.js`、`code-diff-DqYlrALw.js`、`editor-diff-page-BfvRQ2MK.js`、`pdf-preview-diff-DlJ3hr1f.js`；后台 agent 位于 `open-local-conversation-background-agent-B4JT8jt4.js`；MCP 位于 `mcp-tool-item-content-eaxvzQ_W.js`、`mcp-app-resource-content-DoPnKeZb.js`、`mcp-extension-view-page-BVvGdiry.js`。其中 diff 评论的容器在 `diff-comment-card-C8aGiW0S.js`；真实交互由 code-diff renderer 继续分派，不能仅按该卡片容器判断功能范围。

这补充意味着“完整交互清单”中的 **diff、Sources、thread goal、git/PR、MCP/automation、background agent** 六项都要拆成只读与可写子能力逐一验收；不能因为正文时间线已经可渲染，就宣布聊天模式完成。

### 第二轮资产枚举：线程级入口不能被正文 renderer 掩盖

为避免只盯着 message card 而漏掉聊天页周边入口，又枚举了当前安装包中所有 `local-conversation`、`thread`、`artifact`、`automation`、`pull-request`、`MCP`、`diff` 和 `subagent` 相关 asset，并追踪其可见 action。以下是已发现、必须进入发布范围判定的剩余功能面。

| 功能面 | Desktop 已确认行为 | iOS 处理决定 |
| --- | --- | --- |
| Background terminal | 背景终端有独立 tab，并订阅 session snapshot；无输出时显示 `No output yet`。 | 不复刻为聊天原始输出。只可用 `chatBackgroundTaskOpen` 展示受限摘要/日志页；真正交互跳到既有 Terminal 模式并重新走 device-control。 |
| 线程菜单与生命周期 | overflow 菜单能够新建 scheduled task、打开 side chat、复制工作目录、重命名、归档；已归档 thread 有 `Unarchive and open` 和失败状态。线程页还展示 usage（模型、reasoning effort、speed、credits/cost）及设置入口。 | **范围外。** 除聊天历史列表打开/只读恢复所必需的状态外，Rename/archive/unarchive/side-chat、工作目录复制与 settings 均不属于聊天窗口。 |
| Scheduled automation | 自动化侧栏支持创建/应用修改、取消、暂停、恢复、删除确认、失败后 Retry save、跳转 settings，并有任务不存在/云端加载失败状态。 | **范围外。** 不迁移到聊天窗口；若聊天内容提及 automation，仅显示不可交互摘要。 |
| PR 侧栏与 PR-fix automation | 聊天页可打开 PR side panel；PR detail 有 loading、无效 URL/error 分支。相关 asset 还包含 PR-fix automation 入口。 | **范围外。** 不迁移到聊天窗口；PR 链接若出现在助手 Markdown 中，按普通外链确认处理。 |
| Artifact / writing block 与评论 | Desktop artifact shell 内有 comments sidebar：选中、回复、resolve、发送排队评论、删除和删除所有已解决评论；还有反馈提交/重试。 | `interactiveArtifact` 一期只读 snapshot，评论/resolve/delete/feedback 都不属于普通 chat reply，必须独立 artifact capability（artifact version、comment anchor、author、权限、幂等、撤销/失败语义）。没有该协议，卡片只能显示“此设备暂不支持编辑/评论”。 |

**结论的适用范围。** 上述是针对当前 build 的完整静态可达面审计结果。实现前仍要以 app-server 的无副作用 capability probe 与版本化 fixture 验证实际事件/动作是否可承载；任何静态上可达但无法由 app-server 安全承载的 action，必须在 iOS 以明确不可用状态呈现。

### Desktop 本机能力在 iOS 的替代方案（必须经 Agent）

Desktop 的 renderer 直接持有 `hostId`、`cwd` 和本机 bridge，因此可打开 Finder、本地 app、side panel、终端或写入系统剪贴板。iOS 不具备这些权限；**任何需要读取、下载、打开、运行或修改 Mac 上资源的操作都必须先发给 Agent，不能让 iOS 根据聊天文本自行执行。**

统一流程如下：

```text
iOS 点击 timeline action
  → Cloud 只鉴权/路由
  → Agent 校验 device + user + project root + thread/resource 归属 + capability + writer/request 状态
  → Agent 执行只读查询，或创建带审批/幂等键的受控动作
  → 返回公开 DTO（opaque ID、展示元数据、允许动作、明确失败码）
  → iOS 原生预览 / 分享副本 / 显示状态卡
```

| Desktop 本机功能 | iOS 不可直接搬运的原因 | Agent 方案与公开动作 | iOS 呈现与失败语义 |
| --- | --- | --- | --- |
| 在 side panel / 编辑器打开源文件、定位行、读取文件引用 | iPhone 无权访问 Mac 路径，且 `file:`/绝对路径会泄露隐私。 | `chatResourceOpen` / `chatResourcePage`：Agent 校验 resource 属于已授权项目和 `originThreadId`，返回文本页、MIME、相对显示路径、行范围和 revision；资源 ID 不可从路径推导。 | `ResourcePreviewView` 只读代码/文本预览与行高亮；`notFound`、`outsideProject`、`staleRevision` 均显示可理解状态，不降级为原始路径。 |
| Finder / 文件管理器 / Mac 原生 app 的“Open in” | iOS 不能驱动 Mac Finder 或指定 Mac app。 | 不暴露该动作。若目标是可导出的文件，仅提供 `chatResourceDownload`，由 Agent 创建一次性、短期、受鉴权的下载流；不复制原文件到 Cloud 长期存储。 | 显示“下载副本/系统分享”；不可下载或超限时显示原因。不得出现无效的“在 Mac 打开”按钮。 |
| Desktop 文本、代码、图片的复制 | Mac 剪贴板不能远程共享；复制本身不需要读 Mac 路径时无需 Agent。 | 已在 timeline payload 的小文本/图片 preview 可本地复制；完整资源先由 `chatResourceOpen` 获取受权内容后复制。 | 使用 iOS clipboard，并给 VoiceOver 成功提示；大文件不自动整段复制。 |
| 图片预览、加入下一条消息、下载、图片编辑 | Desktop 可以从本机路径解析原图；iOS 不能信任该路径。 | `chatResourceOpen` 返回安全 preview；`chatResourceDownload` 给副本；`chatAttachmentCreate` 只接受 Agent 公布且模型支持的 resource ID。图片编辑另列 capability，编辑结果必须成为新的受控附件。 | 预览/分享/“添加到聊天”按 capability 显示；模型不支持图片输入、资源过期或编辑不支持时明确禁用并说明。 |
| diff 的文件打开、源文件比较、undo/reapply | undo/reapply 会修改 Mac 工作区，且可能与正在运行的 Codex/用户编辑冲突。 | 读取走 `chatResourceOpen` / `chatDiffPage`。变更类动作只能新增 `chatWorkspaceActionRequest`：包含 action ID、thread/turn、预期 revision、幂等键；Agent 重验 writer、设备控制、项目根并生成审批 request，不能由 Cloud 或 iOS 直接执行。 | 首期 diff 默认为只读；以后若支持 apply/revert，必须显示审批卡、预期文件/变更摘要、冲突/过期 revision 的失败结果。 |
| background terminal / process | Desktop 可以直连本机 PTY；手机无法安全复用它，也不应把终端 ANSI 当聊天详情。 | `chatBackgroundTaskOpen` 仅返回结构化摘要/日志分页；若需要交互，跳转既有 Terminal 模式并经过现有 `device_control`。 | 聊天内显示任务状态、最后输出、跳转终端；不在 `ChatThreadView` 注入原始 terminal 输入框。 |
| subagent transcript 与继续任务 | 子 thread 可能有独立 writer、pending request 和资源授权。 | `chatSubagentOpen` / page 返回 parent-child 已验证的 timeline 和 `canInteract`；继续/interrupt/response 复用 chat action，且 Agent 重新取得该 subagent lease。 | 可读详情始终可尝试；无 writer 或不可交互时只读并显示 Retry/原端提示，绝不把父 thread 的 writer 继承给子 thread。 |
| 浏览本地预览站点、外部链接、MCP resource | localhost 只对 Mac 可见；第三方 URL、MCP payload 不能被信任。 | Agent 把 localhost 归类为 `macOnlyPreview`；外部 URL 要经过 scheme/host allowlist，返回 `externalLink`；MCP interactive resource 只有解析器受支持时返回受控 schema/HTTPS URL。 | `macOnlyPreview` 显示“仅可在 Mac 查看”；外部链接经确认在安全浏览器打开；未知或不安全 resource 仅显示摘要。 |
| 编辑 Desktop writing block、表格、checklist、应用内 artefact | 这不是普通聊天 Markdown；编辑可能触发生成、保存、第三方集成或本机写入。 | 作为独立 `interactiveArtifact` capability：Agent 发布 schema、版本、允许 mutation 和审批策略；无安全的 iOS renderer 时不下发编辑动作。 | 一期只读快照；只有完整 schema/保存/冲突/回滚/E2E 均具备时才允许原生编辑，不能嵌入 Desktop WebView。 |
| 发起/打开第三方 MCP App、Drive、automation | 登录态、OAuth scope、网页脚本和 connector action 不能从 Desktop 转移给手机。 | Agent/Cloud 仅转发经过白名单的只读 resource。任何 connector mutation 必须由专用 capability 与 requestId 审批；不代理 Desktop cookie。 | 显示来源、权限和外链确认；未支持时“在 Mac 上继续”或只读摘要。 |

所有 Agent action 都必须返回 `actionResult`，至少有 `accepted`、`completed`、`denied`、`notFound`、`outsideProject`、`writerConflict`、`staleRevision`、`unsupported`、`expired`；iOS 不能从超时推断成功。下载、变更和 connector 动作还须记录可审计 `operationId`，客户端重试携带相同 `clientRequestId`，以防重复下载、重复写入或重复授权。

这不是额外的“以后再说”工作：没有该 Agent capability layer，iOS 就只能显示静态文本，无法安全复刻 Desktop 的文件、subagent、diff、资源和任务交互。因此 Phase 0 先冻结 capability 公告和 action/result fixture，再开发任何可点击 UI。

## 必须先统一的概念

| 概念 | 含义 | 由谁持久化/判定 |
| --- | --- | --- |
| `terminalSessionId` | KN 现有远程 PTY 会话 ID，例如 `s_…` | Agent/Cloud 既有实现 |
| `chatThreadId` | Codex app-server 的会话/线程 ID；用于恢复聊天历史 | Agent 保存与 Codex 关联 |
| `chatTurnId` | 一次用户发言到结束、失败或中断的执行单元 | Agent 作为事件归属键 |
| `timelineItemId` | 一条可展示时间线项目的稳定 ID | Agent 生成或透传并去重 |
| `requestId` | 一个待答复交互（审批、问题、MCP 等）的稳定 ID | Agent 生成/持久化，iOS 回传 |
| `writer` | 当前真正持有 Codex thread 写入权的进程/应用 | Codex app-server 的 resume 结果 + Agent |

不要把前三者合并。`terminalSessionId` 指向 PTY 字节流；`chatThreadId` 指向 Codex 的结构化会话。现有 iOS 也已经将“接回远程 PTY 的 `resumeSession`”和“从本地 CLI 历史启动一个新远程会话的 `resumeLocalHistorySession`”严格区分。[`../kn-ios/Domain/TerminalMessage.swift`](../../kn-ios/Domain/TerminalMessage.swift)；因此新增聊天模式时应继续维持这个区分，而不是把历史恢复塞回终端 API。

## 与 Desktop 一致的关键交互规则

### 1. 同一 Codex 会话只有一个 writer

Desktop 在恢复碰到 writer conflict 时让会话只读：隐藏 composer，显示“此任务正在另一处运行；关闭那里后重试”，并提供 Retry；它不强杀另一端。[Desktop `local-conversation-thread…`、`use-resume-conversation-if-needed…`](#为什么不能直接显示文本)

移动端也必须如此：

1. 用户从终端切换到聊天，或从聊天切换到终端时，先检查旧模式的执行状态；
2. 若旧 writer 是 **KN 自己启动的**，提供“结束并切换”：发 interrupt/结束请求，收到明确 idle/ended 后才启动新模式；
3. 若 writer 是外部 Codex CLI 或 Desktop，聊天历史只读，隐藏输入框，显示 Retry；不得杀外部进程；
4. 不能把一个正在执行的 CLI 会话接管成“继续 token 流式回复”。可以展示已持久化内容，但断开后只能显示“已中断/请在原端继续”。

现有 Agent 已在 `main.rs` / `proto.rs` 对可变消息执行设备控制校验（`requires_device_control`）。这能防止多个 KN 连接抢同一台 Agent，但它**不能替代** Codex 的 thread writer 互斥：前者是 KN 设备控制，后者是 Codex 会话写锁。聊天动作应接入这条已存在的校验路径；任何仍在开发、尚未合并的控制状态实现都不能作为一期前提。[`agent/src/main.rs`](../agent/src/main.rs)、[`agent/src/proto.rs`](../agent/src/proto.rs)

### 2. 审批和问题会阻塞一个 turn

当 thread 为 `active + waitingOnApproval` 或 `active + waitingOnUserInput`：

- 禁止新的 composer 提交；
- 将对应审批卡/表单置于该 turn 的底部；
- 显示“等待你的回答”，并抑制普通“正在思考”的占位；
- 只有该请求 resolve 后才解除阻塞；
- 连接断开时保留“待同步/无法提交”的状态，不能乐观地当作已批准。

这来自 Desktop turn renderer 的 `hasBlockingRequest` 判断和 request handler，而不是视觉偏好。[Desktop `local-conversation-turn…`](#为什么不能直接显示文本)、[Desktop `app-initial…`](#为什么不能直接显示文本)

审批卡不能自行假设决定集合。当前 Codex app-server 中，命令、文件和通用权限的可选决定/作用域并不完全相同；iOS 仅按服务端下发的 `allowedDecisions`/`options` 展示。例如 `decline` 与 `cancel` 的语义不同：前者允许 agent 继续，后者可能中断本 turn。未知 option 需要显示但不猜语义。

### 3. 已完成交互的历史恢复要有 KN 自己的账本

Desktop 会把回答过的 user-input、permission 和 MCP 交互作为 synthetic timeline item 维护。实际 app-server 历史恢复不能保证重新给出完整的题目、选项和答案。因此：

- Agent 在发出任何 blocking request 前先持久化 request；
- Agent 在收到 iOS response 后先持久化 response/完成状态，再把 response 给 app-server；
- iOS 本地缓存只用于短期离线展示和快速打开，不是跨设备同步副本，也不是跨端权威；
- 缺少账本的旧历史只能显示“交互详情不可恢复”，绝不能编造答案、权限状态或已执行操作。

这是避免“恢复后 UI 看上去能继续、实际权限语义已丢失”的安全要求。

## 现状盘点：哪些可以复用，哪些必须新建

### 已有终端链路：保留，不改语义

| 现有组件 | 当前职责 | 对聊天模式的结论 |
| --- | --- | --- |
| `../kn-ios/Presentation/Terminal/TerminalTabManager.swift` | 维护多 PTY tab、ANSI 输出缓冲、尺寸、重连与输入 ACK | 保留为 Terminal 专属，不承载聊天时间线 |
| `../kn-ios/Domain/TerminalMessage.swift` | `startSession/input/ctrl/resize/replayOutput` 和 `output(ansiText:)` 公共协议 | 保持兼容；不要向其中塞聊天事件 |
| `agent/src/session/{manager,output,input,persistence}.rs` | PTY 生命周期、ANSI 回放、输入、session.json | 保留为 Terminal 专属 |
| `agent/src/session/types.rs` | 现有 `Native`（Agent 持 PTY）/`Relay`（Desktop 持 PTY）模型 | 不把 app-server 聊天伪装为 PTY 或 Relay |
| `agent/src/project_session_index.rs` | 扫描 Codex JSONL，为项目历史提供标题和 ID | 可作为“选择一个历史 thread”的只读入口；不是完整聊天解析器 |

证据：`TerminalSession` 保存 `transcript` 并有 ANSI 长度裁剪；`ServerMessage.output` 明确注释为“ANSI 转义文本”。[`../kn-ios/Domain/Entities/TerminalSession.swift`](../../kn-ios/Domain/Entities/TerminalSession.swift)、[`../kn-ios/Domain/TerminalMessage.swift`](../../kn-ios/Domain/TerminalMessage.swift)。Agent 的 Relay IPC 也明确只是“Desktop 拥有 PTY，Agent 中继并轮询输入”。[`agent/src/ipc.rs`](../agent/src/ipc.rs)、[`agent/src/session/types.rs`](../agent/src/session/types.rs)

### 必须新建的聊天链路

| 端 | 新职责 | 计划模块/文件（拟新增，名称可在实施前小幅调整） |
| --- | --- | --- |
| Mac Agent（本仓） | 管理 app-server 子进程、thread/turn、事件规范化、请求账本、单 writer、重连补偿 | `agent/src/codex_chat/`：`manager.rs`、`app_server.rs`、`timeline.rs`、`requests.rs`、`ledger.rs`、`store.rs`、`protocol.rs`；接入 `agent/src/main.rs`、`agent/src/proto.rs`、`agent/src/ipc.rs` |
| Cloud（`../kn-cloud`） | 把内部 chat 消息映射为 camelCase 公共协议；路由、ACK、鉴权、离线重发 | `kn-cloud-ws` 的 `MessageTypes`、Agent/Mobile dispatcher、Agent/Mobile protocol mapper、会话协调/Redis 与 protocol tests；按现有类名定位，不能绕过 mapper |
| iOS（`../kn-ios`） | 独立 Chat domain、WSS 解码、timeline store、SwiftUI renderer 和请求协调 | `Domain/Chat/`、`Data/Network/DTOs/ChatMessageDTOs.swift`、`Data/Repositories/Chat…`、`Presentation/Chat/`；接入 `Domain/TerminalMessage.swift` 的 transport 扩展或拆出 `Domain/ChatMessage.swift` |
| Desktop（本仓 `desktop/`） | 第一阶段不重写 Desktop 聊天 UI；只保证与 Agent 的 chat writer/互斥规则一致 | 若 Desktop 需要展示 KN chat，新增独立 chat IPC/client；不得改动既有 PTY panel 路径 |

“拟新增”不代表已存在。本仓目前 `agent/src/proto.rs` 的注释和枚举只覆盖 terminal/项目工作台方向；`docs/protocol.md` 也要求新增移动端消息同时修改 Cloud Mobile dispatcher/mapper 和 iOS 编解码。这正是 chat 不能只改 iOS 的原因。[`agent/src/proto.rs`](../agent/src/proto.rs)、[协议变更流程](protocol.md#变更流程)

## 建议的公共聊天协议（设计契约，实施前由三仓共同评审）

公共协议不应透传 app-server 原始 JSON。建议使用一个稳定、可版本化的 envelope：

```json
{
  "type": "chatTimelineEvent",
  "data": {
    "schemaVersion": 1,
    "chatSessionId": "knchat_…",
    "threadId": "codex-thread-id",
    "turnId": "…",
    "sequence": 42,
    "event": { "kind": "assistantDelta", "itemId": "…", "text": "…" }
  }
}
```

约束：

- `sequence` 单调递增，iOS 以 `chatSessionId + sequence` 去重、检测缺口；不能以到达顺序猜测；
- 每一个 `event.kind` 必须有明确 schema；新增 kind 必须向后兼容；
- 大文本（完整 diff、命令输出）应提供摘要和可分页详情，不在每个 websocket 帧复制全量；
- client action 必须携带 `requestId`、`threadId`、`turnId` 和幂等 `clientRequestId`；
- Agent/Cloud 只接受当前 controller 对待答复 request 的 response；过期、已解决或不属该 thread 的 response 必须拒绝；
- 所有时间由服务端/Agent 产生，以稳定排序；iOS 不以本地时间重排。

### 除 timeline event 外必须存在的命令与恢复协议

仅有 `chatTimelineEvent` 还不能可靠恢复：手机会断线、WebSocket 可重复投递、一个 diff 也不能永久塞在单帧里。因此公共协议还必须明确包含下列**命令类**消息；具体 JSON 字段在 Phase 0 定稿后冻结并写入三仓 fixture。

| 方向 | 建议类型 | 必填关联键 | 用途与失败处理 |
| --- | --- | --- | --- |
| iOS → Agent | `chatStart` | `deviceId`、`projectPath`、`profile`、`cwd`、`clientRequestId` | 新建 thread。重复 `clientRequestId` 必须返回同一结果，不能开启第二个 Codex writer。 |
| iOS → Agent | `chatResume` / `chatHistoryPage` | `chatSessionId`、`threadId`、`cursor` | 打开历史或请求分页；返回 snapshot/page，不以 WebSocket 曾经收到的 delta 拼历史。 |
| iOS → Agent | `chatSend` / `chatInterrupt` | `threadId`、`turnId`（interrupt） 、`clientRequestId` | 发言或结束 KN 自己管理的 turn；外部 writer conflict 返回只读状态而不是失败后强行重试。 |
| iOS → Agent | `chatRequestResponse` | `requestId`、`threadId`、`turnId`、`clientRequestId` | 提交审批、表单、picker 或 MCP 答复；必须验证 request 尚待处理、属于该 thread 和当前 controller。 |
| iOS → Cloud | `chatAck` / `chatResync` | `chatSessionId`、`lastSequence` | ACK 已落地事件；发现 sequence 缺口或重连后请求 snapshot / 缺失区间。 |
| Agent → iOS | `chatStarted` / `chatSnapshot` / `chatHistoryPage` / `chatActionResult` | `chatSessionId`、`threadId`、`clientRequestId` | 让 UI 可区分“动作被接受”“已完成”“需重试”“已只读”，不能从 event 到达顺序推断。 |

`chatStart` 至少要固定 profile、cwd、project/device 归属、Codex model/reasoning 配置、sandbox/approval policy 和 collaboration mode。它们都由 Agent 的安全配置白名单决定；iOS 只能选择 Agent 明确公布的选项，不能透传任意 CLI 参数、环境变量、shell 命令或权限策略。

大对象采用“timeline 摘要 + 明确详情请求/分页”的方式。生成图片、用户附件或大 diff 只传受鉴权的资源引用和元数据；iOS 通过已有鉴权下载通道读取，不能把本机绝对路径或未经验证的 `file:` URL 放到公共协议中。

最小事件集（第一期必须覆盖）：

| 类别 | `kind`（建议 camelCase） | iOS 组件 |
| --- | --- | --- |
| 生命周期 | `threadSnapshot`、`threadStatusChanged`、`turnStarted`、`turnCompleted`、`turnInterrupted`、`turnFailed` | 状态条、composer enablement |
| 消息 | `userMessage`、`assistantMessageDelta`、`assistantMessageCompleted` | Markdown 气泡；delta 合并到指定 item |
| 活动 | `reasoning`、`commandExecution`、`fileChange`、`mcpToolCall`、`webSearch`、`subagentActivity` | `ActivityDisclosure` |
| 结构化结果 | `todoList`、`proposedPlan`、`turnDiff`、`generatedImage` | 专用卡 |
| 阻塞请求 | `commandApprovalRequested`、`fileApprovalRequested`、`permissionApprovalRequested`、`userInputRequested`、`mcpElicitationRequested`、`connectorAuthRequested`、`urlActionRequested`、`toolSuggestionRequested`、`optionPickerRequested`、`contextPickerRequested` | Approval/Form/Picker/安全外链或“在 Mac 上继续”卡 |
| 交互完成 | `requestResolved`、`requestFailed` | 已批准/拒绝/已回答状态 |
| 兼容性 | `unknownActivity`、`historyIncomplete`、`writerConflict` | 安全降级卡/只读 callout |

`assistantMessageDelta` 是 token 级视觉效果的正确来源：UI 只追加结构化 delta，使用节流合并刷新；它不是抓取 PTY 字节，也不是重新分词。历史恢复获得的是完整 item，不承诺回放既往 token 的到达速度。

## iOS 信息架构与页面改造

### 产品入口与模式选择

在项目页/会话入口明确提供两个并列入口：

- **终端**：现有 `TerminalSessionContentView`；适合 shell、完整 ANSI、手工命令。
- **聊天**：新增 `ChatThreadView`；适合 Codex 结构化对话、工具活动、审批、计划和 diff。

不要在同一个 tab 内把 `WKWebView` 终端和聊天列表相互替换，也不要把聊天会话加入 `TerminalTabManager.sessions`。建议上层新增 `SessionMode` 与 `SessionRoute`，分别拥有 `TerminalTabManager` 和 `ChatThreadStore`。当前 `MainTabView` 在 `App/KnApp.swift` 创建一个 `TerminalTabManager`，项目会话历史选择后也直接调用 `resumeLocalHistorySession`；这是需要拆开路由的准确接点。[`../kn-ios/App/KnApp.swift`](../../kn-ios/App/KnApp.swift)、[`../kn-ios/Presentation/Projects/Sessions/ProjectSessionHistoryView.swift`](../../kn-ios/Presentation/Projects/Sessions/ProjectSessionHistoryView.swift)

建议的聊天页层级：

```text
ChatThreadView
├── ChatNavigationBar（标题、连接/只读/执行状态、模式切换）
├── ChatTimelineList（倒序或正序均可，但使用稳定 item ID）
│   └── ChatItemRenderer（按 kind 分派；未知 kind 有安全 fallback）
│       ├── MessageBubble
│       ├── ActivityDisclosure
│       ├── DiffCard / PlanCard / TodoCard / ImageCard
│       └── BlockingRequestCard
├── TurnStatusFooter（thinking / waiting / interrupted / failed）
└── ChatComposer（仅 writer 且无 blocking request 时可编辑）
```

需要复用现有 `Presentation/DesignSystem/` 的颜色、字体、卡片和 toast，而非复制终端的 WebView。Markdown、代码高亮、图片预览、diff 展开应以原生 SwiftUI 组件实现，并为 Dynamic Type、VoiceOver、复制文本和 Reduce Motion 设计测试。

### iOS 推荐文件切分

| 层 | 计划职责 | 建议文件 |
| --- | --- | --- |
| Domain | 不依赖 SwiftUI 的 `ChatThread`、`ChatTurn`、`TimelineItem`、`ChatRequest`、状态机、repository/use case | `Domain/Chat/ChatModels.swift`、`ChatMessage.swift`、`ChatRepository.swift`、`ChatUseCases.swift` |
| Data | DTO 与 WebSocket 事件解码/编码、缓存仓储 | `Data/Network/DTOs/ChatMessageDTOs.swift`、`Data/Repositories/ChatRepositoryImpl.swift` |
| Presentation | 单一 observable `ChatThreadStore`；事件去重、分页合并、delta 批处理、request 动作 | `Presentation/Chat/ChatThreadStore.swift`、`ChatThreadView.swift`、`ChatItemRenderer.swift`、`ChatComposer.swift` |
| Components | 各种非文本 UI | `Presentation/Chat/Components/{ActivityDisclosure,ApprovalCard,UserInputCard,McpElicitationCard,DiffCard,PlanCard}.swift` |
| App wiring | 依赖注入、route、WSS fan-out | `App/AppContainer.swift`、`App/KnApp.swift`、`Data/Network/WebSocketTransport.swift` |

当前 `MessageTransport` 只输出一个 `AsyncStream<ServerMessage>`，`TerminalTabManager` 已经承担全局 WSS 消息分流。实施时必须增加一个上层 `SessionEventRouter`（或把分发移动入 transport 的多订阅机制），让 Terminal 和 Chat 各自订阅自己的消息；不能让两个 manager 竞争同一个单消费者 stream。`TerminalTabManager` 源码对此已经有明确注释，说明多 Task 竞争同一个 `AsyncStream` 会死锁/挂起。[`../kn-ios/Domain/Protocols/MessageTransport.swift`](../../kn-ios/Domain/Protocols/MessageTransport.swift)、[`../kn-ios/Presentation/Terminal/TerminalTabManager.swift`](../../kn-ios/Presentation/Terminal/TerminalTabManager.swift)

## 建议采用的产品默认设计（供审阅）

这一节不是 Desktop 逆向事实，而是基于其语义、当前 KN 产品形态和手机屏幕限制给出的**建议默认决策**。若你认可，Phase 0 就把它们固化为验收标准，避免开发中临时决定。

### A. 入口：同一历史，两种明确打开方式

在项目会话历史的每条 Codex 历史上，展示一个会话详情页/菜单，而不是把模式切换藏在聊天正文里：

```text
历史条目「修复登录问题」
├── 聊天查看                 ← 默认；结构化 timeline、只读也可打开
├── 继续聊天                 ← 仅 thread 无外部 writer、且当前设备支持 chat 时显示
└── 在终端中打开             ← 现有 PTY 路径；明确是另一种工作方式
```

- 聊天页导航栏的 `terminal` 图标只作为“在终端中打开”的显式动作，不使用常驻 segmented control；终端页同理提供“聊天查看”。这样用户始终知道自己在什么模式，两个 renderer/state manager 也不会互相替换。
- **已结束历史**：`聊天查看` 可直接打开；`继续聊天` 或 `在终端中打开` 会先取得新的 writer，二者不会自动同时启动。
- **本 iOS/KN 正在运行的会话**：切换时弹出确认 sheet：`结束并切换` / `留在当前模式`。只有旧端报告 idle/ended 才跳转。
- **外部 CLI/Desktop writer**：历史仍可查看；所有写入入口隐藏，顶部显示“此任务正在其他设备/应用中运行。请在那里结束后重试”。这里的“其他设备”是指同一 Mac 上的其他应用或进程，不是 KN 的跨 Mac 同步。

这满足“终端和聊天并存、切换必须结束”的产品规则，也与 Desktop 的 writer conflict 行为一致。

### B. 聊天页：内容优先，活动与交互各归其位

```text
┌──────────────────────────────────────┐
│ ‹ 项目名 · Codex        已连接  ⋯    │
├──────────────────────────────────────┤
│ 你：请修复登录问题                     │
│                                      │
│ Codex：我先检查认证流程。              │
│                                      │
│ ▸ 已分析 4 个文件 · 2 个命令           │  ← ActivityDisclosure
│                                      │
│ ┌ 需要确认                             │
│ │ 将运行：npm test                     │
│ │ 工作目录：…/project                  │
│ │ [查看详情]                           │
│ │ [允许一次]               [拒绝]      │
│ └────────────────────────────────────│
├──────────────────────────────────────┤
│ 等待你的确认                           │
│ [输入框（禁用）]                       │
└──────────────────────────────────────┘
```

- user/assistant message 使用 Markdown 气泡；代码块可复制、横向滚动，不能执行 HTML/脚本。
- reasoning、命令、搜索、MCP、subagent 默认折叠为 activity disclosure；标题使用事实摘要（如“运行命令”“修改文件”），展开才显示完整 stdout、参数、diff。
- todo、计划、diff、生成图片使用独立卡；diff 默认显示摘要和文件数，点开底部 sheet/详情页，避免挤占手机 timeline。
- timeline 用 `LazyVStack` + 稳定 item ID；活跃 assistant delta 在同一 item 内节流追加，用户正在浏览旧消息时不强制滚到底部，只显示“回到最新”按钮。
- 只有 `idle` 且无 pending request、非只读时显示 composer；`active` 可保留草稿但不允许提交第二个 turn，`waiting*` 则明确禁用 composer。

### C. 审批就是聊天内的阻塞交互

审批不是设置页，也不是 toast。它是当前 turn 的一个 `BlockingRequestCard`，放在触发它的命令/文件活动之后；同时底部 composer 变为“等待你的确认”。用户处理完卡片，当前 turn 才继续。

| 请求 | 卡片必须展示 | 用户动作 | 完成后的样子 |
| --- | --- | --- | --- |
| 命令审批 | 命令、cwd、理由、是否网络/高风险、完整输出入口 | 服务端给出的 Allow/Decline/Cancel/作用域选项 | 冻结为“已允许/已拒绝/已取消”，保留命令结果链接 |
| 文件审批 | 文件列表、每文件 add/modify/delete、diff 摘要和完整 diff | 服务端给出的 Apply/Decline/作用域选项 | 冻结为“已应用/已拒绝”，可查看 diff |
| 通用权限 | 申请的能力、scope、理由、影响范围 | 完全由 `allowedDecisions` 渲染 | 冻结结果；未知 decision 不显示猜测按钮 |
| 用户问题 | 标题、问题、单/多选、Other、是否敏感 | 原生单选/多选/文本输入 + 提交 | 显示所选答案；敏感答案用掩码 |
| MCP elicitation / picker | 服务端定义的字段/来源/安全说明 | 原生表单或 picker；明确 Dismiss | 显示提交/取消状态，永不让请求悬挂 |

交互细则：

1. 卡片按钮只依据服务端当前 request 的 `allowedDecisions`；例如 `decline` 与 `cancel` 必须用不同文案和确认语义，不能合并成一个“拒绝”。
2. 可能扩大权限的动作（例如“本会话始终允许”）使用二次确认；`允许一次` 是优先的安全默认值。没有 server capability 时不创造“始终允许”。
3. 点击后卡片进入 loading，所有动作禁用；网络断开显示“等待重连，尚未提交”，不乐观标记为成功。
4. 只读 follower 和外部 writer 不显示可操作按钮；显示“请在启动此会话的窗口继续”。
5. 任一 pending request 都应发本地/远程通知，但通知只写“Codex 需要你的确认”，不泄露命令、文件名、prompt 或权限内容。

### D. 首个可写版本的范围：完整安全闭环，非“只显示文本”

建议首个对外可写版本覆盖下表中的**全部必须项**。原因是少一个 blocking request 类型，用户就可能在手机上把会话卡死；不应把“首期”理解成只实现 assistant 气泡。

| 范围 | 首版要求 |
| --- | --- |
| 读取/流式 | 历史分页、user/assistant Markdown、assistant delta、reasoning/activity、命令、文件变更、todo、plan、diff、图片、失败/中断、writer conflict |
| 写入 | 新建聊天、继续聊天、发送消息、interrupt、结束并切换 |
| 阻塞交互 | command/file/permission approval、user input、MCP elicitation、option picker、context picker；全部通过统一 request schema 与卡片 renderer 实现 |
| 可靠性 | request ledger、action 幂等、ACK/resync、断线恢复、只读降级、未知事件可见但不可操作 |
| 安全 | controller 校验、项目/cwd 白名单、Markdown 安全渲染、敏感内容不进 Cloud 长期库或通知 |

不会在首版承诺的内容：任意本机文件浏览器、任意附件上传、在手机上编辑超大 patch、复刻既往 token 到达节奏、接管外部 writer。若 app-server 给出尚未建模的活动，先用“未支持的 Codex 活动”卡展示摘要和原始安全字段；若它是 blocking request，则显示“需要在 Mac 上继续”并提供明确 dismiss/cancel 路径，绝不让 turn 无期限挂起。

## Mac Agent 改造

### 新的 ChatSessionManager，而非扩展 PTY Manager

新增 `agent/src/codex_chat/ChatSessionManager`，职责如下：

1. 使用 `codex app-server --stdio` 启动受 Agent 管理的子进程，并在初始化声明所需实验能力；
2. 对每个 chat session 维护 `chatSessionId → threadId → app-server connection` 映射；
3. 将 app-server notification/request 转为上表稳定 timeline events；
4. 将 iOS 提交消息、interrupt、approval、user input、MCP response 转回相应 app-server RPC；
5. 写入 snapshot、timeline journal、request ledger 和幂等操作状态；
6. 严格执行 writer conflict、controller 与 thread 状态机；
7. 断线后向 Cloud 重放**未确认的规范化事件**，而非重新执行 Codex。

它不应进入 `agent/src/session/manager.rs`：该文件开 PTY、选择终端尺寸、合并按键输入，且 `OutputFanout` 持久化的是终端日志。这些职责对 app-server 结构化协议是错误的。[`agent/src/session/manager.rs`](../agent/src/session/manager.rs)、[`agent/src/session/output.rs`](../agent/src/session/output.rs)

### Codex 运行环境与凭据边界

Agent 需要启动的是用户本机已安装的 `codex app-server`，但不能自行拼 PATH 或复制登录凭据：

- 解析 `codex` 二进制应复用 `kn_common::path::find_binary()`，以适配 launchd 缺少登录 shell PATH 的情况；
- `KN_HOME` 仍只控制 KN 的 `~/.kn` / `~/.kn-dev` 运行配置和本计划的 Agent journal；它**不是** Codex 数据目录；
- Codex 默认仍使用该 macOS 用户既有的 `~/.codex` 登录态与会话存储；只有明确的受控测试才可传隔离 `CODEX_HOME`；
- 不复制 `auth.json`、access token、rollout 原文或完整 prompt 到 `~/.kn`、Cloud、iOS 日志或 crash report；Cloud 只负责消息路由；
- 每次 chat start 先确认用户已启动 Codex Desktop 并完成登录，再验证 `codex app-server` 可启动、当前 CLI 版本受支持、cwd 是该设备已授权项目目录。Codex Desktop 未启动或未登录时，客户端必须显示“请先启动 Codex 桌面端并完成登录，然后返回重试”，并提供“重试”操作；不得自动启动 Desktop、读取或复制其凭据。其余任一条件不满足时只显示对应的不可用状态，不能退回到抓 PTY 文本的伪聊天模式。

这是现有路径规范的直接延伸：KN Rust 侧通过 `config_dir()` 管理 KN 配置，二进制解析使用 `find_binary()`；两者不能替代 Codex 自己的配置/认证根目录。[`common/src/path.rs`](../common/src/path.rs)、[`AGENTS.md`](../AGENTS.md)

建议持久化目录（由 `kn_common::path::config_dir()` 计算，不自行拼 `$HOME`）：

```text
<config_root>/agent/chat/
  sessions/<chatSessionId>/metadata.json
  sessions/<chatSessionId>/timeline.jsonl
  sessions/<chatSessionId>/requests.jsonl
  sessions/<chatSessionId>/snapshot.json
```

文件写入应遵循仓库现有配置写入的原子替换/锁原则；journal 则追加时要带 sequence、fsync 策略和定期 compact。敏感提示、命令输出和授权内容均是本机私密数据，不上传到 MySQL 作为业务历史，除非产品另行明确授权。

### 启动、恢复、结束状态机

```text
open history / start new
  → Agent preflight（Codex binary + app-server capability + writer 状态）
  → thread/start 或 thread/resume
  → readOnly | idle | active | waitingApproval | waitingUserInput

active 的本 Agent 聊天 writer
  → 用户点“结束并切换”
  → turn/interrupt
  → 收到 turn interrupted + thread idle
  → 切换 Terminal 或其他 thread

外部 writer conflict
  → readOnly（保留历史）
  → Retry
  → 成功 resume 后才显示 composer
```

恢复历史分页使用 app-server 的 `thread/turns/list(itemsView: "full")`，而不是只扫描 Codex JSONL，也不能依赖当前运行时未支持的 `thread/items/list`。Agent 应将 CLI 版本、initialize `userAgent` 和 preflight 能力随 metadata 保存；这为升级回归检测提供依据。

### Agent 需要改的既有接点

| 文件 | 改动类型 | 原因 |
| --- | --- | --- |
| `agent/src/lib.rs` | 导出 `codex_chat` | 新模块注册 |
| `agent/src/main.rs` | 创建 manager、启动/停止、WSS inbound/outbound 路由 | 现有 terminal 消息在此分派 |
| `agent/src/proto.rs` | 增加 chat internal DTO/解析/序列化；保持 terminal DTO 不变 | 当前 public/内部协议在此定义 |
| `agent/src/ws_client.rs`、`agent/src/ws_client/outbound_frame.rs` | 新事件帧大小、确认/重试和断线策略 | 聊天事件与长 diff/输出有不同大小语义 |
| `agent/src/ipc.rs` | 增加仅 Desktop/本机所需的 chat 生命周期查询或 relay；不可让 iOS 绕 Cloud 直连 | Desktop 与 Agent 的本地边界 |
| `agent/src/project_session_index.rs` | 可选：扩展历史条目以标记“可聊天恢复/仅终端恢复” | 当前只索引标题/ID，非完整 timeline |
| `agent/tests/` | manager、ledger、协议、writer conflict、断线、幂等、未知 event test | 不依赖真实模型随机触发事件 |

## Cloud 改造

Cloud 是唯一 mapper，因此此工作至少包含三个改变，缺一不可：

1. **定义 mobile chat public DTO。** 增加 chat command 和 chat event 白名单；全部 camelCase；校验 `chatSessionId/threadId/turnId/requestId` 所属设备和当前用户。
2. **映射与路由。** Mobile dispatcher 把 iOS action 映射为 Agent internal chat message；Agent dispatcher 把 timeline event/request 映射回 mobile event；任何未知字段保留兼容信息但不自动执行。
3. **可靠投递与协调。** Redis 维护在线路由、事件 ACK/重发窗口、当前 controller 与 pending request；它保存短期协调状态，Agent 的本地 journal 才是可恢复事件的权威源。

需要检查/修改的真实边界由仓库文档和现有实现明确列出：

- `../kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/dispatch/MobileMessageDispatcher.java` 与 `AgentMessageDispatcher.java`：两端都有 `ALLOWED_TYPES` 白名单；
- `../kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/protocol/MobileProtocolMapper.java` 与 `AgentProtocolMapper.java`：字段命名与内部/公共 DTO 映射入口；
- `../kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/service/MessageRelayService.java`：当前把 Agent 的 `ansi_text` 映射成 iOS 的 `ansiText`，因此不能让 chat 混入 `output`；
- `ProtocolMapperBoundaryTest`、`DispatcherBoundaryTest`、`MessageRelayServiceTest`：新增协议的边界测试位置；以及涉及会话/ACK 时的 Redis 断线恢复。

[Cloud 模块](cloud.md#模块与入口)、[协议变更流程](protocol.md#变更流程)

Cloud 不应解析 Codex Markdown、补丁、审批策略或 app-server 原始字段；它只验证路由/权限/幂等并执行公共协议映射。这样 Codex 升级时，兼容层集中在 Agent，iOS 不会被内部协议击穿。

## 交付阶段与验收门槛

### Phase 0：协议契约与 fixture（先做）

产物：三仓共享的版本化 JSON fixture，至少覆盖 user/assistant delta、reasoning、连续可折叠 activity 与 standalone activity、command、file change、diff、图片成功/失败、todo、plan、所有 blocking request、MCP schema form、connectorAuth、urlAction、toolSuggestion、decline/cancel、unknown event、writer conflict、interrupted/failed、ACK 重放、sequence 缺口、断线重连和 history pagination。还必须覆盖 inline subagent 的 `canOpen`/`canInteract`、background agent/terminal、子线程分页、子线程内资源，以及 file/resource 的授权、已删除、越界、下载过期和行定位；Sources/provenance（含截断/不可访问）、thread goal 只读、diff 的评论 anchor/只读、interactive artifact 只读 snapshot，以及 MCP app 的 loading/error/retry/unsupported。Git/PR、自动化、线程菜单、归档和设置均为范围外，不纳入本聊天窗口 fixture。每个 fixture 还应断言 presentation reducer 的结果（group、排序、blocking、composer 是否可编辑、collapsed 默认值），而不仅是 DTO 能否解码。另增加**服务端能力公告 + feature flag**：未公告 chat capability 的 Agent、Cloud 或 iOS 一律不显示可写聊天入口。

验收：iOS DTO/renderer unit test 和 Agent normalizer test 全部只用 fixture 就能覆盖事件分支；不依赖“提示模型恰好调用某个工具”。

### Phase 1：只读聊天历史

范围：项目历史列表增加“以聊天方式查看”；Agent 读取完整 thread 并发 `threadSnapshot`，iOS 只展示消息/activity/计划/diff；未知事件安全降级。

验收：可打开已完成 Codex 历史；终端仍可打开同一项目；没有 composer、没有审批动作；长历史分页、缺页和无法恢复详情均有明确 UI。

### Phase 2：KN 管理的新聊天会话与 token 视觉

范围：新建/恢复可写 chat thread、`assistantMessageDelta` 合并、turn 状态、interrupt、KN 内 writer 切换。

验收：一条 assistant 消息仅有一个稳定 item；网络重连不重复 delta；结束并切换必须等待 idle；外部 writer conflict 一律只读 Retry。

### Phase 3：审批和交互闭环

范围：command/file/permission approval、user input、MCP elicitation、option/context picker；request ledger、幂等 response、恢复后的已解决卡片。

验收：每一种请求有 fixture + Agent/Cloud/iOS 测试；response 重发不会重复执行；未识别 decision 不展示危险默认按钮；用户关闭 picker 也会发送明确 dismiss/cancel，永不悬挂。

### Phase 4：富内容与跨端韧性

范围：图片、todo、计划、diff 大对象分页、后台恢复、通知跳转、desktop follower/read-only 提示。用户上传图片/文件、语音转写和其他富输入不是本阶段的默认承诺：只有 Codex app-server 的输入能力、资源上传/下载权限、隐私说明和三仓协议都已单独设计并通过安全审查后，才作为后续独立小阶段加入。

验收：App 重启、Agent 重启、WSS 重连、Codex writer conflict、旧版本事件均有回归测试；敏感历史不意外进入日志、推送或云端长期库。

每一阶段都保持终端的既有测试通过：iOS `TerminalTabManager`、Agent PTY/session、Cloud terminal protocol tests 不能因为聊天协议重构而改语义。

## Codex 升级后的检测与降级

Codex app-server 的 schema 不是运行时能力保证，必须在每次支持版本变更时做兼容性 gate：

1. 记录 `codex --version`、app-server initialize 的 `userAgent` 和 Agent chat protocol version；
2. 在隔离测试目录生成 experimental schema 并与已支持版本 diff；
3. 启动 app-server 运行无副作用 capability probes（例如分页、resume、请求 response）；
4. 用 Phase 0 的 fixture contract test 回放 parser/renderer；
5. 新事件自动进入 `unknownActivity`，版本不支持的动作禁用并说明原因；绝不猜 RPC response，绝不自动批准。

一个已知反例是：schema 曾列出 `thread/items/list`，但当前运行时返回“not supported yet”。所以“生成 schema 能通过”不能作为上线证据；必须同时有 runtime probe。

## 上线、迁移与回滚

聊天能力应以独立 feature flag 灰度，而不是和终端功能一起发布：先允许开发设备，再按 Agent 版本、Cloud mapper 版本和 iOS 最低版本联合放量。Cloud 需要拒绝旧客户端发送 chat action，并向旧客户端隐藏聊天入口；新客户端连接旧 Agent 时也只能显示“此电脑暂不支持聊天模式”。符合版本条件但 Codex Desktop 尚未启动或未登录时，则显示启动并登录 Desktop 的引导，不显示可写聊天界面。

新增 Agent chat journal 时要使用独立目录和版本化 metadata，不读取或改写现有 PTY session 文件。升级时只做向前兼容读取；遇到未知 ledger 版本保留只读历史并提示升级，不能删除旧记录。回滚时关闭 feature flag、停止新 writer、保留本地 journal 供新版本恢复；不得为回滚清空用户的 Codex 或聊天历史。

隐私与安全验收还必须覆盖：Markdown 不执行 HTML/JavaScript；链接、图片、diff 和工具输出按不可信内容显示；审批卡的按钮只来自 request payload；完整 prompt、命令输出、审批账本不得写入 analytics、崩溃报告、推送文案或 Cloud 长期业务库。

## 本计划刻意不承诺的事情

- 不承诺把外部 CLI/Desktop 正在运行的 thread 无缝接管到 iOS；这是 Desktop 也不做的 writer conflict 场景。
- 不承诺复现历史 token 的原始到达时间；历史显示完整 item，只有活跃 app-server 的 delta 做实时视觉。
- 不承诺将所有未来 app-server 新事件立即拥有精美专用 UI；未知事件先安全可见、不可操作，再由 fixture 驱动新增 renderer。
- 不将 terminal 和 chat 合并成一个“万能会话”类；二者的传输、恢复、写入权和渲染模型不同。

## 审阅时需要产品/技术负责人确认的选择

1. iOS 缓存保留多久；建议仅作可清除的短期本地缓存，Agent 为权威，Cloud 不保存完整 prompt/output，也绝不把会话同步到另一台 Mac。
2. 审批的默认策略：建议所有破坏性或未知权限均显式询问，不能因终端模式已有远程控制权而自动同意。
3. 哪些 Codex action 可在移动端出现：建议先覆盖 command/file/user-input，MCP/通用权限使用 capability 驱动的卡片而非硬编码保证。
4. 终端到聊天的入口：建议历史列表同一条历史显示“终端恢复”和“聊天查看”两个明确动作；仅当旧 writer 已结束才显示可写聊天。

## 事实来源索引

- 本仓架构/协议边界：[`docs/architecture.md`](architecture.md)、[`docs/agent.md`](agent.md)、[`docs/cloud.md`](cloud.md)、[`docs/protocol.md`](protocol.md)。
- Agent 现状：[`agent/src/proto.rs`](../agent/src/proto.rs)、[`agent/src/ipc.rs`](../agent/src/ipc.rs)、[`agent/src/session/types.rs`](../agent/src/session/types.rs)、[`agent/src/session/manager.rs`](../agent/src/session/manager.rs)、[`agent/src/project_session_index.rs`](../agent/src/project_session_index.rs)。
- iOS 现状（相邻仓）：[`../kn-ios/Domain/TerminalMessage.swift`](../../kn-ios/Domain/TerminalMessage.swift)、[`../kn-ios/Domain/Entities/TerminalSession.swift`](../../kn-ios/Domain/Entities/TerminalSession.swift)、[`../kn-ios/Domain/Protocols/MessageTransport.swift`](../../kn-ios/Domain/Protocols/MessageTransport.swift)、[`../kn-ios/Data/Network/WebSocketTransport.swift`](../../kn-ios/Data/Network/WebSocketTransport.swift)、[`../kn-ios/Presentation/Terminal/TerminalTabManager.swift`](../../kn-ios/Presentation/Terminal/TerminalTabManager.swift)、[`../kn-ios/App/KnApp.swift`](../../kn-ios/App/KnApp.swift)。
- Codex Desktop 静态行为：本机 ChatGPT.app 的 `app.asar` 中列出的四个 webview assets；版本指纹和逐个函数摘录应在实施前再次从当前安装包复核。
- Cloud 实现核对：`../kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/{dispatch,protocol,service}/` 中的 dispatcher、mapper 和 relay；对应 `src/test/java/dev/kn/cloud/ws/` 的边界/集成测试。
