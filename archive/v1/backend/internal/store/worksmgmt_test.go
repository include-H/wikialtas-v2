package store

import (
	"strings"
	"testing"

	"wikiatlas/backend/internal/domain"
)

func mustWork(t *testing.T, s *Store, title string, parent *string, kind domain.WorkKind) domain.Work {
	t.Helper()
	b := domain.CreateWorkBody{Kind: kind, Title: title}
	if parent != nil {
		b.ParentID = parent
	}
	w, err := s.CreateWork(b)
	if err != nil {
		t.Fatalf("CreateWork %s: %v", title, err)
	}
	return *w
}

// 归档：status 枚举扩展 + 树/检索排除 + list 可查 + 可恢复。
func TestArchivedLifecycle(t *testing.T) {
	s := newTestStore(t)
	u := mustWork(t, s, "宇宙A", nil, domain.WorkKindUniverse)
	w := mustWork(t, s, "单作A", &u.ID, domain.WorkKindWork)

	st := domain.WorkStatusArchived
	if _, err := s.PatchWork(w.ID, domain.PatchWorkBody{Status: &st}); err != nil {
		t.Fatalf("归档: %v", err)
	}
	got, _ := s.GetWork(w.ID)
	if got.Status != domain.WorkStatusArchived {
		t.Fatalf("status = %s", got.Status)
	}

	// 树里消失
	for _, n := range mustListTree(t, s) {
		if n.ID == w.ID {
			t.Fatal("已归档节点仍出现在 ListTree")
		}
	}
	// 检索里消失
	if hits, err := s.Search("单作A", "", 10); err != nil {
		t.Fatal(err)
	} else if len(hits) != 0 {
		t.Fatalf("已归档节点仍可被搜索到: %v", hits)
	}
	// list 显式可查
	works, err := s.ListWorks(domain.ListWorksFilter{Status: "archived"})
	if err != nil {
		t.Fatal(err)
	}
	if len(works) != 1 || works[0].ID != w.ID {
		t.Fatalf("ListWorks(status=archived) = %v", works)
	}
	// 默认筛选排除归档
	all, err := s.ListWorks(domain.ListWorksFilter{})
	if err != nil {
		t.Fatal(err)
	}
	for _, n := range all {
		if n.Status == domain.WorkStatusArchived {
			t.Fatal("默认 ListWorks 不应返回归档节点")
		}
	}
	// 恢复
	draft := domain.WorkStatusDraft
	if _, err := s.PatchWork(w.ID, domain.PatchWorkBody{Status: &draft}); err != nil {
		t.Fatal(err)
	}
	tree2 := mustListTree(t, s)
	found := false
	for _, n := range tree2 {
		if n.ID == w.ID {
			found = true
		}
	}
	if !found {
		t.Fatal("恢复后节点未回到树")
	}
	// 非法状态仍拒绝
	bad := domain.WorkStatus("deleted")
	if _, err := s.PatchWork(w.ID, domain.PatchWorkBody{Status: &bad}); err == nil {
		t.Fatal("非法 status 应被拒绝")
	}
}

func mustListTree(t *testing.T, s *Store) []domain.WorkSummary {
	t.Helper()
	out, err := s.ListTree()
	if err != nil {
		t.Fatal(err)
	}
	return out
}

// tags/aliases：建与改都能落，读出形状稳定（nil ≠ null 不在这儿验，工具层会兜）。
func TestTagsRoundtrip(t *testing.T) {
	s := newTestStore(t)
	id := mustWork(t, s, "带标签", nil, domain.WorkKindUniverse).ID
	tags := []string{"1998", "SRPG"}
	if _, err := s.PatchWork(id, domain.PatchWorkBody{Tags: &tags}); err != nil {
		t.Fatal(err)
	}
	w, _ := s.GetWork(id)
	if len(w.Tags) != 2 || w.Tags[0] != "1998" {
		t.Fatalf("tags = %v", w.Tags)
	}
	// 整份替换
	tags2 := []string{"御三家"}
	if _, err := s.PatchWork(id, domain.PatchWorkBody{Tags: &tags2}); err != nil {
		t.Fatal(err)
	}
	w, _ = s.GetWork(id)
	if len(w.Tags) != 1 || w.Tags[0] != "御三家" {
		t.Fatalf("替换后 tags = %v", w.Tags)
	}
	// create 路径
	alias := []string{"FF7R"}
	b := domain.CreateWorkBody{Kind: domain.WorkKindWork, Title: "克劳德精选", Aliases: &alias}
	ww, err := s.CreateWork(b)
	if err != nil {
		t.Fatal(err)
	}
	if len(ww.Aliases) != 1 || ww.Aliases[0] != "FF7R" {
		t.Fatalf("create aliases = %v", ww.Aliases)
	}
	if ww.Tags == nil || len(ww.Tags) != 0 {
		t.Fatalf("新节点 tags 应为空切片: %v", ww.Tags)
	}
}

