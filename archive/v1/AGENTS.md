# WikiAltas 工程约束

## 后端

- 分层：`cmd → httpapi → domain → store`；store 不 import run/llm
- 正文唯一入口：`PUT content`（人和 LLM 共用）
- SQL 全参数化；错误上抛，禁止 `_ = err`
- 无 Proposal 表；无 Entity/Fact 真相表
- 关系 type 枚举写死在代码
- ID：UUIDv7

## 前端

- Semi UI 组件优先；禁止 hex 色；禁止反向覆盖 `.semi-*`
- 视觉对齐 `VISUAL_SPEC.md`（飞书母本，AI 助手=右侧可收起侧栏，见 §1.5）；与 DESIGN_V2 冲突以 VISUAL_SPEC 为准
- 默认编辑态；AI 面板可收起
- Run 事件分两条线消费：`response.*` 整条交给 Semi 的流式归约器（不自己造投影），
  `wikiatlas.*` 归宿主部件（任务面板 / 用量环 / 提醒条）；两边都不 dump payload
- `src/views` `src/components/**` `src/lib`；单文件建议 ≤300 行

## 工具面纪律（给 Altas 加工具前读）

- 加新工具前先问「现有面是不是已覆盖它」——与 skill 维护纪律同源，别重复造。
- LLM 破坏性写的护栏基准 = 只许删**自己造的、无版本可回滚**的对象（空节点/空资料/关系边）；
  有正文的节点/资料走 `status=archived` 归档（树与检索默认排除、可恢复、写工具拒写），
  带正文的硬删永远是**人类界面专属**（`write_content` 拒空写守护的是同一件事）。
- 批量工具一律波次模式：预检全在前、all-or-nothing、带上限（patch_sections / upsert_relations 是范本）。
- 只读面（get_tree / list_works / audit / read_*）不进 writeToolNames——docMode=read 与 oneShot 只剥写工具。

## 验证

```bash
# 后端
wsl -d Ubuntu -- bash -lc 'export PATH=/usr/local/go/bin:$PATH; export GOCACHE=/root/.cache/wikiatlas-go-build; cd /root/WikiAltas/backend && go test ./...'

# 前端
wsl -d Ubuntu -- bash -lc 'export PATH=/root/.nvm/versions/node/v24.20.0/bin:$PATH; cd /root/WikiAltas/frontend && npm run build'
```

## Skill

- 运行时读 `skills/wiki-writing/`，不注入 examples/
- create_wiki 加载 SKILL.md → core.md → 一个 media-*.md
- 目标是 series/universe 节点时，额外注入 readme.md（系列主文/合集写法）
- 内容规则（骨架、题记、字数、参考资料）以 skill 为唯一权威；系统提示只留宿主/流程，不复述细则
- **维护纪律**：看到新毛病，先问「现有哪条原则已经覆盖它？」
  - 已被覆盖 → 不是 skill 的问题，别加规则（去查提示/注入/模型行为）；
  - 没被覆盖 → **加原则（讲道理），不加个例（列情况）**；
  - 个例证据（逐篇判决、已知偏差、语域补丁）只进 `examples/`，不进运行时规则。

## 交接须知（新会话 / 小模型先读：上面各节之外的雷都在这）

### 操作纪律
- dev 栈生命周期统一走 `scripts/start-dev.sh`（start / stop / restart / status / logs，target be|fe；
  停 be 前自动查 runs 表有没 running 工单，`--force` 强制）。旧 `start-backend.sh` / `start-frontend.sh`
  已降级为兼容包装，别再直接 pkill 起新进程（pidfile 在 /tmp/wikiatlas-dev/，按组 kill 不误伤 5200 第二套）。
- 重启后端前先查有没有在跑的工单（重启会杀掉 run）：读 `backend/data/wikiatlas.db` 的 runs 表，有 running 就等。
- 跑 `scripts/start-backend.sh` 时，**命令文本里别出现它 pkill 的目标名**（pkill -f 会自伤，exit 144）。
- `.secrets/credentials.md` 与 settings 库里的任何 key：只读来用，**永不打印 / 永不提交 / 永不写进代码与文档**。
- 推送 / 打 tag 前都先问用户。双远端：origin=Gitea；GitHub=`include-H/WikiAltas`
  （推送要走代理 `192.168.1.253:7890` + gh 一次性凭据，别改 git 配置）。
  tag `v*` → 自动 release + Docker Hub `hao0114/wikialtas`。
- 有并行会话在跑（GA / GA 前端线）：跨会话用 SendMessage；任务与 GA 无关就不碰 `/root/gamemanager`。

### LLM 使用
- 思考档位合法值只有 `none/minimal/low/medium/high/xhigh/max`——**没有 "off"**（配置入口已把旧值归一到 none）。
- 机械判定（媒体库扫描 / 层级判定 / 题名变体 / 相关性判官）用 `runs.ActiveClientFor("none")` 直出；
  写作 / 问答工单用默认档——别把无思考档拿去做需要推理的活。
- `ActiveClient()` 没配 Key 会退回 mock/echo：任何直接调模型的端点必须先用 `store.HasAPIKey()` 守卫。
- 模型偶发空答 / 畸形输出（真见过判官返回 `{"links":[]}`）：关键路径要有重试兜底 + 日志，别静默成"没找到"。
- **工具面只剥两种用户显式选择**：`docMode=read`（只读模式）与 `oneShot`（工具栏「翻译/解释」
  按钮的能力契约，`context.oneShot=true` → 运行时剥写工具）。意图（answer/continue_wiki…）
  只是提示里的任务框定与 skill 预载依据，**不决定能力面**——猜错了（错字/纠偏/问句暗示干活）也要能干。
  护栏在动作层（修订可回滚、写工具自带空内容/残稿拒绝）与提示（This task 段的执行偏向：
  "能核实就落下去，别汇报完等批准"；一次性输出不落盘），别再加"按猜测锁工具"的设计。
