// Package domain mirrors shared/types.ts as Go structs with camelCase JSON tags.
package domain

import "github.com/google/uuid"

type UUID = uuid.UUID

type WorkKind string

const (
	WorkKindUniverse WorkKind = "universe"
	WorkKindSeries   WorkKind = "series"
	WorkKindWork     WorkKind = "work"
)

type Medium string

const (
	MediumGame  Medium = "game"
	MediumMovie Medium = "movie"
	MediumTV    Medium = "tv"
	MediumManga Medium = "manga"
	MediumBook  Medium = "book"
	MediumOther Medium = "other"
)

type WorkStatus string

// Visibility 决定访客能否看到该节点（public 且所有祖先都 public 才可见）。
type Visibility string

const (
	VisibilityPublic  Visibility = "public"
	VisibilityPrivate Visibility = "private"
)

const (
	WorkStatusStub  WorkStatus = "stub"
	WorkStatusDraft WorkStatus = "draft"
	WorkStatusReady WorkStatus = "ready"
	// WorkStatusArchived 是**隔离态**而非生命周期末档：正文还在、可改回
	// stub/draft/ready 随时复活，但树与检索默认都不露它，LLM 的写工具拒写。
	// 设计动机：LLM 建错节点的退路只能是"隐藏"（可恢复），硬删仅留给空节点。
	WorkStatusArchived WorkStatus = "archived"
)

type Author string

const (
	AuthorHuman  Author = "human"
	AuthorLLM    Author = "llm"
	AuthorImport Author = "import"
)

type RelationType string

const (
	RelationAdaptationOf RelationType = "adaptation_of"
	RelationSequelTo     RelationType = "sequel_to"
	RelationSpinOffOf    RelationType = "spin_off_of"
	RelationRemakeOf     RelationType = "remake_of"
	RelationExpansionOf  RelationType = "expansion_of"
	RelationReferences   RelationType = "references"
	// same_series is display-only; store only when not derivable from tree.
	RelationSameSeries RelationType = "same_series"
)

// ValidRelationTypes is the frozen dictionary. Code-change only; no runtime invention.
var ValidRelationTypes = map[RelationType]bool{
	RelationAdaptationOf: true,
	RelationSequelTo:     true,
	RelationSpinOffOf:    true,
	RelationRemakeOf:     true,
	RelationExpansionOf:  true,
	RelationReferences:   true,
	RelationSameSeries:   true,
}

// DirectionalRelationTypes 是**有方向、合起来必须构成 DAG** 的那几种。
//
// 它们描述的是"谁从谁那里来"（改编自、续作于、衍生自…），所以 A→B 与 B→A
// 同时存在是自相矛盾的：读取侧会看到「A 是 B 的续作」和「B 是 A 的续作」两句话，
// 无从判断。建边时据此查环。
//
// `same_series` 与 `references` **天然是互相的**，不参与查环——它们本来就是
// 对称关系，两边各存一条是正常的。
var DirectionalRelationTypes = map[RelationType]bool{
	RelationAdaptationOf: true,
	RelationSequelTo:     true,
	RelationSpinOffOf:    true,
	RelationRemakeOf:     true,
	RelationExpansionOf:  true,
}

// EmbyLibraryRole 是用户在设置里给一个 Emby 媒体库标的角色（2026-09-12）：
// work = 正片（喂「建档建议」）；mixed = 混杂内容（喂「关联到节点」的候选）。
// 键值对不上（改名/重建）时按 name 兜底匹配。
type EmbyLibraryRole struct {
	ID   string `json:"id,omitempty"`
	Name string `json:"name"`
	Role string `json:"role"` // work | mixed
}

// 设置里的合法角色值。
const (
	EmbyLibraryRoleWork  = "work"
	EmbyLibraryRoleMixed = "mixed"
)