// ListWorks 各筛选项叠加。
func TestListWorksFilters(t *testing.T) {
	s := newTestStore(t)
	u := mustWork(t, s, "U1", nil, domain.WorkKindUniverse)
	sr := mustWork(t, s, "S1", &u.ID, domain.WorkKindSeries)
	w1 := mustWork(t, s, "W-游戏", &sr.ID, domain.WorkKindWork)
	m := domain.MediumGame
	if _, err := s.PatchWork(w1.ID, domain.PatchWorkBody{Medium: &m}); err != nil {
		t.Fatal(err)
	}
	big := strings.Repeat("正文内容", 200)
	if _, err := s.PutWorkContent(w1.ID, domain.PutContentBody{ContentMd: big, Author: domain.AuthorHuman}); err != nil {
		t.Fatal(err)
	}
	_ = mustWork(t, s, "W-空", &sr.ID, domain.WorkKindWork)

	if got, _ := s.ListWorks(domain.ListWorksFilter{Kind: "work"}); len(got) != 2 {
		t.Fatalf("kind=work → %d", len(got))
	}
	if got, _ := s.ListWorks(domain.ListWorksFilter{Medium: "game"}); len(got) != 1 || got[0].ID != w1.ID {
		t.Fatalf("medium=game → %v", got)
	}
	if got, _ := s.ListWorks(domain.ListWorksFilter{Kind: "work", HasContent: boolPtr(true)}); len(got) != 1 || got[0].ID != w1.ID {
		t.Fatalf("hasContent=true → %v", got)
	}
	if got, _ := s.ListWorks(domain.ListWorksFilter{Kind: "work", HasContent: boolPtr(false)}); len(got) != 1 || got[0].Status != domain.WorkStatusStub {
		t.Fatalf("hasContent=false → %v", got)
	}
	// limit 生效
	_ = mustWork(t, s, "W3", &sr.ID, domain.WorkKindWork)
	if got, _ := s.ListWorks(domain.ListWorksFilter{Kind: "work", Limit: 2}); len(got) != 2 {
		t.Fatalf("limit=2 → %d", len(got))
	}
}

func boolPtr(b bool) *bool { return &b }

// DeleteDoc：删得掉、级联清修订、找不回。
func TestDeleteDoc(t *testing.T) {
	s := newTestStore(t)
	u := mustWork(t, s, "U2", nil, domain.WorkKindUniverse)
	doc, err := s.CreateDoc(u.ID, domain.CreateDocBody{Title: "空资料", ContentMd: strPtr("")})
	if err != nil {
		t.Fatal(err)
	}
	md := "一些正文"
	if _, err := s.PutDocContent(doc.ID, domain.PutContentBody{ContentMd: md, Author: domain.AuthorHuman}); err != nil {
		t.Fatal(err)
	}
	if err := s.DeleteDoc(doc.ID); err != nil {
		t.Fatalf("DeleteDoc: %v", err)
	}
	if _, err := s.GetDoc(doc.ID); err == nil {
		t.Fatal("删除后 GetDoc 应 404")
	}
	docs, _ := s.ListDocsByWork(u.ID)
	if len(docs) != 0 {
		t.Fatalf("ListDocsByWork 仍返回 %d 条", len(docs))
	}
	if err := s.DeleteDoc("nonexistent"); err == nil {
		t.Fatal("删不存在的 doc 应报错")
	}
}

