package tools_test

import (
	"strings"
	"testing"

	"wikiatlas/backend/internal/tools"
)

// 归档 + 受限删除：LLM 建错节点的两条退路。
func TestDeleteWorkGuards(t *testing.T) {
	d, st := newDeps(t)
	reg := tools.NewLibrarianRegistry(d)

	// 空节点可删
	created := call(t, reg, "upsert_work", map[string]any{"kind": "work", "title": "建错的节点"})
	id := created["id"].(string)
	del := call(t, reg, "delete_work", map[string]any{"id": id})
	if del["ok"] != true {
		t.Fatalf("delete_work 空节点: %v", del)
	}
	if _, err := st.GetWork(id); err == nil {
		t.Fatal("节点应已删除")
	}

	// 有正文的拒绝删，提示走归档
	w2 := call(t, reg, "upsert_work", map[string]any{"kind": "work", "title": "有正文的"})
	id2 := w2["id"].(string)
	if r := call(t, reg, "write_content", map[string]any{
		"targetId":  id2,
		"contentMd": strings.Repeat("这是正文", 40),
	}); r["ok"] != true {
		t.Fatalf("write_content: %v", r)
	}
	del2 := call(t, reg, "delete_work", map[string]any{"id": id2})
	if del2["ok"] != false {
		t.Fatalf("有正文应拒删: %v", del2)
	}

	// 归档后可写拒
	if r := call(t, reg, "upsert_work", map[string]any{"id": id2, "status": "archived"}); r["ok"] != true {
		t.Fatalf("归档: %v", r)
	}
	if r := call(t, reg, "write_content", map[string]any{
		"targetId":  id2,
		"contentMd": "再写一次",
	}); r["ok"] != false {
		t.Fatalf("归档节点拒写失败: %v", r)
	}
	if r := call(t, reg, "edit", map[string]any{"targetId": id2, "oldString": "这是正文", "newString": "改"}); r["ok"] != false {
		t.Fatalf("归档节点 edit 拒写失败: %v", r)
	}
	if r := call(t, reg, "patch_section", map[string]any{"targetId": id2, "heading": "x", "newMarkdown": "y"}); r["ok"] != false {
		t.Fatalf("归档节点 patch_section 拒写失败: %v", r)
	}

	// 恢复后可写
	if r := call(t, reg, "upsert_work", map[string]any{"id": id2, "status": "draft"}); r["ok"] != true {
		t.Fatalf("恢复: %v", r)
	}
	if r := call(t, reg, "write_content", map[string]any{
		"targetId":  id2,
		"contentMd": strings.Repeat("恢复后的正文", 40),
	}); r["ok"] != true {
		t.Fatalf("恢复后拒写: %v", r)
	}

	// 有子节点的拒绝删
	u := call(t, reg, "upsert_work", map[string]any{"kind": "universe", "title": "有儿的宇"})
	uID := u["id"].(string)
	_ = call(t, reg, "upsert_work", map[string]any{"kind": "work", "title": "儿的子", "parentId": uID})
	if r := call(t, reg, "delete_work", map[string]any{"id": uID}); r["ok"] != false {
		t.Fatalf("有子节点应拒删: %v", r)
	}
}

// delete_doc：空资料可删，有正文拒。
func TestDeleteDocGuards(t *testing.T) {
	d, _ := newDeps(t)
	reg := tools.NewLibrarianRegistry(d)
	u := call(t, reg, "upsert_work", map[string]any{"kind": "universe", "title": "资料宿主"})
	uID := u["id"].(string)
	doc := call(t, reg, "create_doc", map[string]any{"workId": uID, "title": "空资料"})
	did := doc["id"].(string)
	if r := call(t, reg, "delete_doc", map[string]any{"id": did}); r["ok"] != true {
		t.Fatalf("空资料应可删: %v", r)
	}

	doc2 := call(t, reg, "create_doc", map[string]any{"workId": uID, "title": "有料资料", "contentMd": "正文"})
	did2 := doc2["id"].(string)
	if r := call(t, reg, "delete_doc", map[string]any{"id": did2}); r["ok"] != false {
		t.Fatalf("有正文资料应拒删: %v", r)
	}
}