- **质量门与节点扫库按 `wroteContent` 触发，不看意图标签**——问答单也可能顺手写正文；
  验证跟着"事实发生"走。
- **context 字段是三件套**：前端 `types.ts` 的 CreateRunBody.context + AiPanel 构造（用
  `NonNullable<CreateRunBody['context']>`，别铺平 Record）+ 后端 `domain.RunContext` 字段
  + `manager.go` 到 ctxMap 的映射。缺一环就静默丢（"@ 引用"就这样坏过：前端发了、
  执行器读了、中间没人接住——Go 结构体解码不认识的 key 不报错）。

### 视觉
- `VISUAL_SPEC.md` 是视觉宪法（与 DESIGN_V2 冲突以它为准）；颜色全走 Semi token、禁新写 hex、禁反向覆盖 `.semi-*`。
- **紫色（purple / violet / indigo）是 AI 专属强调色**，非 AI 元素一律不许用。
- 组件 API 先查 Semi MCP（semi-mcp）文档再写，**禁止凭记忆造 props**。

### 媒体库模块（改这块必读）
- **LLM 永不主动建库 / 挂链**：AI 只产出建议（KV `library_ai_suggestions`），用户点确认才执行；
  "写完之后"的关联由宿主钩子做确定性扫库（node-sweep）。扫描是低频确定性流程，稳态零 LLM。
- Komga 层级判定按「书单指纹 + 口径版本」缓存：**改了判定 prompt 必须把 `komgaJudgeVersion` 加一**，
  否则旧判定永远不会重判；判定失败退回启发式，不允许挡扫描。
- `library_ai_suggestions` 是共用 KV（sweep 产出 / 池子建议 / per-node 忽略共存）：保存时只替换自己那份，
  并持 `suggestionsMu` 锁。三套「不要」别混：池子全局忽略（manifest，条目没价值）vs
  节点页 per-node 忽略（Dismissed，别向这个节点提起，可附理由）vs 建议拒绝
  （Rejected，模型给的挂法不对——条目仍是一等公民；拒绝理由进 KV，下次「AI 对一遍」
  作为 previousRejections 喂回模型二次判断；模型复读被拒挂法时行不藏、挂「上次被拒」标注）。
- API 形状纪律：空列表一律给 `[]` 不给 `null`（前端 `.length` 直接崩）；KV 里多字段共存时，
  别写"空列表 = 没有记录"这类守卫（会吞掉同记录里的其他字段）。
- 真库联调：192.168.1.4（Emby 8081 / Komga 8082 / GameAtlas 3000）；UI 改动要起 dev server（5173）
  用 playwright 无头真跑再截图（playwright 在 `/root/.npm/_npx/` 下），别只看构建通过。

### 正文流（Altas 实时长草稿，改编辑器前必读）
- 正文是**经由写正文工具的流式参数**吐出来的（write_content.contentMd / patch_section.newMarkdown）。
  后端把它投影成 `wikiatlas.content.delta` 的"段"（~200ms 节流，`respEmitter.contentArgsStream`），
  staging 提前到开写时发出；逐条 `function_call_arguments.delta` 对这类工具**不再转发**
  （没有消费者，且一个条目几千条会把前端全量归约卡死）。前端由 `lib/liveDraft.tsx` 的
  Provider 累积（值/写入口拆两个 context，避免全应用跟着重渲染），WorkView/DocView 拼好后
  经 `liveMd` 进 MarkdownEditor 只读显示；patch_section 用 `lib/mdOutline.ts` 的 `spliceSection`
  原位拼接（**镜像后端 tools.ReplaceSection 的匹配规则**，改一边要同步另一边）。
- 收尾有两种语义：`commit()`（留末稿垫底防闪回）/ `abort()`（中断退回真实内容，工单被停/失败）。
- 落块动画（对齐飞书写文档）：流式帧走**增量替换**（按块指纹找分歧点，前缀块保住 DOM），
  当前段挂 `.live-tail`（压暗 + 文末紫色脉动条 `.live-writer-bar`），段落写完摘类 → CSS 过渡渐显回正文。
  改这块注意：①BlockNote 的 `replaceBlocks` 删除集**不能为空**（源码取 `t[0].id` 会炸），纯追加帧走 `insertBlocks`；
  ②想让"渐显"有节点可落，就不能整篇重建（全量替换会把所有动画重播）。
- BlockNote 两个已踩的雷：①`lastLoaded` 基准要存**替换后实际序列化出来的样子**（题记块往返有损：
  750 字进 739 字出），否则 `editor.isEditable` 切换派发的 change 回声会把正文误标"未保存"；
  ②同一基准也是加载守卫（value === lastLoaded 就跳过），改成别的键会让打字回声触发整篇重替换、
  光标弹走、后续按键全丢。
- 联调 mock：起第二套后端 `WIKIATLAS_MOCK_WRITE_DELAY_MS=24000 go run ./cmd/wikiatlas -db /tmp/x.db
  -addr 127.0.0.1:8090 -seed -skill …`（无 key 即 mock）+ 前端 `WIKIATLAS_DEV_PORT=5200
  WIKIATLAS_API_TARGET=http://127.0.0.1:8090 npx vite`；mock 写入步走真实分流路径（逐段喂参数）。
