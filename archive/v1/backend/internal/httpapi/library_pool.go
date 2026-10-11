package httpapi

import (
	"net/http"
	"sort"
	"strings"

	"wikiatlas/backend/internal/domain"
)

// --- 媒体库：建议池（读后台扫描清单）---
//
// 池子是「库状态仪表盘」：数据全部来自清单快照（秒开，不实时轰库）；
// 挂链状态用数据库实时覆盖（刚挂的立刻反映）。写操作只有三件、全是用户点的：
// 关联（挂到已有节点）/ 建档（建 stub 挂到所选父节点）/ 忽略（单条或整组，持久化）。
// AI 建议（/api/library/ai-suggest）只负责展示建议，点确认才执行。

type poolEntry struct {
	Source          string  `json:"source"`
	PublicID        string  `json:"publicId"`
	Title           string  `json:"title"`
	TitleAlt        *string `json:"titleAlt,omitempty"`
	ReleaseDate     *string `json:"releaseDate,omitempty"`
	CoverImage      string  `json:"coverImage,omitempty"`
	URL             string  `json:"url"`
	Kind            string  `json:"kind"`
	Format          string  `json:"format,omitempty"`
	Extra           string  `json:"extra,omitempty"`
	LinkedWorkID    string  `json:"linkedWorkId,omitempty"`
	LinkedWorkTitle string  `json:"linkedWorkTitle,omitempty"`
	FirstSeenAt     string  `json:"firstSeenAt,omitempty"`
	IsNew           bool    `json:"isNew,omitempty"`
	// Role 是条目固有角色（work=正片级 / mix=混杂级），随确认落到链上。
	Role string `json:"role,omitempty"`
	// （这里曾有 SuggestedIn：该条目有未处理的 AI 建议指向某节点。建议这套已退役。）
}

type poolGroupOut struct {
	Source         string      `json:"source"`
	ContainerKey   string      `json:"containerKey"`
	ContainerTitle string      `json:"containerTitle"`
	ContainerKind  string      `json:"containerKind,omitempty"`
	Unlinked       []poolEntry `json:"unlinked"`
	LinkedCount    int         `json:"linkedCount"`
	IgnoredCount   int         `json:"ignoredCount"`
	NewCount       int         `json:"newCount"`
}

// allLinkedIDs 返回 "source:entryId" → workID（实时挂链状态，覆盖清单快照）。
func (s *Server) allLinkedIDs() map[string]string {
	out := map[string]string{}
	for _, src := range []domain.LibrarySource{domain.LibraryGameAtlas, domain.LibraryEmby, domain.LibraryKomga} {
		for entryID, workID := range s.linkedIDsBySource(src) {
			out[string(src)+":"+entryID] = workID
		}
	}
	return out
}

func sourceOrder(source string) int {
	switch source {
	case "gameatlas":
		return 0
	case "emby":
		return 1
	case "komga":
		return 2
	default:
		return 9
	}
}

func (s *Server) poolEntryOf(e domain.LibraryManifestEntry, liveLinks map[string]string, nodeTitleByID map[string]string, prevScannedAt string) poolEntry {
	linkedWorkID := liveLinks[e.Key]
	role := domain.LinkRoleWork
	if e.Source == string(domain.LibraryEmby) && e.ContainerKind == domain.EmbyLibraryRoleMixed {
		role = domain.LinkRoleMix
	}
	return poolEntry{
		Source: e.Source, PublicID: e.EntryID, Title: e.Title, TitleAlt: e.TitleAlt,
		ReleaseDate: e.ReleaseDate, CoverImage: e.CoverImage, URL: e.URL,
		Kind: e.Kind, Format: e.Format, Extra: e.Extra,
		LinkedWorkID: linkedWorkID, LinkedWorkTitle: nodeTitleByID[linkedWorkID],
		FirstSeenAt: e.FirstSeenAt,
		IsNew:       prevScannedAt != "" && e.FirstSeenAt > prevScannedAt,
		Role:        role,
	}
}