// 审计：五类发现各命中一条。
func TestAuditFindings(t *testing.T) {
	s := newTestStore(t)
	// misplaced: 单作挂根
	orphan := mustWork(t, s, "孤儿单作", nil, domain.WorkKindWork)
	// misplaced: 单作下面挂了子节点
	host := mustWork(t, s, "有子的单作", nil, domain.WorkKindWork)
	_ = mustWork(t, s, "错挂的子", &host.ID, domain.WorkKindWork)
	// misplaced: 宇宙挂系列下
	sr := mustWork(t, s, "系列X", nil, domain.WorkKindSeries)
	_ = mustWork(t, s, "倒挂的宇宙", &sr.ID, domain.WorkKindUniverse)
	// driftStub: stub 有大正文。正常写路径会把 stub 自动升 draft（PutWorkContent 的既有行为），
	// 所以这个形态只能来自导入/手工改库等旁路 —— 测试里写完后把状态压回去模拟。
	stub := mustWork(t, s, "伪stub", nil, domain.WorkKindWork)
	if _, err := s.PutWorkContent(stub.ID, domain.PutContentBody{ContentMd: strings.Repeat("字", 600), Author: domain.AuthorHuman}); err != nil {
		t.Fatal(err)
	}
	stubSt := domain.WorkStatusStub
	if _, err := s.PatchWork(stub.ID, domain.PatchWorkBody{Status: &stubSt}); err != nil {
		t.Fatal(err)
	}
	// driftReady: ready 空正文
	ready := mustWork(t, s, "假ready", nil, domain.WorkKindWork)
	rd := domain.WorkStatusReady
	if _, err := s.PatchWork(ready.ID, domain.PatchWorkBody{Status: &rd}); err != nil {
		t.Fatal(err)
	}
	// duplicates: 同名 + 别名撞名
	_ = mustWork(t, s, "重名者", nil, domain.WorkKindWork)
	_ = mustWork(t, s, "重名者", nil, domain.WorkKindWork)
	host2 := mustWork(t, s, "本尊", nil, domain.WorkKindWork)
	alias := []string{"冒名者"}
	if _, err := s.PatchWork(host2.ID, domain.PatchWorkBody{Aliases: &alias}); err != nil {
		t.Fatal(err)
	}
	_ = mustWork(t, s, "冒名者", nil, domain.WorkKindWork)
	// noMedium: 孤儿/伪stub 都是无介质单作（上面已满足，不重复造）

	rep, err := s.Audit()
	if err != nil {
		t.Fatalf("Audit: %v", err)
	}
	if rep.Total < 10 {
		t.Fatalf("total = %d", rep.Total)
	}
	has := func(list []domain.AuditFinding, id string) bool {
		for _, f := range list {
			if f.ID == id {
				return true
			}
		}
		return false
	}
	if !has(rep.Misplaced, orphan.ID) {
		t.Fatalf("misplaced 缺孤儿单作: %+v", rep.Misplaced)
	}
	if !has(rep.Misplaced, host.ID) {
		t.Fatalf("misplaced 缺有子单作: %+v", rep.Misplaced)
	}
	if !has(rep.DriftStub, stub.ID) {
		t.Fatalf("driftStub 缺: %+v", rep.DriftStub)
	}
	if !has(rep.DriftReady, ready.ID) {
		t.Fatalf("driftReady 缺: %+v", rep.DriftReady)
	}
	if len(rep.Duplicates) < 3 {
		t.Fatalf("duplicates 应 ≥3（2 重名 + 1 别名撞名）: %+v", rep.Duplicates)
	}
	if !has(rep.NoMedium, orphan.ID) {
		t.Fatalf("noMedium 缺: %+v", rep.NoMedium)
	}
	// 归档的节点不进体检
	arch := mustWork(t, s, "归档的单作", nil, domain.WorkKindWork)
	arc := domain.WorkStatusArchived
	if _, err := s.PatchWork(arch.ID, domain.PatchWorkBody{Status: &arc}); err != nil {
		t.Fatal(err)
	}
	rep2, _ := s.Audit()
	if !has(rep2.NoMedium, orphan.ID) || has(rep2.NoMedium, arch.ID) {
		t.Fatalf("归档节点应被体检排除: %+v", rep2.NoMedium)
	}
	if rep2.Archived != 1 {
		t.Fatalf("archived 计数 = %d", rep2.Archived)
	}
}

