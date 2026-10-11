package httpapi_test

import (
	"testing"

	"wikiatlas/backend/internal/domain"
)

// 同名不同来源（Emby 番剧 + Komga 漫画）走一次批量建档：
// 应建出 系列《16bit的感动》 + 子《16bit的感动（剧集）》+ 子《16bit的感动（漫画）》，
// 两条链各挂到自己的子节点；**再跑一次不新建**（幂等）。
func TestArchivePlaceSeriesAndMediaChildren(t *testing.T) {
	ts, st := newTestServer(t)
	if err := st.SaveLibraryManifest(&domain.LibraryManifest{
		ScannedAt: "2026-10-08T00:00:00Z",
		Entries: []domain.LibraryManifestEntry{
			{Key: "emby:tv1", Source: "emby", EntryID: "tv1", Title: "16bit的感动", Kind: "tv", ContainerKey: "view:1", ContainerKind: "library"},
			{Key: "komga:m1", Source: "komga", EntryID: "m1", Title: "16bit的感动", Kind: "book", Format: "comic", ContainerKey: "series:x", ContainerKind: "series"},
		},
	}); err != nil {
		t.Fatal(err)
	}

	body := map[string]any{"items": []map[string]any{
		{"source": "emby", "externalId": "tv1"},
		{"source": "komga", "externalId": "m1"},
	}}
	rep := doJSON(t, "POST", ts.URL+"/api/library/archive-place", body, 200)
	if rep["linked"] != float64(2) {
		t.Fatalf("linked = %v（failed=%v）", rep["linked"], rep["failed"])
	}

	nodes, err := st.ListTree()
	if err != nil {
		t.Fatal(err)
	}
	byTitle := map[string]domain.WorkSummary{}
	for _, n := range nodes {
		byTitle[n.Title] = n
	}
	series, ok := byTitle["16bit的感动"]
	if !ok || series.Kind != domain.WorkKindSeries {
		t.Fatalf("应建出系列《16bit的感动》: %#v", byTitle)
	}
	for _, want := range []string{"16bit的感动（剧集）", "16bit的感动（漫画）"} {
		child, ok := byTitle[want]
		if !ok {
			t.Fatalf("应建出子节点 %q: %#v", want, byTitle)
		}
		if child.ParentID == nil || *child.ParentID != series.ID {
			t.Fatalf("%q 应挂在系列下, got parent=%v", want, child.ParentID)
		}
	}

	// 两条链各挂到自己的子节点
	embyLinks, _ := st.ListLibraryLinksBySource(domain.LibraryEmby)
	if len(embyLinks) != 1 || embyLinks[0].WorkID != byTitle["16bit的感动（剧集）"].ID {
		t.Fatalf("emby 链应挂在《（剧集）》: %#v", embyLinks)
	}
	komgaLinks, _ := st.ListLibraryLinksBySource(domain.LibraryKomga)
	if len(komgaLinks) != 1 || komgaLinks[0].WorkID != byTitle["16bit的感动（漫画）"].ID {
		t.Fatalf("komga 链应挂在《（漫画）》: %#v", komgaLinks)
	}

	// 幂等：再跑一次 → 不新建节点（两条都命中已有子节点）
	rep2 := doJSON(t, "POST", ts.URL+"/api/library/archive-place", body, 200)
	if rep2["created"] != float64(0) {
		t.Fatalf("重跑不该新建节点, created=%v", rep2["created"])
	}
	after, _ := st.ListTree()
	if len(after) != len(nodes) {
		t.Fatalf("重跑后节点数变了：%d → %d", len(nodes), len(after))
	}
}

// 「不归属」：parentId 显式给空串 → 节点建在顶级（不建系列）。
func TestArchivePlaceTopLevel(t *testing.T) {
	ts, st := newTestServer(t)
	if err := st.SaveLibraryManifest(&domain.LibraryManifest{
		ScannedAt: "2026-10-08T00:00:00Z",
		Entries: []domain.LibraryManifestEntry{
			{Key: "gameatlas:g1", Source: "gameatlas", EntryID: "g1", Title: "刺客信条：叛变", Kind: "game", ContainerKey: "series:ac", ContainerKind: "series"},
		},
	}); err != nil {
		t.Fatal(err)
	}
	rep := doJSON(t, "POST", ts.URL+"/api/library/archive-place", map[string]any{
		"items":    []map[string]any{{"source": "gameatlas", "externalId": "g1"}},
		"parentId": "",
	}, 200)
	if rep["linked"] != float64(1) {
		t.Fatalf("linked = %v（failed=%v）", rep["linked"], rep["failed"])
	}
	nodes, _ := st.ListTree()
	if len(nodes) != 1 {
		t.Fatalf("应只建一个顶级节点, got %d", len(nodes))
	}
	if nodes[0].ParentID != nil {
		t.Fatalf("「不归属」应建在顶级, got parent=%v", nodes[0].ParentID)
	}
	if nodes[0].Title != "刺客信条：叛变" || nodes[0].Kind != domain.WorkKindWork {
		t.Fatalf("顶级节点形状不对: %#v", nodes[0])
	}
}

// 命中已有单作 → 直接关联，不建节点（同一作品的媒体各挂正片位）。
func TestArchivePlaceLinksToExistingWork(t *testing.T) {
	ts, st := newTestServer(t)
	w, err := st.CreateWork(domain.CreateWorkBody{Kind: domain.WorkKindWork, Title: "葬送的芙莉莲"})
	if err != nil {
		t.Fatal(err)
	}
	if err := st.SaveLibraryManifest(&domain.LibraryManifest{
		ScannedAt: "2026-10-08T00:00:00Z",
		Entries: []domain.LibraryManifestEntry{
			{Key: "emby:sr-frieren", Source: "emby", EntryID: "sr-frieren", Title: "葬送的芙莉莲", Kind: "tv"},
		},
	}); err != nil {
		t.Fatal(err)
	}
	rep := doJSON(t, "POST", ts.URL+"/api/library/archive-place", map[string]any{
		"items": []map[string]any{{"source": "emby", "externalId": "sr-frieren"}},
	}, 200)
	if rep["created"] != float64(0) || rep["linked"] != float64(1) {
		t.Fatalf("命中单作应只挂链不建节点, got %v", rep)
	}
	links, _ := st.ListLibraryLinksBySource(domain.LibraryEmby)
	if len(links) != 1 || links[0].WorkID != w.ID {
		t.Fatalf("链应挂到已有单作: %#v", links)
	}
}
