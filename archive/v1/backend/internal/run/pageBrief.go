package run

import (
	"fmt"
	"sort"
	"strings"
)

// 页面名 → 一句人话。页面名对模型不是自明的：`library` 是哪个库、`runs` 是什么，
// 都得配一句说明；否则"当前页面：library"等于没给。
//
// 这里也是"页面"这件事的唯一 owner：模型该看到哪些页面、每页是什么，只在本文件定义。
// 前端只报页面名（context.page），不报它的含义。
var pageLabels = map[string]string{
	"home":     "首页（最近更新的条目列表）",
	"work":     "作品正文页（正在看/改这篇作品）",
	"folder":   "资料夹页（这个节点下的资料列表）",
	"doc":      "资料正文页（正在看/改这篇资料）",
	"runs":     "工单页（历史与进行中的 Altas 工单）",
	"library":  "媒体库建议（从库里批量挑、批量建档的进料台：挂链 / 建档 / 忽略；挑中的会在『待写正文』里交给 Altas 写）",
	"settings": "设置页（模型、媒体库连接、访问与隐私）",
}

// 三个媒体库源在界面上的名字（与前端 mediaTags 的 SOURCE_ORDER 一致）。
var pageSourceDisplay = map[string]string{"gameatlas": "GameAtlas", "emby": "Emby", "komga": "Komga"}

// buildPageBrief 生成"## 当前页面"段：一句人话 + 这一页的数据面（有什么、多少）。
//
// 为什么简报由后端装配、而不是前端一起发：页面上那一坨数据的 owner 在后端
// （清单与建议两个 KV），前端只该回答"哪一页"。数据各归各家，才不会两边各算一遍。
//
// 实测动机：在媒体库建议页问"你知道现在是什么页面吗"，模型此前只能如实回答
// "我看不到你当前打开的页面"——因为它拿到的只有"意图 / 目标"两行。
func (m *Manager) buildPageBrief(page string) string {
	if strings.TrimSpace(page) == "" {
		return ""
	}
	label := pageLabels[page]
	if label == "" {
		label = page // 认不出的页面名照实回显，不猜
	}
	var b strings.Builder
	b.WriteString("\n## 当前页面\n")
	b.WriteString(label + "\n")
	if page == "library" {
		b.WriteString(m.libraryStatusLine() + "\n")
		// 指路：页面上那几条建议，模型是看不见的（建议住在 KV 里），得用工具读。
		// 不指这一句，它要么去翻清单凑、要么直接猜用户说的"第几条"。
		b.WriteString("页面上按容器分组列出未挂链条目（带封面）；建档=在节点树里建一个待写节点，之后到「待写正文」交给 Altas 写。\n")
	}
	return b.String()
}

// libraryStatusLine 是媒体库建议页那行状态：各源 总数/未挂、上次扫描时间、待确认建议数。
//
// 口径说明：这里的"待确认"只做两个**便宜的**过滤（未挂链、未被全局忽略），不等于
// 池子页那份权威列表（它还要求目标节点仍然存在）。它是给模型的现状摘要，不是判决
// 依据——真要动手，让模型自己去 list_library / search_library 查，别读这一行。
func (m *Manager) libraryStatusLine() string {
	man, err := m.store.GetLibraryManifest()
	if err != nil || man == nil || man.ScannedAt == "" {
		return "库状态：还没有扫描过（用户可以在本页点『扫描媒体库』）。"
	}

	type srcStat struct{ total, unlinked int }
	bySource := map[string]*srcStat{}
	linked := map[string]bool{}
	for i := range man.Entries {
		e := &man.Entries[i]
		s := bySource[e.Source]
		if s == nil {
			s = &srcStat{}
			bySource[e.Source] = s
		}
		s.total++
		if e.LinkedWorkID == "" {
			s.unlinked++
		} else {
			linked[e.Key] = true
		}
	}

	names := make([]string, 0, len(bySource))
	for k := range bySource {
		names = append(names, k)
	}
	sort.Strings(names)
	parts := make([]string, 0, len(names))
	for _, k := range names {
		name := pageSourceDisplay[k]
		if name == "" {
			name = k
		}
		parts = append(parts, fmt.Sprintf("%s %d（未挂 %d）", name, bySource[k].total, bySource[k].unlinked))
	}
	if len(parts) == 0 {
		parts = append(parts, "一个源都没配")
	}

	// 已建节点、还没写正文的（= /stubs 上的待写清单）：这页的下游。
	stubs := 0
	if nodes, err := m.store.ListTree(); err == nil {
		for _, n := range nodes {
			if n.Status == "stub" {
				stubs++
			}
		}
	}

	return fmt.Sprintf("库状态：%s；扫描于 %s；待写正文 %d 篇。",
		strings.Join(parts, " · "), man.ScannedAt, stubs)
}