func (s *Server) handleLibraryPool(w http.ResponseWriter, r *http.Request) {
	st, err := s.store.GetSettings()
	if err != nil {
		writeStoreErr(w, err)
		return
	}
	manifest, err := s.store.GetLibraryManifest()
	if err != nil {
		writeStoreErr(w, err)
		return
	}
	nodes, err := s.store.ListTree()
	if err != nil {
		writeStoreErr(w, err)
		return
	}
	nodeTitleByID := make(map[string]string, len(nodes))
	for _, n := range nodes {
		nodeTitleByID[n.ID] = n.Title
	}

	resp := map[string]any{
		"sourcesConfigured": map[string]bool{
			"gameatlas": st.Library.GameAtlasURL != nil && *st.Library.GameAtlasURL != "",
			"emby":      st.Library.EmbyURL != nil && *st.Library.EmbyAPIKey != "",
			"komga":     st.Library.KomgaURL != nil && *st.Library.KomgaAPIKey != "",
		},
		"rolesConfigured":     len(st.Library.EmbyLibraryRoles) > 0,
		"scanIntervalMinutes": scanIntervalMinutes(st),
	}
	if manifest == nil {
		resp["scannedAt"] = ""
		resp["groups"] = []poolGroupOut{}
		resp["ignoredEntries"] = []poolEntry{}
		resp["ignoredGroups"] = []domain.IgnoredGroup{}
		writeJSON(w, http.StatusOK, resp)
		return
	}

	liveLinks := s.allLinkedIDs()
	ignoredKeys := make(map[string]bool, len(manifest.IgnoredKeys))
	for _, k := range manifest.IgnoredKeys {
		ignoredKeys[k] = true
	}
	ignoredGroups := make(map[string]bool, len(manifest.IgnoredGroups))
	for _, g := range manifest.IgnoredGroups {
		ignoredGroups[g.Source+"|"+g.ContainerKey] = true
	}

	groupMap := map[string]*poolGroupOut{}
	entryByKey := make(map[string]domain.LibraryManifestEntry, len(manifest.Entries))
	ignoredEntries := make([]poolEntry, 0)
	for _, e := range manifest.Entries {
		entryByKey[e.Key] = e
		out := s.poolEntryOf(e, liveLinks, nodeTitleByID, manifest.PreviousScannedAt)
		gkey := e.Source + "|" + e.ContainerKey
		g := groupMap[gkey]
		if g == nil {
			g = &poolGroupOut{Source: e.Source, ContainerKey: e.ContainerKey, ContainerTitle: e.ContainerTitle, ContainerKind: e.ContainerKind}
			groupMap[gkey] = g
		}
		if ignoredKeys[e.Key] || ignoredGroups[gkey] {
			g.IgnoredCount++
			ignoredEntries = append(ignoredEntries, out)
			continue
		}
		if out.LinkedWorkID != "" {
			g.LinkedCount++
			continue
		}
		if out.IsNew {
			g.NewCount++
		}
		g.Unlinked = append(g.Unlinked, out)
	}
	groups := make([]poolGroupOut, 0, len(groupMap))
	for _, g := range groupMap {
		if len(g.Unlinked) == 0 {
			continue // 全挂链/全忽略的组不占版面
		}
		sort.Slice(g.Unlinked, func(i, j int) bool { return g.Unlinked[i].Title < g.Unlinked[j].Title })
		groups = append(groups, *g)
	}
	sort.Slice(groups, func(i, j int) bool {
		if groups[i].Source != groups[j].Source {
			return sourceOrder(groups[i].Source) < sourceOrder(groups[j].Source)
		}
		return groups[i].ContainerTitle < groups[j].ContainerTitle
	})

	resp["scannedAt"] = manifest.ScannedAt
	resp["previousScannedAt"] = manifest.PreviousScannedAt
	resp["groups"] = groups
	resp["ignoredEntries"] = ignoredEntries
	// 清单里的忽略字段带 omitempty，空时存进去就被省略、读回来是 nil——
	// API 形状要稳定：给前端永远是数组，不是 null（真实踩到：池子页 .length 崩）。
	ignoredGroupsOut := manifest.IgnoredGroups
	if ignoredGroupsOut == nil {
		ignoredGroupsOut = []domain.IgnoredGroup{}
	}
	resp["ignoredGroups"] = ignoredGroupsOut
	writeJSON(w, http.StatusOK, resp)
}

