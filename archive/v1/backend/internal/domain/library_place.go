package domain

import "strings"

// --- 建档落位：条目该落到哪个节点 ---
//
// 这页的主线是"从库里挑、建为待写"，于是有一个反复出现的问题：**同名不同来源**
// 的条目（Komga 的《16bit的感动》漫画 + Emby 的《16bit的感动》番剧）如果各自建档，
// 就会造出两个同名节点。正确的结构是：
//
//	16bit的感动            (series)
//	├─ 16bit的感动（剧集）  ← Emby 条目
//	└─ 16bit的感动（漫画）  ← Komga 条目
//
// 规则只有一条，按顺序判（见 Placer.Plan）。它必须在后端：前端没有测试基建，
// 而这条规则会**建节点**——错一次是一批错节点。

// 落位动作。
const (
	PlaceLink        = "link"         // 直接挂到已有节点
	PlaceChild       = "child"        // 在指定父节点下建/复用子节点，再挂上去
	PlaceSeriesChild = "series-child" // 先确保一个同名系列存在，再在其下建/复用子节点
)

// PlacementPlan 是"这条库条目该落到哪儿"的答案，由调用方执行（建节点 / 挂链）。
type PlacementPlan struct {
	Action string
	// TargetNodeID 非空（Action=link）= 直接挂到这个节点。
	TargetNodeID string
	// SeriesTitle 非空（Action=series-child）= 先确保这个标题的 series 节点存在。
	SeriesTitle string
	// ParentNodeID（Action=child）= 在哪个节点下建子节点。
	ParentNodeID string
	// NodeTitle（Action=child / series-child）= 要建/复用的子节点标题。
	NodeTitle string
	Medium    Medium
}

// Placer 逐条给出落位方案，并记住"本次已复用/新建过的节点"，
// 所以同一批里的同名条目会共用系列与子节点（重复点同一个容器只补链，不重复建）。
type Placer struct {
	byTitle  map[string]*WorkSummary   // 规范化标题（含别名）→ 节点
	children map[string][]*WorkSummary // parentID（"" = 顶级）→ 子节点
	titleOf  map[string]string         // nodeID → 标题
	tempN    int
}

// NewPlacer 用当前节点树建索引。
func NewPlacer(nodes []WorkSummary) *Placer {
	p := &Placer{
		byTitle:  map[string]*WorkSummary{},
		children: map[string][]*WorkSummary{},
		titleOf:  map[string]string{},
	}
	for i := range nodes {
		p.Register(nodes[i])
	}
	return p
}

// Register 登记一个节点（既用于初始化，也用于登记"刚建好的"——临时 id 即可）。
func (p *Placer) Register(n WorkSummary) {
	node := n
	for _, t := range append([]string{n.Title}, n.Aliases...) {
		k := normTitle(t)
		if k == "" {
			continue
		}
		cur := p.byTitle[k]
		if cur == nil || kindRank(n.Kind) < kindRank(cur.Kind) {
			p.byTitle[k] = &node
		}
	}
	parent := ""
	if n.ParentID != nil {
		parent = *n.ParentID
	}
	p.children[parent] = append(p.children[parent], &node)
	p.titleOf[n.ID] = n.Title
}

// Plan 给出一条条目的落位方案。
//
//	命中单作          → 直接挂上去（同一作品的媒体各挂正片位，不建新节点）
//	命中系列/宇宙/合集 → 在它下面建/复用一个子节点，再挂上去
//	什么都没命中       → 建同名系列 + 建子节点，再挂上去
//	调用方指定了父节点  → 在它下面建/复用一个子节点（跳过"建系列"）
func (p *Placer) Plan(entryTitle string, medium Medium, explicitParentID string) PlacementPlan {
	if explicitParentID != "" {
		if child := p.reuseChild(explicitParentID, entryTitle, medium); child != nil {
			return PlacementPlan{Action: PlaceLink, TargetNodeID: child.ID}
		}
		return PlacementPlan{
			Action:       PlaceChild,
			ParentNodeID: explicitParentID,
			NodeTitle:    p.childTitle(explicitParentID, entryTitle, medium),
			Medium:       medium,
		}
	}
	if hit := p.byTitle[normTitle(entryTitle)]; hit != nil {
		if hit.Kind == WorkKindWork {
			return PlacementPlan{Action: PlaceLink, TargetNodeID: hit.ID}
		}
		if child := p.reuseChild(hit.ID, entryTitle, medium); child != nil {
			return PlacementPlan{Action: PlaceLink, TargetNodeID: child.ID}
		}
		return PlacementPlan{
			Action:       PlaceChild,
			ParentNodeID: hit.ID,
			NodeTitle:    p.childTitle(hit.ID, entryTitle, medium),
			Medium:       medium,
		}
	}
	// 系列还不存在：建系列（标题 = 条目名），系列下再建子节点。
	// 子节点必然与这个**待建的**系列同名，所以直接带介质后缀（《X》→《X（剧集）》）——
	// 这里不能去查撞名：系列还没建、也无从查父节点。
	seriesTitle := strings.TrimSpace(entryTitle)
	return PlacementPlan{
		Action:      PlaceSeriesChild,
		SeriesTitle: seriesTitle,
		NodeTitle:   seriesTitle + "（" + MediumLabel(medium) + "）",
		Medium:      medium,
	}
}

