package run

import (
	"strings"
	"testing"

	"wikiatlas/backend/internal/domain"
)

// 媒体库建议页的"当前页面"段：没扫过要如实说没扫过，扫过要给各源统计与待确认数。
// 实测动机：在这一页问"你知道现在是什么页面吗"，模型此前只能答"我看不到"。
func TestBuildPageBriefLibrary(t *testing.T) {
	m := newTestManager(t)

	if got := m.buildPageBrief("library"); !strings.Contains(got, "还没有扫描过") {
		t.Fatalf("清单缺失时应如实说没扫过，得到 %q", got)
	}

	man := &domain.LibraryManifest{
		ScannedAt: "2026-10-07T08:00:00Z",
		Entries: []domain.LibraryManifestEntry{
			{Key: "emby:1", Source: "emby", LinkedWorkID: "w1"}, // 已挂
			{Key: "emby:2", Source: "emby"},                     // 未挂
			{Key: "komga:9", Source: "komga"},                   // 未挂，但被全局忽略
		},
		IgnoredKeys: []string{"komga:9"},
	}
	if err := m.store.SaveLibraryManifest(man); err != nil {
		t.Fatal(err)
	}
	// 已建、还没写正文的节点（= /stubs 的待写清单，本页的下游）
	if _, err := m.store.CreateWork(domain.CreateWorkBody{Kind: domain.WorkKindWork, Title: "待写一"}); err != nil {
		t.Fatal(err)
	}

	got := m.buildPageBrief("library")
	for _, want := range []string{
		"## 当前页面",
		"媒体库建议",
		"Emby 2（未挂 1）",
		"Komga 1（未挂 1）",
		"2026-10-07T08:00:00Z",
		"待写正文 1 篇",
	} {
		if !strings.Contains(got, want) {
			t.Fatalf("缺少 %q：\n%s", want, got)
		}
	}
}

// 其余页面各一行；未知页面名照实回显（不猜），空页面名不产出这一段。
func TestBuildPageBriefOtherPages(t *testing.T) {
	m := newTestManager(t)

	if got := m.buildPageBrief(""); got != "" {
		t.Fatalf("空页面名不该产出这一段，得到 %q", got)
	}
	if got := m.buildPageBrief("runs"); !strings.Contains(got, "工单页") {
		t.Fatalf("runs 应给人话，得到 %q", got)
	}
	if got := m.buildPageBrief("home"); !strings.Contains(got, "首页") {
		t.Fatalf("home 应给人话，得到 %q", got)
	}
	// 非媒体库页不该带库状态（那是 library 独有的简报）
	if got := m.buildPageBrief("home"); strings.Contains(got, "库状态") {
		t.Fatalf("非媒体库页不该带库状态，得到 %q", got)
	}
	if got := m.buildPageBrief("weird"); !strings.Contains(got, "weird") {
		t.Fatalf("认不出的页面名应原样回显，得到 %q", got)
	}
}