// LibraryManifest 是媒体库的后台扫描快照：库里有哪些条目、各自挂没挂链。
// 纯确定性数据（零 LLM 成本）；建议池、Altas 的库工具都读这份清单。
type LibraryManifest struct {
	ScannedAt         string                 `json:"scannedAt"`
	PreviousScannedAt string                 `json:"previousScannedAt,omitempty"`
	Entries           []LibraryManifestEntry `json:"entries"`
	// 忽略（持久化，扫描时保留）：单条按 key="source:entryId"；整组按容器。
	IgnoredKeys   []string       `json:"ignoredKeys,omitempty"`
	IgnoredGroups []IgnoredGroup `json:"ignoredGroups,omitempty"`
}

// RoleOf 是清单条目的固有角色：Emby 混杂库 = mix，其余（GA/Komga/正片库）= work。
func (e LibraryManifestEntry) RoleOf() string {
	if e.Source == "emby" && e.ContainerKind == EmbyLibraryRoleMixed {
		return LinkRoleMix
	}
	return LinkRoleWork
}

type LibraryManifestEntry struct {
	Key            string  `json:"key"` // source:entryId
	Source         string  `json:"source"`
	EntryID        string  `json:"entryId"`
	Title          string  `json:"title"`
	TitleAlt       *string `json:"titleAlt,omitempty"`
	ReleaseDate    *string `json:"releaseDate,omitempty"`
	CoverImage     string  `json:"coverImage,omitempty"`
	URL            string  `json:"url"`
	Kind           string  `json:"kind"`             // tv|movie|game|book|collection…
	Format         string  `json:"format,omitempty"` // comic|novel（Komga）
	Extra          string  `json:"extra,omitempty"`  // 给 LLM 的补充：GA系列/合集成员…
	ContainerKey   string  `json:"containerKey"`     // 容器（库/系列/合集）内键
	ContainerTitle string  `json:"containerTitle"`
	ContainerKind  string  `json:"containerKind,omitempty"`
	LinkedWorkID   string  `json:"linkedWorkId,omitempty"`
	FirstSeenAt    string  `json:"firstSeenAt"`
}

// IgnoredGroup 是被整组忽略的容器。
type IgnoredGroup struct {
	Source         string `json:"source"`
	ContainerKey   string `json:"containerKey"`
	ContainerTitle string `json:"containerTitle"`
}

// LibraryAiSuggestions 是 AI 建议清单（「AI 对一遍」与「写完后节点扫库」共用）：只建议，不执行。
type LibraryAiSuggestions struct {
	GeneratedAt string                `json:"generatedAt"`
	Model       string                `json:"model"`
	Suggestions []LibraryAiSuggestion `json:"suggestions"`
	// Dismissed 是节点页里逐条忽略过的建议（per-node，不等于池子的全局忽略）：
	// 再扫同一节点时不再重新提起，但不影响该条目在别处的候选资格。
	Dismissed []LibraryAiDismissal `json:"dismissed,omitempty"`
	// Rejected 是被拒绝的 AI 建议（全局，分属「挂法」）：条目本身仍是池子里的
	// 未挂链一等公民，拒绝理由会在下次「AI 对一遍」时喂回给模型二次判断。
	// 它和两套忽略互不混淆：全局忽略说的是"条目没价值"，这里是"挂法不对"。
	Rejected []LibraryAiRejection `json:"rejected,omitempty"`
}

type LibraryAiSuggestion struct {
	Key          string  `json:"key"` // source:entryId
	Source       string  `json:"source"`
	Action       string  `json:"action"` // link | archive | ignore
	TargetNodeID string  `json:"targetNodeId,omitempty"`
	Confidence   float64 `json:"confidence"`
	Reason       string  `json:"reason"`
	// Origin 标记建议出处：sweep = 节点扫库（写完后触发）。「AI 对一遍」重刷时保留 sweep 建议。
	Origin string `json:"origin,omitempty"`
	// Role 是条目角色（work=正片级 / mix=混杂级），建议产出时由条目元数据确定性给出。
	Role string `json:"role,omitempty"`
	// 显示字段：节点页秒开不重拉库（池子那边仍以扫描清单为准）。
	Title      string `json:"title,omitempty"`
	Kind       string `json:"kind,omitempty"`
	Format     string `json:"format,omitempty"`
	Extra      string `json:"extra,omitempty"`
	CoverImage string `json:"coverImage,omitempty"`
	URL        string `json:"url,omitempty"`
}