// PlanTop 是「不归属」：不建系列，直接在**顶级**建/复用一个节点。
// （批量建 88 条互不相关的番剧时，硬塞一个父节点是错的——它们本来就还没有归属。）
func (p *Placer) PlanTop(entryTitle string, medium Medium) PlacementPlan {
	if hit := p.byTitle[normTitle(entryTitle)]; hit != nil && hit.Kind == WorkKindWork {
		return PlacementPlan{Action: PlaceLink, TargetNodeID: hit.ID}
	}
	if child := p.reuseChild("", entryTitle, medium); child != nil {
		return PlacementPlan{Action: PlaceLink, TargetNodeID: child.ID}
	}
	return PlacementPlan{
		Action:    PlaceChild,
		NodeTitle: p.childTitle("", entryTitle, medium),
		Medium:    medium,
	}
}

// NewTempID 给"还没落库的新节点"发一个临时 id（用于同一批内复用）。
func (p *Placer) NewTempID() string {
	p.tempN++
	return "\x00new-" + itoa(p.tempN)
}

// reuseChild 在该父节点下找"这条条目该落到的子节点"：标题或别名等于
// **条目名**，或等于**带介质后缀的条目名**（也含本次刚建的）。
//
// 两条都要：前者是"系列下已经有这个作品的页面了"；后者是**幂等**的关键——
// 重跑同一个条目、或同一作品同介质的第二条（番剧的剧集版 + 电影版都叫一个名字），
// 都该复用同一个《X（剧集）》，而不是造出《X（剧集） 2》。
func (p *Placer) reuseChild(parentID, title string, medium Medium) *WorkSummary {
	want := map[string]bool{
		normTitle(title): true,
		normTitle(title + "（" + MediumLabel(medium) + "）"): true,
	}
	for _, c := range p.children[parentID] {
		if want[normTitle(c.Title)] {
			return c
		}
		for _, a := range c.Aliases {
			if want[normTitle(a)] {
				return c
			}
		}
	}
	return nil
}

// childTitle 决定子节点标题：默认就用条目名；**只有**与父节点同名、或与已有兄弟撞名时
// 才加介质后缀（《16bit的感动》落在同名系列下 → 《16bit的感动（剧集）》）。
// 后缀也撞（同介质同名）时加序号兜底。
func (p *Placer) childTitle(parentID, title string, medium Medium) string {
	title = strings.TrimSpace(title)
	if !p.taken(parentID, title) && !p.sameTitle(parentID, title) {
		return title
	}
	base := title + "（" + MediumLabel(medium) + "）"
	if !p.taken(parentID, base) {
		return base
	}
	for i := 2; ; i++ {
		cand := base + " " + itoa(i)
		if !p.taken(parentID, cand) {
			return cand
		}
	}
}

// taken 该父节点下是否已有同名兄弟。
func (p *Placer) taken(parentID, title string) bool {
	k := normTitle(title)
	for _, c := range p.children[parentID] {
		if normTitle(c.Title) == k {
			return true
		}
	}
	return false
}

// sameTitle 父节点自己是不是就叫这个名字（系列与子节点不能同名）。
func (p *Placer) sameTitle(parentID, title string) bool {
	t, ok := p.titleOf[parentID]
	return ok && normTitle(t) == normTitle(title)
}

// MatchNodeByPlacer 按规范化标题（含别名）找一个节点；命中多个时取"更像容器"的那个
// （universe > series > work）。池子页与建档落位共用这一条口径。
func MatchNodeByPlacer(nodes []WorkSummary, title string) *WorkSummary {
	k := normTitle(title)
	if k == "" {
		return nil
	}
	var best *WorkSummary
	for i := range nodes {
		n := &nodes[i]
		if normTitle(n.Title) != k && !hasAlias(n, k) {
			continue
		}
		if best == nil || kindRank(n.Kind) < kindRank(best.Kind) {
			best = n
		}
	}
	return best
}

func hasAlias(n *WorkSummary, k string) bool {
	for _, a := range n.Aliases {
		if normTitle(a) == k {
			return true
		}
	}
	return false
}

// MediumLabel 是介质的显示词（与前端 MEDIUM_LABEL 同一套词）。
func MediumLabel(m Medium) string {
	switch m {
	case MediumGame:
		return "游戏"
	case MediumMovie:
		return "电影"
	case MediumTV:
		return "剧集"
	case MediumManga:
		return "漫画"
	case MediumBook:
		return "书籍"
	}
	return "其他"
}

// MediumOfEntry 由清单条目的 source/kind/format 推介质。
// （Emby 的 tv 一律算"剧集"；要显示"动画"是改这一行的事。）
func MediumOfEntry(source, kind, format string) Medium {
	switch strings.TrimSpace(source) {
	case "gameatlas":
		return MediumGame
	case "komga":
		if strings.TrimSpace(format) == "comic" || strings.TrimSpace(kind) == "comic" {
			return MediumManga
		}
		if strings.TrimSpace(format) == "novel" {
			return MediumBook
		}
		return MediumManga
	case "emby":
		if strings.TrimSpace(kind) == "movie" {
			return MediumMovie
		}
		return MediumTV
	}
	return MediumOther
}

// normTitle 规范化：去全部空白（含全角空格）、全角字母数字转半角、大小写归一。
func normTitle(s string) string {
	var b strings.Builder
	for _, r := range s {
		switch {
		case r == ' ' || r == '\t' || r == '\n' || r == '\u3000':
			continue
		case r >= 'Ａ' && r <= 'Ｚ', r >= 'ａ' && r <= 'ｚ', r >= '０' && r <= '９':
			b.WriteRune(r - 0xfee0)
		default:
			b.WriteRune(r)
		}
	}
	return strings.ToLower(b.String())
}

func kindRank(k WorkKind) int {
	switch k {
	case WorkKindUniverse:
		return 0
	case WorkKindSeries:
		return 1
	}
	return 2
}

func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	var buf [12]byte
	i := len(buf)
	for n > 0 {
		i--
		buf[i] = byte('0' + n%10)
		n /= 10
	}
	return string(buf[i:])
}
