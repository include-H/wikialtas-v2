package run

import (
	"strings"
	"testing"

	"wikiatlas/backend/internal/domain"
	"wikiatlas/backend/internal/store"
)

// 预取要让模型"开口就知道目标对象长什么样"：节点元信息 + 现有章节 + 正文开头 +
// 下级版图 + 资料夹清单。这段文本会直接进入首条 user 消息。
func TestContextBriefForSeries(t *testing.T) {
	st, err := store.OpenMemory()
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()

	ff, err := st.CreateWork(domain.CreateWorkBody{Kind: domain.WorkKindUniverse, Title: "最终幻想"})
	if err != nil {
		t.Fatal(err)
	}
	series, err := st.CreateWork(domain.CreateWorkBody{
		Kind: domain.WorkKindSeries, Title: "最终幻想：新水晶神话", ParentID: &ff.ID,
	})
	if err != nil {
		t.Fatal(err)
	}
	body := "# 最终幻想：新水晶神话\n\n> 说明：系列主文。\n\n## 1. 系列概览\n\n水晶神话是跨作品共享的神话基底。\n\n## 2. 作品构成\n\n包含 13 与 零式。\n"
	if _, err := st.PutWorkContent(series.ID, domain.PutContentBody{
		ContentMd: body, Author: domain.AuthorHuman,
	}); err != nil {
		t.Fatal(err)
	}
	var kidID string
	for _, title := range []string{"最终幻想13", "最终幻想 零式"} {
		k, err := st.CreateWork(domain.CreateWorkBody{
			Kind: domain.WorkKindWork, Title: title, ParentID: &series.ID,
		})
		if err != nil {
			t.Fatal(err)
		}
		kidID = k.ID
	}
	if _, err := st.CreateDoc(series.ID, domain.CreateDocBody{Title: "神话基底梳理"}); err != nil {
		t.Fatal(err)
	}

	brief := buildContextBrief(st, series.ID, "")
	for _, want := range []string{
		"最终幻想：新水晶神话",
		"series",
		"所属层级：最终幻想(universe)",
		"现有章节：1. 系列概览 / 2. 作品构成",
		"正文开头",
		"下级节点（2）",
		"《最终幻想13》(work,stub)",
		"《最终幻想 零式》(work,stub)",
		"《神话基底梳理》",
	} {
		if !strings.Contains(brief, want) {
			t.Fatalf("brief 缺少 %q\n---\n%s", want, brief)
		}
	}
	// 清单里必须**带 id**：只给标题的话，模型想读别家条目还得先调 get_tree 换 id，
	// 多一跳就常常不跳——"能连读别家条目"于是停在纸面上。
	if !strings.Contains(brief, kidID) {
		t.Fatalf("brief 里没有子节点的 id（%s），模型还得先去换：\n%s", kidID, brief)
	}
}

// 空节点必须被明确标出来，避免模型对着 stub 编内容。
func TestContextBriefMarksEmptyNode(t *testing.T) {
	st, err := store.OpenMemory()
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	w, err := st.CreateWork(domain.CreateWorkBody{Kind: domain.WorkKindWork, Title: "王者之剑"})
	if err != nil {
		t.Fatal(err)
	}
	brief := buildContextBrief(st, w.ID, "")
	if !strings.Contains(brief, "空节点") {
		t.Fatalf("空节点提示缺失:\n%s", brief)
	}
}

// 预取要告诉模型"这个节点挂了库里的哪些实物"（正片位 / 混杂内容）。
// 不给的话，写正文时只看得到标题里的「（剧集）」两个字——年份、原著漫画、
// 库里那条的原始标题与库名全都拿不到，于是要么联网搜（同名作品极多），
// 要么把改编关系写错。这些东西本来就躺在库里。
func TestContextBriefIncludesLibraryMedia(t *testing.T) {
	st, err := store.OpenMemory()
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()

	tv := domain.MediumTV
	w, err := st.CreateWork(domain.CreateWorkBody{
		Kind: domain.WorkKindWork, Title: "16bit的感动（剧集）", Medium: &tv,
	})
	if err != nil {
		t.Fatal(err)
	}
	rel := "2024-10-05"
	if err := st.SaveLibraryManifest(&domain.LibraryManifest{
		ScannedAt: "2026-10-09T00:00:00Z",
		Entries: []domain.LibraryManifestEntry{
			{Key: "emby:tv1", Source: "emby", EntryID: "tv1", Title: "16bit的感动", Kind: "tv",
				ContainerTitle: "番剧", ContainerKind: "library", ReleaseDate: &rel},
			{Key: "komga:m1", Source: "komga", EntryID: "m1", Title: "16bit的感动", Kind: "book", Format: "comic",
				ContainerTitle: "漫画", ContainerKind: "series"},
			{Key: "emby:pre1", Source: "emby", EntryID: "pre1", Title: "16bit的感动 预告", Kind: "movie",
				ContainerTitle: "番剧", ContainerKind: "library"},
		},
	}); err != nil {
		t.Fatal(err)
	}
	tvHint, preHint := "16bit的感动", "16bit的感动 预告"
	m1Hint := "16bit的感动"
	if _, err := st.CreateLibraryLink(w.ID, domain.LibraryEmby, "tv1", nil, &tvHint, domain.LinkRoleWork, domain.LinkByArchive); err != nil {
		t.Fatal(err)
	}
	if _, err := st.CreateLibraryLink(w.ID, domain.LibraryKomga, "m1", nil, &m1Hint, domain.LinkRoleWork, domain.LinkByArchive); err != nil {
		t.Fatal(err)
	}
	if _, err := st.CreateLibraryLink(w.ID, domain.LibraryEmby, "pre1", nil, &preHint, domain.LinkRoleMix, domain.LinkByManual); err != nil {
		t.Fatal(err)
	}

	got := buildContextBrief(st, w.ID, "")
	for _, want := range []string{
		"正片位（库里实际有的载体）",
		"《16bit的感动》（2024年） · Emby「番剧」",
		"《16bit的感动》 · Komga「漫画」",
		"混杂内容（1 条",
		"《16bit的感动 预告》",
	} {
		if !strings.Contains(got, want) {
			t.Fatalf("预取里应有 %q：\n%s", want, got)
		}
	}
}

// 没有媒体链时不该冒出那一段（别给模型空标题）。
func TestContextBriefOmitsMediaWhenNoLinks(t *testing.T) {
	st, err := store.OpenMemory()
	if err != nil {
		t.Fatal(err)
	}
	defer st.Close()
	w, err := st.CreateWork(domain.CreateWorkBody{Kind: domain.WorkKindWork, Title: "无媒体的节点"})
	if err != nil {
		t.Fatal(err)
	}
	got := buildContextBrief(st, w.ID, "")
	if strings.Contains(got, "正片位") || strings.Contains(got, "混杂内容") {
		t.Fatalf("没有链时不该有媒体段：\n%s", got)
	}
}