// LibraryAiDismissal 是节点页被忽略的一条建议（key 对某个 workId 不再提起）。
type LibraryAiDismissal struct {
	WorkID string `json:"workId"`
	Key    string `json:"key"`
	Source string `json:"source"`
	Title  string `json:"title"`
	// Reason 是忽略时用户写的理由（可空）：展示在「已忽略」里，有当儿的也会
	// 作为反馈喂进下次「AI 对一遍」，让模型知道这个节点不想再收这条。
	Reason   string `json:"reason,omitempty"`
	ReasonAt string `json:"reasonAt,omitempty"`
}

// LibraryAiRejection 是被拒绝的一条 AI 建议：键为 (key, action, targetNodeId)——
// 同一键只能有一个（再拒绝一次 = 用新理由覆盖）。拒绝不删建议记录本身：
// 展示层按这个键标注「上次被拒」，模型二次判断后换了挂法就自然显示新建议。
type LibraryAiRejection struct {
	Key          string `json:"key"` // source:entryId
	Source       string `json:"source"`
	Title        string `json:"title,omitempty"`
	Action       string `json:"action"` // link | archive | ignore
	TargetNodeID string `json:"targetNodeId,omitempty"`
	TargetTitle  string `json:"targetTitle,omitempty"`
	Reason       string `json:"reason,omitempty"`
	ReasonAt     string `json:"reasonAt,omitempty"`
}

// KomgaJudgments 是 LLM 对 Komga 系列层级的判定缓存（扫描时刷新）：
// series=分卷作品（合并成系列一条、跳系列页）；drawer=独立单册抽屉（逐本拆开、跳书页）。
// FP 是判定时的书单指纹——书单没变就不重新判，所以扫描常态仍是零 LLM。
type KomgaJudgments struct {
	GeneratedAt string                   `json:"generatedAt"`
	Model       string                   `json:"model"`
	Items       map[string]KomgaJudgment `json:"items"`
}

type KomgaJudgment struct {
	Kind     string `json:"kind"` // series | drawer
	FP       string `json:"fp"`
	JudgedAt string `json:"judgedAt,omitempty"`
}

type LibrarySource string

const (
	LibraryEmby      LibrarySource = "emby"
	LibraryKomga     LibrarySource = "komga"
	LibraryGameAtlas LibrarySource = "gameatlas"
)

type RunIntent string

const (
	RunIntentCreateWiki     RunIntent = "create_wiki"
	RunIntentContinueWiki   RunIntent = "continue_wiki"
	RunIntentRewriteSection RunIntent = "rewrite_section"
	RunIntentWriteDoc       RunIntent = "write_doc"
	RunIntentOrganizeTree   RunIntent = "organize_tree"
	RunIntentAnswer         RunIntent = "answer"
)

type RunStatus string

const (
	RunStatusRunning     RunStatus = "running"
	RunStatusInterrupted RunStatus = "interrupted"
	RunStatusCompleted   RunStatus = "completed"
	RunStatusFailed      RunStatus = "failed"
	RunStatusExpired     RunStatus = "expired"
)