// handleLibraryIgnore 是 POST /api/library/ignore：
// {source, entryId} 忽略单条；{source, containerKey(, containerTitle)} 忽略整组。
func (s *Server) handleLibraryIgnore(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Source         string `json:"source"`
		EntryID        string `json:"entryId"`
		ContainerKey   string `json:"containerKey"`
		ContainerTitle string `json:"containerTitle"`
	}
	if err := decodeBody(r, &body); err != nil {
		writeErr(w, http.StatusBadRequest, "bad_json", err.Error())
		return
	}
	body.Source = strings.TrimSpace(body.Source)
	if body.Source != "emby" && body.Source != "komga" && body.Source != "gameatlas" {
		writeErr(w, http.StatusBadRequest, "bad_request", "source 必填（emby|komga|gameatlas）")
		return
	}
	entryID, containerKey := strings.TrimSpace(body.EntryID), strings.TrimSpace(body.ContainerKey)
	if entryID == "" && containerKey == "" {
		writeErr(w, http.StatusBadRequest, "bad_request", "entryId 或 containerKey 必填")
		return
	}
	// 读改写走 store 的原子 updater：清单还有另外两个写点（扫描、以及工具面的
	// ignore_library_entry），大家共用同一把锁，才不会出现"后写的吞掉先写的"。
	err := s.store.UpdateLibraryManifest(func(man *domain.LibraryManifest) error {
		if entryID != "" {
			key := body.Source + ":" + entryID
			if !containsString(man.IgnoredKeys, key) {
				man.IgnoredKeys = append(man.IgnoredKeys, key)
			}
			return nil
		}
		for _, g := range man.IgnoredGroups {
			if g.Source == body.Source && g.ContainerKey == containerKey {
				return nil
			}
		}
		man.IgnoredGroups = append(man.IgnoredGroups, domain.IgnoredGroup{
			Source: body.Source, ContainerKey: containerKey, ContainerTitle: strings.TrimSpace(body.ContainerTitle),
		})
		return nil
	})
	if err != nil {
		writeStoreErr(w, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"ok": true})
}

// handleLibraryUnignore 是 DELETE /api/library/ignore?source=&entryId=|containerKey=。
func (s *Server) handleLibraryUnignore(w http.ResponseWriter, r *http.Request) {
	source := strings.TrimSpace(r.URL.Query().Get("source"))
	entryID := strings.TrimSpace(r.URL.Query().Get("entryId"))
	containerKey := strings.TrimSpace(r.URL.Query().Get("containerKey"))
	if source == "" || (entryID == "" && containerKey == "") {
		writeErr(w, http.StatusBadRequest, "bad_request", "source 与 entryId/containerKey 必填")
		return
	}
	err := s.store.UpdateLibraryManifest(func(man *domain.LibraryManifest) error {
		if entryID != "" {
			key := source + ":" + entryID
			next := make([]string, 0, len(man.IgnoredKeys))
			for _, k := range man.IgnoredKeys {
				if k != key {
					next = append(next, k)
				}
			}
			man.IgnoredKeys = next
		}
		if containerKey != "" {
			next := make([]domain.IgnoredGroup, 0, len(man.IgnoredGroups))
			for _, g := range man.IgnoredGroups {
				if !(g.Source == source && g.ContainerKey == containerKey) {
					next = append(next, g)
				}
			}
			man.IgnoredGroups = next
		}
		return nil
	})
	if err != nil {
		writeStoreErr(w, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"ok": true})
}

func containsString(list []string, s string) bool {
	for _, v := range list {
		if v == s {
			return true
		}
	}
	return false
}