// upsert_relations：整批落 / 整批不落脚本。
func TestUpsertRelationsBatch(t *testing.T) {
	d, _ := newDeps(t)
	reg := tools.NewLibrarianRegistry(d)
	mk := func(title string) string {
		r := call(t, reg, "upsert_work", map[string]any{"kind": "work", "title": title})
		return r["id"].(string)
	}
	a, b, c := mk("R1"), mk("R2"), mk("R3")

	ok := call(t, reg, "upsert_relations", map[string]any{
		"relations": []map[string]any{
			{"fromId": a, "toId": b, "type": "sequel_to"},
			{"fromId": b, "toId": c, "type": "same_series"},
		},
	})
	if ok["ok"] != true || ok["created"].(float64) != 2 {
		t.Fatalf("正常批: %v", ok)
	}

	// 一条坏 → 整批不落（第三条指向不存在的节点）
	bad := call(t, reg, "upsert_relations", map[string]any{
		"relations": []map[string]any{
			{"fromId": a, "toId": c, "type": "references"},
			{"fromId": a, "toId": "ghost", "type": "references"},
		},
	})
	if bad["ok"] != true {
		// 应整批不落
	}
	if bad["ok"] != false {
		t.Fatalf("含坏边应整批拒: %v", bad)
	}

	// 批内成环
	x, y, z := mk("X"), mk("Y"), mk("Z")
	ring := call(t, reg, "upsert_relations", map[string]any{
		"relations": []map[string]any{
			{"fromId": x, "toId": y, "type": "sequel_to"},
			{"fromId": y, "toId": z, "type": "sequel_to"},
			{"fromId": z, "toId": x, "type": "sequel_to"},
		},
	})
	if ring["ok"] != false {
		t.Fatalf("批内成环应拒: %v", ring)
	}

	// 上限
	many := make([]map[string]any, 0, 25)
	for i := 0; i < 25; i++ {
		many = append(many, map[string]any{"fromId": x, "toId": y, "type": "references"})
	}
	if r := call(t, reg, "upsert_relations", map[string]any{"relations": many}); r["ok"] != false {
		t.Fatalf("超上限应拒: %v", r)
	}
}

// list_works + audit 的形状与语义。
func TestListWorksAndAuditTools(t *testing.T) {
	d, st := newDeps(t)
	reg := tools.NewLibrarianRegistry(d)

	big := strings.Repeat("正文", 300)
	u := call(t, reg, "upsert_work", map[string]any{"kind": "universe", "title": "LU", "tags": []string{"1997"}})
	uID := u["id"].(string)
	w := call(t, reg, "upsert_work", map[string]any{"kind": "work", "title": "LW", "parentId": uID, "medium": "game"})
	wID := w["id"].(string)
	_ = call(t, reg, "upsert_work", map[string]any{"kind": "work", "title": "孤儿", "medium": "game"})
	_ = call(t, reg, "write_content", map[string]any{"targetId": wID, "contentMd": big})

	lw := call(t, reg, "list_works", map[string]any{"kind": "work", "hasContent": true})
	if lw["ok"] != true || lw["count"].(float64) != 1 {
		t.Fatalf("list_works hasContent: %v", lw)
	}
	empty := call(t, reg, "list_works", map[string]any{"kind": "work", "hasContent": false})
	if empty["ok"] != true || empty["count"].(float64) != 1 {
		t.Fatalf("list_works empty: %v", empty)
	}
	// 归档态查不到默认列表
	_ = call(t, reg, "upsert_work", map[string]any{"id": wID, "status": "archived"})
	active := call(t, reg, "list_works", map[string]any{"kind": "work"})
	if active["count"].(float64) != 1 {
		t.Fatalf("默认应只剩活跃 work: %v", active)
	}
	arch := call(t, reg, "list_works", map[string]any{"kind": "work", "status": "archived"})
	if arch["count"].(float64) != 1 {
		t.Fatalf("archived 列表: %v", arch)
	}
	tagged := call(t, reg, "list_works", map[string]any{"tag": "1997"})
	if tagged["count"].(float64) != 1 {
		t.Fatalf("tag 筛: %v", tagged)
	}

	// audit：孤儿 + 无介质 should 被报（孤儿=单作挂根；LW 有介质、孤儿有介质 → noMedium 0 条）
	_ = call(t, reg, "upsert_work", map[string]any{"kind": "work", "title": "缺介质"})
	rep := call(t, reg, "audit", map[string]any{})
	if rep["ok"] != true {
		t.Fatalf("audit: %v", rep)
	}
	misplaced := rep["misplaced"].([]any)
	foundOrphan := false
	for _, m := range misplaced {
		f := m.(map[string]any)
		if f["title"] == "孤儿" {
			foundOrphan = true
		}
	}
	if !foundOrphan {
		t.Fatalf("audit misplaced 缺孤儿: %v", rep["misplaced"])
	}
	noMedium := rep["noMedium"].([]any)
	if len(noMedium) != 1 {
		t.Fatalf("noMedium 应 1 条: %v", noMedium)
	}
	// stub drift 应 0（write_content 升了 draft）
	if n := len(rep["driftReady"].([]any)); n != 0 {
		t.Fatalf("driftReady 应 0: %v", rep["driftReady"])
	}
	_ = st
}