// Work is a tree node (universe / series / work).
type Work struct {
	ID       string   `json:"id"`
	ParentID *string  `json:"parentId"`
	Kind     WorkKind `json:"kind"`
	Medium   *Medium  `json:"medium"`
	Title    string   `json:"title"`
	Aliases  []string `json:"aliases"`
	// Tags 是自由标签（年份/类型/平台…），与 medium（枚举）互补；
	// 检索走精确包含，不做分面索引。
	Tags       []string   `json:"tags"`
	ContentMd  *string    `json:"contentMd"`
	ContentVer int64      `json:"contentVer"`
	Status     WorkStatus `json:"status"`
	// Visibility: public | private；访客只有"自身与祖先都 public"的节点可见
	Visibility Visibility `json:"visibility"`
	SortOrder  int64      `json:"sortOrder"`
	CreatedAt  string     `json:"createdAt"`
	UpdatedAt  string     `json:"updatedAt"`
}

// WorkSummary is the flat tree row.
type WorkSummary struct {
	ID       string   `json:"id"`
	ParentID *string  `json:"parentId"`
	Kind     WorkKind `json:"kind"`
	Medium   *Medium  `json:"medium"`
	Title    string   `json:"title"`
	// Aliases 是别名（"这部作品还叫什么"）：媒体库条目标题与它精确同名时，
	// 就是"Wiki 已有这个条目"——池子页靠它给出「挂到《X》下」而不是「建档」。
	Aliases        []string   `json:"aliases,omitempty"`
	Status         WorkStatus `json:"status"`
	Visibility     Visibility `json:"visibility"`
	HasContent     bool       `json:"hasContent"`
	HasLibraryLink bool       `json:"hasLibraryLink"`
	SortOrder      int64      `json:"sortOrder"`
	UpdatedAt      string     `json:"updatedAt"`
}

// Doc is a materials document hanging under a work folder.
type Doc struct {
	ID         string   `json:"id"`
	FolderOf   string   `json:"folderOf"`
	Title      string   `json:"title"`
	ContentMd  string   `json:"contentMd"`
	ContentVer int64    `json:"contentVer"`
	Links      []string `json:"links"`
	CreatedAt  string   `json:"createdAt"`
	UpdatedAt  string   `json:"updatedAt"`
}

// Revision stores one content commit for works and docs.
type Revision struct {
	ID         string  `json:"id"`
	TargetType string  `json:"targetType"` // work | doc
	TargetID   string  `json:"targetId"`
	Version    int64   `json:"version"`
	Author     Author  `json:"author"`
	RunID      *string `json:"runId"`
	Summary    string  `json:"summary"`
	CreatedAt  string  `json:"createdAt"`
	ContentMd  string  `json:"contentMd,omitempty"`
}

// Relation is a typed directed edge between works.
type Relation struct {
	ID        string       `json:"id"`
	FromID    string       `json:"fromId"`
	ToID      string       `json:"toId"`
	Type      RelationType `json:"type"`
	CreatedAt string       `json:"createdAt"`
}

// LibraryLink points at Emby / Komga / GameAtlas.
// 库外链的角色：work = 正文/正片级（条目本身是一部作品：游戏本体/正片影视/漫画小说版）；
// mix = 混杂内容级（OST、剧情解析、预告片这类围绕作品的内容）。
// 来源上 GA / Komga 库里天然是作品本体；Emby 由媒体库角色（正片/混杂）决定，见
// httpapi.roleForEntry。
const (
	LinkRoleWork = "work"
	LinkRoleMix  = "mix"
)

// 挂链出处：这条链是谁建的（审计与展示用）。
const (
	LinkBySweep   = "sweep"   // 节点扫库建议，用户确认
	LinkByPool    = "pool"    // 「AI 对一遍」建议，用户确认
	LinkByManual  = "manual"  // 手动搜索关联
	LinkByArchive = "archive" // 建档时自动挂上
	LinkByLegacy  = "legacy"  // 引入角色列之前的历史链
)