// CreateRelationsBatch：全量预检、整批落或整批不落、批内查环。
func TestCreateRelationsBatch(t *testing.T) {
	s := newTestStore(t)
	a := mustWork(t, s, "A", nil, domain.WorkKindWork).ID
	b := mustWork(t, s, "B", nil, domain.WorkKindWork).ID
	c := mustWork(t, s, "C", nil, domain.WorkKindWork).ID

	// 正常批
	got, err := s.CreateRelationsBatch([]domain.CreateRelationBody{
		{FromID: a, ToID: b, Type: domain.RelationSequelTo},
		{FromID: b, ToID: c, Type: domain.RelationSameSeries},
	})
	if err != nil {
		t.Fatalf("正常批: %v", err)
	}
	if len(got) != 2 {
		t.Fatalf("创建 %d 条", len(got))
	}
	if n, _ := s.listRelationsCount(t); n != 2 {
		t.Fatalf("库里 %d 条", n)
	}

	// 一条坏 → 整批不落（前置断言：批前 2 条，批后仍 2 条）
	_, err = s.CreateRelationsBatch([]domain.CreateRelationBody{
		{FromID: a, ToID: c, Type: domain.RelationSameSeries},
		{FromID: a, ToID: "no-such-node", Type: domain.RelationReferences},
	})
	if err == nil {
		t.Fatal("端点不存在应拒绝")
	}
	if n, _ := s.listRelationsCount(t); n != 2 {
		t.Fatalf("整批未落被破坏：库里 %d 条（应仍 2）", n)
	}

	// 批内成环：a→b 已存在方向性边，批里再 b→?, 造 a? 不行（已存在）。
	// 用 fresh 节点：x→y, y→z, z→x 全是 directional
	x := mustWork(t, s, "X", nil, domain.WorkKindWork).ID
	y := mustWork(t, s, "Y", nil, domain.WorkKindWork).ID
	z := mustWork(t, s, "Z", nil, domain.WorkKindWork).ID
	_, err = s.CreateRelationsBatch([]domain.CreateRelationBody{
		{FromID: x, ToID: y, Type: domain.RelationSequelTo},
		{FromID: y, ToID: z, Type: domain.RelationSequelTo},
		{FromID: z, ToID: x, Type: domain.RelationSequelTo},
	})
	if err == nil {
		t.Fatal("批内成环应拒绝")
	}
	if n, _ := s.listRelationsCount(t); n != 2 {
		t.Fatalf("环批未落被破坏：库里 %d 条（应仍 2）", n)
	}

	// 批内重复三元组
	_, err = s.CreateRelationsBatch([]domain.CreateRelationBody{
		{FromID: x, ToID: y, Type: domain.RelationReferences},
		{FromID: x, ToID: y, Type: domain.RelationReferences},
	})
	if err == nil {
		t.Fatal("批内重复三元组应拒绝")
	}

	// 与已存在的边撞三元组
	_, err = s.CreateRelationsBatch([]domain.CreateRelationBody{
		{FromID: a, ToID: b, Type: domain.RelationSequelTo},
	})
	if err == nil {
		t.Fatal("与既有边重复合应拒绝")
	}

	// 上限
	big := make([]domain.CreateRelationBody, 0, 25)
	for i := 0; i < 25; i++ {
		big = append(big, domain.CreateRelationBody{FromID: a, ToID: b, Type: domain.RelationReferences})
	}
	if _, err := s.CreateRelationsBatch(big); err == nil {
		t.Fatal("超过 20 条应拒绝")
	}
}

func (s *Store) listRelationsCount(t *testing.T) (int, error) {
	t.Helper()
	var n int
	err := s.DB.QueryRow(`SELECT COUNT(*) FROM relations`).Scan(&n)
	return n, err
}