// 重挂：update 带 parentId 移动节点是确定行为；移进自己子树被拒。
func TestUpsertWorkRepaintRepars(t *testing.T) {
	d, st := newDeps(t)
	reg := tools.NewLibrarianRegistry(d)
	uA := call(t, reg, "upsert_work", map[string]any{"kind": "universe", "title": "U-A"})
	uB := call(t, reg, "upsert_work", map[string]any{"kind": "universe", "title": "U-B"})
	w := call(t, reg, "upsert_work", map[string]any{"kind": "work", "title": "要搬的", "parentId": uA["id"]})
	id := w["id"].(string)

	// 合法：搬到 U-B 下
	moved := call(t, reg, "upsert_work", map[string]any{"id": id, "parentId": uB["id"]})
	if moved["ok"] != true || moved["action"] != "updated" {
		t.Fatalf("重挂: %v", moved)
	}
	got, err := st.GetWork(id)
	if err != nil {
		t.Fatal(err)
	}
	if got.ParentID == nil || *got.ParentID != uB["id"].(string) {
		t.Fatalf("parentId 未生效: %v", got.ParentID)
	}

	// 造 w 的子孙：child（w 已挂在 U-B 下）
	child := call(t, reg, "upsert_work", map[string]any{"kind": "work", "title": "子的", "parentId": id})
	childID := child["id"].(string)

	// 拒：把 U-B（w 的祖先）挪到 w 下面 = 成环（w→U-B 已存在）
	// 等价于"把节点移进自己的子树"的镜像形态，用反向移动构造：
	// 先把 U-B 挪走（解除 w→U-B），再把 U-B 挂到 child 下，然后尝试 w→U-B
	if r := call(t, reg, "upsert_work", map[string]any{"id": id, "parentId": uA["id"]}); r["ok"] != true {
		t.Fatalf("w 回 U-A: %v", r)
	}
	// 现在链：U-A → w → child。把 U-B 挂到 child 下：
	if r := call(t, reg, "upsert_work", map[string]any{"id": uB["id"].(string), "parentId": childID}); r["ok"] != true {
		t.Fatalf("U-B 挂到 child 下: %v", r)
	}
	// 现在链：U-A → w → child → U-B。把 w 挪到 U-B 下 = w 进自己子孙的子树
	bad := call(t, reg, "upsert_work", map[string]any{"id": id, "parentId": uB["id"].(string)})
	if bad["ok"] != false {
		t.Fatalf("移进自己子树应拒: %v", bad)
	}
	// 自身当父 → 拒
	if r := call(t, reg, "upsert_work", map[string]any{"id": id, "parentId": id}); r["ok"] != false {
		t.Fatalf("自身当父应拒: %v", r)
	}
}