type LibraryLink struct {
	ID         string        `json:"id"`
	WorkID     string        `json:"workId"`
	Source     LibrarySource `json:"source"`
	ExternalID string        `json:"externalId"`
	URL        *string       `json:"url"`
	TitleHint  *string       `json:"titleHint"`
	// Role 是这条链的角色（work=正片级 / mix=混杂级）；见文件内 LinkRole* 常量。
	Role string `json:"role"`
	// LinkedBy 标记挂链出处（sweep/pool/manual/archive/legacy）。
	LinkedBy string `json:"linkedBy,omitempty"`
	// CoverImage 是展示用字段：读接口从扫描清单补上（不落库）；清单里没有就没有。
	CoverImage string `json:"coverImage,omitempty"`
	CreatedAt  string `json:"createdAt"`
}

// Run is a librarian work order.
type Run struct {
	ID         string         `json:"id"`
	Workspace  string         `json:"workspace"`
	Intent     RunIntent      `json:"intent"`
	Goal       string         `json:"goal"`
	Status     RunStatus      `json:"status"`
	Checkpoint map[string]any `json:"checkpoint,omitempty"`
	ToolCache  map[string]any `json:"toolCache,omitempty"`
	Result     map[string]any `json:"result"`
	Error      map[string]any `json:"error"`
	// Context 是从 checkpoint 里投影出来的上下文（workId/docId…），
	// 供前端做跳转与批次卡片；不是新增字段，读的是 checkpoint.context。
	Context     map[string]any `json:"context,omitempty"`
	Model       string         `json:"model"`
	StartedAt   string         `json:"startedAt"`
	LastActive  string         `json:"lastActive"`
	ExpiresAt   *string        `json:"expiresAt"`
	CompletedAt *string        `json:"completedAt"`
}

// RunEvent is one SSE stream record.
type RunEvent struct {
	ID        string         `json:"id"`
	RunID     string         `json:"runId"`
	Seq       int64          `json:"seq"`
	Type      string         `json:"type"`
	Payload   map[string]any `json:"payload"`
	CreatedAt string         `json:"createdAt"`
}

// Session 是一段连续对话。会话 id 就是 runs.workspace；target 是它归属的页面
// （work:<id> / doc:<id> / home / batch:<id>），同一页面可以有多段会话。
type Session struct {
	ID     string `json:"id"`
	Target string `json:"target"`
	Title  string `json:"title"`
	// LastGoal 是该会话最近一句用户消息。列表的副标题用它——
	// 标题只在建会话时定一次，之后问"这场对话最近在聊什么"要靠它。
	LastGoal  string `json:"lastGoal"`
	RunCount  int64  `json:"runCount"`
	Status    string `json:"status"` // 最近一次执行的状态；有 running 的优先
	CreatedAt string `json:"createdAt"`
	UpdatedAt string `json:"updatedAt"`
}

// CreateSessionBody is POST /api/sessions.
type CreateSessionBody struct {
	Target string `json:"target"`
	Title  string `json:"title"`
}

// SearchHit is one FTS result.
type SearchHit struct {
	ID      string `json:"id"`
	Kind    string `json:"kind"` // work | doc
	Title   string `json:"title"`
	Snippet string `json:"snippet"`
}

