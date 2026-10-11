package domain

import "testing"

func testNodes() []WorkSummary {
	pid := "s-ff"
	up := "u-root"
	return []WorkSummary{
		{ID: "u-root", Kind: WorkKindUniverse, Title: "最终幻想"},
		{ID: "s-ff", ParentID: &up, Kind: WorkKindSeries, Title: "最终幻想"},
		{ID: "w-ff15", ParentID: &pid, Kind: WorkKindWork, Title: "最终幻想15"},
		{ID: "w-ff15a", ParentID: &pid, Kind: WorkKindWork, Title: "最终幻想15", Aliases: []string{"FF15"}},
		{ID: "w-lonely", Kind: WorkKindWork, Title: "龙与虎"},
	}
}

// 命中单作 → 直接挂（不建节点）。
func TestPlanLinksToExistingWork(t *testing.T) {
	p := NewPlacer(testNodes())
	got := p.Plan("龙与虎", MediumTV, "")
	if got.Action != PlaceLink || got.TargetNodeID != "w-lonely" {
		t.Fatalf("命中单作应直接挂, got %+v", got)
	}
	// 别名也算命中
	if got := p.Plan("FF15", MediumTV, ""); got.Action != PlaceLink || got.TargetNodeID != "w-ff15a" {
		t.Fatalf("别名应命中, got %+v", got)
	}
}

// 命中系列 → 在它下面建党名子节点（与系列同名 → 加介质后缀）。
func TestPlanChildUnderSeries(t *testing.T) {
	p := NewPlacer(testNodes())
	got := p.Plan("16bit的感动", MediumTV, "")
	if got.Action != PlaceSeriesChild || got.SeriesTitle != "16bit的感动" {
		t.Fatalf("没命中应「建系列+建子」, got %+v", got)
	}
	if got.NodeTitle != "16bit的感动（剧集）" {
		t.Fatalf("与系列同名应加介质后缀, got %q", got.NodeTitle)
	}

	// 同名的第二种介质：复用刚建的系列（Register 回填后）→ 另一个子节点
	seriesID := p.NewTempID()
	childID := p.NewTempID()
	p.Register(WorkSummary{ID: seriesID, Kind: WorkKindSeries, Title: got.SeriesTitle})
	p.Register(WorkSummary{ID: childID, ParentID: &seriesID, Kind: WorkKindWork, Title: got.NodeTitle})

	got2 := p.Plan("16bit的感动", MediumManga, "")
	if got2.Action != PlaceChild || got2.ParentNodeID != seriesID {
		t.Fatalf("第二种介质应落在同一个系列下, got %+v", got2)
	}
	if got2.NodeTitle != "16bit的感动（漫画）" {
		t.Fatalf("got %q", got2.NodeTitle)
	}
}

// 命中系列下的已有子节点 → 直接挂（幂等：重跑只补链）。
func TestPlanReusesChild(t *testing.T) {
	nodes := testNodes()
	pid := "s-ff"
	nodes = append(nodes, WorkSummary{ID: "w-ff16", ParentID: &pid, Kind: WorkKindWork, Title: "最终幻想16"})
	p := NewPlacer(nodes)
	got := p.Plan("最终幻想16", MediumGame, "")
	if got.Action != PlaceLink || got.TargetNodeID != "w-ff16" {
		t.Fatalf("系列下已有同名子节点应直接挂, got %+v", got)
	}
}

// 不同作品同属一个系列容器（丧尸围城2/3/4）→ 子标题不撞 → 不加后缀。
func TestPlanNoSuffixWhenDistinct(t *testing.T) {
	series := "s-dc"
	root := "u-dc"
	p := NewPlacer([]WorkSummary{
		{ID: root, Kind: WorkKindUniverse, Title: "丧尸围城"},
		{ID: series, ParentID: &root, Kind: WorkKindSeries, Title: "丧尸围城"},
	})
	got := p.Plan("丧尸围城2", MediumGame, series)
	if got.Action != PlaceChild || got.NodeTitle != "丧尸围城2" {
		t.Fatalf("不同名不加后缀, got %+v", got)
	}
}

// 同名但**已存在同名子节点**（那个节点就是这条作品的页面）→ 复用它、直接挂。
// （这是"命中 work"在"指定了父节点"下的同一条规则：不重复建节点。）
func TestPlanReusesSameNameSibling(t *testing.T) {
	series := "s-x"
	p := NewPlacer([]WorkSummary{
		{ID: series, Kind: WorkKindSeries, Title: "某系列"},
		{ID: "w-a", ParentID: &series, Kind: WorkKindWork, Title: "16bit的感动"},
	})
	got := p.Plan("16bit的感动", MediumManga, series)
	if got.Action != PlaceLink || got.TargetNodeID != "w-a" {
		t.Fatalf("同名子节点应复用, got %+v", got)
	}
}

// 系列名 = 作品名（《16bit的感动》），先落了剧集 → 后来的漫画要另起一个带后缀的子节点。
func TestPlanSecondMediumGetsItsOwnChild(t *testing.T) {
	series := "s-16bit"
	p := NewPlacer([]WorkSummary{
		{ID: series, Kind: WorkKindSeries, Title: "16bit的感动"},
		{ID: "w-tv", ParentID: &series, Kind: WorkKindWork, Title: "16bit的感动（剧集）"},
	})
	got := p.Plan("16bit的感动", MediumManga, "")
	if got.Action != PlaceChild || got.ParentNodeID != series {
		t.Fatalf("应落在已有系列下, got %+v", got)
	}
	if got.NodeTitle != "16bit的感动（漫画）" {
		t.Fatalf("got %q", got.NodeTitle)
	}
}

// 顶级（不归属）：没指定父节点、也没命中 → 建系列 + 子节点（先不落库的那种由调用方决定）。
func TestPlanSeriesChildMediumLabels(t *testing.T) {
	cases := []struct {
		medium Medium
		want   string
	}{
		{MediumGame, "名（游戏）"},
		{MediumMovie, "名（电影）"},
		{MediumTV, "名（剧集）"},
		{MediumManga, "名（漫画）"},
		{MediumBook, "名（书籍）"},
	}
	for _, c := range cases {
		p := NewPlacer(nil)
		got := p.Plan("名", c.medium, "")
		if got.NodeTitle != c.want {
			t.Fatalf("介质 %v 应得 %q, got %q", c.medium, c.want, got.NodeTitle)
		}
	}
}

func TestMediumOfEntry(t *testing.T) {
	cases := []struct {
		source, kind, format string
		want                 Medium
	}{
		{"gameatlas", "game", "", MediumGame},
		{"komga", "book", "comic", MediumManga},
		{"komga", "book", "novel", MediumBook},
		{"emby", "movie", "", MediumMovie},
		{"emby", "tv", "", MediumTV},
		{"emby", "collection", "", MediumTV},
	}
	for _, c := range cases {
		if got := MediumOfEntry(c.source, c.kind, c.format); got != c.want {
			t.Fatalf("%s/%s/%s → %v, want %v", c.source, c.kind, c.format, got, c.want)
		}
	}
}