// Settings is the persisted app configuration.
type Settings struct {
	LLM struct {
		Endpoint         string   `json:"endpoint"`
		Model            string   `json:"model"`
		APIKeyConfigured bool     `json:"apiKeyConfigured"`
		Temperature      *float64 `json:"temperature,omitempty"`
		MaxTokens        *int     `json:"maxTokens,omitempty"`
		// ReasoningEffort 思考等级：none | low | medium | high | xhigh | max（空=走默认 medium）
		ReasoningEffort string `json:"reasoningEffort,omitempty"`
		// Protocol 协议标识。留空 = 默认（responses）。这是"以后要加新协议"的位置：
		// 加协议只动 llm.NewClient 的分派，上层与前端不感知。
		Protocol string `json:"protocol,omitempty"`
		// ContextWindow 当前模型的上下文窗口（token）。只用于界面显示用量占比
		// （面板右下角那圈）；0 表示未知，用 DefaultContextWindow。
		ContextWindow int `json:"contextWindow,omitempty"`
	} `json:"llm"`
	Library struct {
		EmbyURL         *string `json:"embyUrl,omitempty"`
		EmbyAPIKey      *string `json:"embyApiKey,omitempty"`
		KomgaURL        *string `json:"komgaUrl,omitempty"`
		KomgaAPIKey     *string `json:"komgaApiKey,omitempty"`
		GameAtlasURL    *string `json:"gameatlasUrl,omitempty"`
		GameAtlasAPIKey *string `json:"gameatlasApiKey,omitempty"`
		// EmbyLibraryRoles 是用户给 Emby 各媒体库标的角色（正片/混杂内容）。
		EmbyLibraryRoles []EmbyLibraryRole `json:"embyLibraryRoles,omitempty"`
		// ScanIntervalMinutes 是媒体库后台扫描间隔（分钟；nil=默认 360，0=关闭）。
		ScanIntervalMinutes *int `json:"scanIntervalMinutes,omitempty"`
		// 是否已配置（响应里只回标记，不回密钥原文）
		EmbyAPIKeyConfigured      bool `json:"embyApiKeyConfigured"`
		KomgaAPIKeyConfigured     bool `json:"komgaApiKeyConfigured"`
		GameAtlasAPIKeyConfigured bool `json:"gameatlasApiKeyConfigured"`
	} `json:"library"`
	Runs struct {
		ExpireDays        int `json:"expireDays"`
		KeepEventsDays    int `json:"keepEventsDays"`
		MaxConcurrentRuns int `json:"maxConcurrentRuns"`
	} `json:"runs"`
	// Search 联网检索（Exa）。key 单独存 settings 表，接口只回"是否已配置"。
	Search struct {
		ExaAPIKeyConfigured bool `json:"exaApiKeyConfigured"`
		// ProxyURL 出外网的 HTTP 代理（如 http://192.168.1.253:7890）。
		// 只给 fetch_url 用：直连超时时模型会带 useProxy 重试同一页。
		// Exa 检索走它自己的通道，不受这里影响。
		ProxyURL string `json:"proxyUrl,omitempty"`
	} `json:"search"`
	// Admin 简易用户系统（单管理员）。密码只存哈希，接口只回是否已设置。
	Admin struct {
		Username           string `json:"username"`
		PasswordConfigured bool   `json:"passwordConfigured"`
		// UsingDefaultPassword 仍是出厂密码 1234（设置页提醒改掉）
		UsingDefaultPassword bool `json:"usingDefaultPassword"`
		// NewNodeVisibility 新节点默认可见性（public|private）
		NewNodeVisibility Visibility `json:"newNodeVisibility"`
	} `json:"admin"`
	SkillRoot string `json:"skillRoot"`
}

// DefaultSettings returns sane defaults.
func DefaultSettings() Settings {
	var s Settings
	s.LLM.Endpoint = "https://api.openai.com/v1"
	s.LLM.Model = "gpt-4o-mini"
	s.LLM.APIKeyConfigured = false
	s.Runs.ExpireDays = 7
	s.Runs.KeepEventsDays = 90
	s.Runs.MaxConcurrentRuns = 2
	s.Admin.Username = "admin"
	s.Admin.NewNodeVisibility = VisibilityPrivate
	s.SkillRoot = "/root/WikiAltas/skills/wiki-writing"
	return s
}

// CreateWorkBody is POST /api/works.
type CreateWorkBody struct {
	ParentID *string   `json:"parentId"`
	Kind     WorkKind  `json:"kind"`
	Medium   *Medium   `json:"medium"`
	Title    string    `json:"title"`
	Aliases  *[]string `json:"aliases"`
	Tags     *[]string `json:"tags"`
	// Visibility 省略时用设置页的"新节点默认可见性"
	Visibility *Visibility `json:"visibility"`
}

// CreateWorkBatchItem 是 upsert_works 批量建档的单项。
// 纪律：本批不制造父节点——ParentID 必须指向已存在的节点。
type CreateWorkBatchItem struct {
	Kind     WorkKind  `json:"kind"`
	Title    string    `json:"title"`
	ParentID *string   `json:"parentId"`
	Medium   *Medium   `json:"medium"`
	Aliases  *[]string `json:"aliases"`
	Tags     *[]string `json:"tags"`
}

// PatchWorkBody is PATCH /api/works/:id.
type PatchWorkBody struct {
	Title      *string     `json:"title"`
	ParentID   *string     `json:"parentId"`
	Medium     *Medium     `json:"medium"`
	Status     *WorkStatus `json:"status"`
	Visibility *Visibility `json:"visibility"`
	SortOrder  *int64      `json:"sortOrder"`
	Aliases    *[]string   `json:"aliases"`
	Tags       *[]string   `json:"tags"`
}

// PutContentBody is PUT content endpoints.
type PutContentBody struct {
	ContentMd       string  `json:"contentMd"`
	Author          Author  `json:"author"`
	RunID           *string `json:"runId"`
	Summary         *string `json:"summary"`
	ExpectedVersion *int64  `json:"expectedVersion"`
}

// CreateDocBody is POST /api/works/:id/docs.
type CreateDocBody struct {
	Title     string   `json:"title"`
	ContentMd *string  `json:"contentMd"`
	Links     []string `json:"links"`
}

// PatchDocBody is PATCH /api/docs/:id.
type PatchDocBody struct {
	Title *string   `json:"title"`
	Links *[]string `json:"links"`
}

// CreateRelationBody is POST /api/relations.
type CreateRelationBody struct {
	FromID string       `json:"fromId"`
	ToID   string       `json:"toId"`
	Type   RelationType `json:"type"`
}

// CreateRunBody is POST /api/runs.
type CreateRunBody struct {
	Intent RunIntent `json:"intent"`
	Goal   string    `json:"goal"`
	// Workspace 是"会话键"（work:<id> / home / batch:<id>）。
	// 面板刷新后按它召回同一段对话；留空按 default 处理。列已存在于 runs 表。
	Workspace string      `json:"workspace"`
	Context   *RunContext `json:"context"`
	// ReasoningEffort 思考等级覆盖（none|low|medium|high|xhigh|max）：聊天框里选的档位，
	// 留空用设置页的值。只影响本条工单。
	ReasoningEffort string `json:"reasoningEffort"`
}

// RunContext 是一次工单要绑定的上下文（作品/资料/父节点/介质/章节）。
type RunContext struct {
	WorkID   *string `json:"workId"`
	DocID    *string `json:"docId"`
	ParentID *string `json:"parentId"`
	Medium   *Medium `json:"medium"`
	Section  *string `json:"section"`
	// DocMode 是发起工单时用户所处的文档模式：read | edit | revision。
	// 它决定 Altas 的作业方式（只读=只分析不改、编辑=直接改、修订=定点改+给理由）。
	DocMode *string `json:"docMode"`
	// Selection 是用户在正文里选中的文本（修订模式的作用域，对齐飞书「选定内容」）。
	Selection *string `json:"selection"`
	// OneShot 是"一次性入口"的能力契约（工具栏翻译/解释按钮）：只输出、不落盘。
	// 运行时据此剥写工具——与 DocMode=read 同层：用户显式选择，不是对消息的猜测。
	OneShot *bool `json:"oneShot"`
	// Mentioned 是 @ 引用的条目（"标题(type:id)、…"），执行器拼进 user turn。
	// 注意：前端曾用扁平 key 发它，被结构体解码静默丢弃（executor 读了却永远为空）。
	Mentioned *string `json:"mentioned"`
	// Page 是用户发起工单时所在的页面（home|work|folder|doc|runs|library|settings）。
	// 执行器据此在工单里写出"## 当前页面"，并按页面装配简报（见 run/pageBrief.go）。
	//
	// **context 字段是五处，缺一处就静默丢**（AGENTS.md 说的"三件套"是前三处）：
	//   ① 前端 `types.ts` 的 CreateRunBody.context
	//   ② 前端 AiPanel 构造（用 NonNullable<CreateRunBody['context']>，别铺平 Record）
	//   ③ 这里（domain.RunContext）
	//   ④ `run/manager.go` 的 ctxMap 映射
	//   ⑤ `run/executor.go` 的 buildUserTurn（模型要看见才需要）
	// 活标本：这个字段一度只做了 ①②，Go 解码不认识的 key 不报错——于是它一个字节
	// 都没到后端（和当年 "@ 引用" 坏掉同一个死法）。
	Page  *string        `json:"page"`
	Extra map[string]any `json:"extra"`
}

// CreateBatchBody is POST /api/runs/batch —— 批量建档（Emby/Komga/GameAtlas
// 扫出来的一堆 stub，一键让 Altas 逐个写）。
// 不新建表：一个批次 = 一个共享 workspace（batch:<id>）+ N 个子工单。
type CreateBatchBody struct {
	WorkIDs   []string `json:"workIds"`
	BatchSize int      `json:"batchSize"`
	Medium    *Medium  `json:"medium"`
	// GoalTemplate 可选，缺省用「写《{title}》的 Wiki」。
	GoalTemplate string `json:"goalTemplate"`
}

// BatchResult 返回批次 id 与实际创建的工单。
type BatchResult struct {
	BatchID    string   `json:"batchId"`
	Workspace  string   `json:"workspace"`
	RunIDs     []string `json:"runIds"`
	SkippedIDs []string `json:"skippedIds"`
}

// ContentCommitResult is returned by PUT content.
type ContentCommitResult struct {
	ID         string `json:"id"`
	ContentVer int64  `json:"contentVer"`
	RevisionID string `json:"revisionId"`
}

// WorkDetail is GET /api/works/:id response.
type WorkDetail struct {
	Work           Work          `json:"work"`
	LatestRevision *Revision     `json:"latestRevision,omitempty"`
	LibraryLinks   []LibraryLink `json:"libraryLinks"`
	Relations      []Relation    `json:"relations"`
}

// ListWorksFilter 是 GET /api/list-works 与 audit/list_works 工具共用的筛选条件。
// 与 Search（全文）互补：它按结构字段筛，不走 FTS。
type ListWorksFilter struct {
	Kind       string `json:"kind"`
	Medium     string `json:"medium"`
	Status     string `json:"status"`
	HasContent *bool  `json:"hasContent"`
	// Limit 缺省 100，上限 200。
	Limit int `json:"limit"`
}

// AuditFinding 是 audit 工具的一条发现：定位（id/title）+ 一句人话原因。
type AuditFinding struct {
	ID     string `json:"id"`
	Title  string `json:"title"`
	Kind   string `json:"kind"`
	Status string `json:"status"`
	Detail string `json:"detail"`
}

// AuditReport 是确定性体检的产物（零 LLM）：只报不修。
// 各类发现单独限长，总数超限只报计数——审计结果本身不能撑爆上下文。
type AuditReport struct {
	Total      int            `json:"total"`
	Archived   int            `json:"archived"`
	Relations  int            `json:"relations"`
	StubsEmpty int            `json:"stubsEmpty"`
	DriftStub  []AuditFinding `json:"driftStub"`
	DriftReady []AuditFinding `json:"driftReady"`
	Misplaced  []AuditFinding `json:"misplaced"`
	Duplicates []AuditFinding `json:"duplicates"`
	NoMedium   []AuditFinding `json:"noMedium"`
}
