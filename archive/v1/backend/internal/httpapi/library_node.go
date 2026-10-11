package httpapi

// 节点页「关联与媒体」的统一数据面：
//
//	GET /api/library/node/{workId}
//	    正片级链 / 混杂级链 / 待确认建议 / 已忽略 / 源状态，一个端点吐齐
//	    ——节点页不再分别打 sweep / related / suggest 五个请求，也没有
//	    前端标题启发式去重（建议 KV 里本就只有去重后的 key 集）。
//
//	POST /api/library/link {workId, source, publicId, origin}
//	    统一挂链入口（origin = sweep|pool|manual，落到 linked_by）。
//	    内部派发给各源既有 handler（查重/元数据/URL 逻辑不重复实现），
//	    origin 走 context 传下去。
//
//	PATCH /api/library/link {id, role}
//	    改判一条链的角色（work↔mix）：挂错了归级的修正口。

import (
	"context"
	"encoding/json"
	"net/http"
	"sort"
	"strings"

	"wikiatlas/backend/internal/domain"
)

type linkOriginKey struct{}

// linkOriginOf 读统一入口挂在 context 上的 origin；直接调各源端点时为 manual。
func linkOriginOf(r *http.Request) string {
	if v, ok := r.Context().Value(linkOriginKey{}).(string); ok && v != "" {
		return v
	}
	return domain.LinkByManual
}

// roleForEntry 的判定在 store（RoleForEntry）：Emby 按容器角色定 work/mix，
// GA / Komga 恒 work。这里只是包一层，调用方不用记包名。
func (s *Server) roleForEntry(source, entryID string) string {
	return s.store.RoleForEntry(source, entryID)
}

// manifestIndex 把清单拍成 key→entry，供链/建议补展示字段。
type manifestIndex struct {
	byKey map[string]domain.LibraryManifestEntry
}

func (s *Server) loadManifestIndex() *manifestIndex {
	idx := &manifestIndex{byKey: map[string]domain.LibraryManifestEntry{}}
	if m, err := s.store.GetLibraryManifest(); err == nil && m != nil {
		for _, e := range m.Entries {
			idx.byKey[e.Key] = e
		}
	}
	return idx
}

// linkDisplayOut 是节点页一条链的展示行：链本身 + 清单补的条目信息。
type linkDisplayOut struct {
	ID          string  `json:"id"`
	Source      string  `json:"source"`
	Role        string  `json:"role"`
	ExternalID  string  `json:"externalId"`
	URL         string  `json:"url,omitempty"`
	Title       string  `json:"title"`
	CoverImage  string  `json:"coverImage,omitempty"`
	Kind        string  `json:"kind,omitempty"`
	Format      string  `json:"format,omitempty"`
	ReleaseDate *string `json:"releaseDate,omitempty"`
	Extra       string  `json:"extra,omitempty"`
	LinkedBy    string  `json:"linkedBy,omitempty"`
	CreatedAt   string  `json:"createdAt"`
}

func linkDisplayOf(l domain.LibraryLink, idx *manifestIndex) linkDisplayOut {
	out := linkDisplayOut{
		ID: l.ID, Source: string(l.Source), ExternalID: l.ExternalID,
		Role: l.Role, LinkedBy: l.LinkedBy, CreatedAt: l.CreatedAt,
	}
	if l.URL != nil {
		out.URL = *l.URL
	}
	out.Title = l.ExternalID
	if l.TitleHint != nil && *l.TitleHint != "" {
		out.Title = *l.TitleHint
	}
	if e, ok := idx.byKey[string(l.Source)+":"+l.ExternalID]; ok {
		if e.Title != "" {
			out.Title = e.Title
		}
		out.CoverImage, out.Kind, out.Format, out.Extra = e.CoverImage, e.Kind, e.Format, e.Extra
		out.ReleaseDate = e.ReleaseDate
		if e.URL != "" && out.URL == "" {
			out.URL = e.URL
		}
	}
	return out
}

// nodePendingOut 是节点页一条「待确认」：目标就是本节点（action=link 挂本节点 /
// action=archive 在本节点下建档）。建议指向别节点的不在这里出现（去那边看/池子看）。
type nodePendingOut struct {
	Key          string  `json:"key"`
	Source       string  `json:"source"`
	EntryID      string  `json:"entryId"`
	Title        string  `json:"title"`
	Kind         string  `json:"kind,omitempty"`
	Format       string  `json:"format,omitempty"`
	Extra        string  `json:"extra,omitempty"`
	CoverImage   string  `json:"coverImage,omitempty"`
	URL          string  `json:"url,omitempty"`
	Role         string  `json:"role"`
	Action       string  `json:"action"`
	TargetNodeID string  `json:"targetNodeId"`
	Confidence   float64 `json:"confidence"`
	Reason       string  `json:"reason"`
	Origin       string  `json:"origin,omitempty"`
	// RejectedReason 非空 = 这条挂法被拒绝过、模型又提了（让用户看见复读，可再拒）。
	RejectedReason string `json:"rejectedReason,omitempty"`
}

type nodeSourceOut struct {
	Source  string `json:"source"`
	Status  string `json:"status"` // ok | unconfigured
	Message string `json:"message,omitempty"`
}

func (s *Server) handleNodeLibrary(w http.ResponseWriter, r *http.Request) {
	workID := strings.TrimSpace(r.PathValue("workId"))
	if workID == "" {
		writeErr(w, http.StatusBadRequest, "bad_request", "workId 必填")
		return
	}
	work, err := s.store.GetWork(workID)
	if err != nil {
		writeStoreErr(w, err)
		return
	}
	idx := s.loadManifestIndex()

	// 链：按角色分两堆。
	linksOut := map[string][]linkDisplayOut{"work": {}, "mix": {}}
	if links, err := s.store.ListLibraryLinksByWork(workID); err == nil {
		for _, l := range links {
			d := linkDisplayOf(l, idx)
			bucket := d.Role
			if bucket != domain.LinkRoleMix {
				bucket = domain.LinkRoleWork
			}
			linksOut[bucket] = append(linksOut[bucket], d)
		}
	}

	// 待确认：KV 建议里目标 = 本节点、未挂链、未被本节点忽略的。
	ai, _ := s.store.GetLibraryAiSuggestions()
	liveLinks := s.allLinkedIDs()
	dismissed := map[string]bool{}
	dismissedOut := make([]domain.LibraryAiDismissal, 0)
	pending := make([]nodePendingOut, 0)
	generatedAt, model := "", ""
	if ai != nil {
		generatedAt, model = ai.GeneratedAt, ai.Model
		for _, d := range ai.Dismissed {
			if d.WorkID == workID {
				dismissed[d.Key] = true
				dismissedOut = append(dismissedOut, d)
			}
		}
		seen := map[string]bool{} // 同 key 多条(不同节点扫过)只展示一条
		rej := domain.RejectionIndex(ai)
		for _, sug := range ai.Suggestions {
			if sug.TargetNodeID != workID || dismissed[sug.Key] || liveLinks[sug.Key] != "" {
				continue
			}
			if sug.Action != "link" && sug.Action != "archive" {
				continue
			}
			if seen[sug.Key] {
				continue
			}
			seen[sug.Key] = true
			p := nodePendingOut{
				Key: sug.Key, Source: sug.Source, Action: sug.Action,
				TargetNodeID: sug.TargetNodeID, Confidence: sug.Confidence,
				Reason: sug.Reason, Origin: sug.Origin,
			}
			if rj, hit := rej[domain.RejectionID(sug.Key, sug.Action, workID)]; hit {
				p.RejectedReason = rj.Reason
			}
			p.EntryID = strings.TrimPrefix(sug.Key, sug.Source+":")
			p.Role = sug.Role
			if p.Role == "" {
				p.Role = s.roleForEntry(sug.Source, p.EntryID)
			}
			if sug.Title != "" {
				p.Title, p.Kind, p.Format = sug.Title, sug.Kind, sug.Format
				p.Extra, p.CoverImage, p.URL = sug.Extra, sug.CoverImage, sug.URL
			} else if e, ok := idx.byKey[sug.Key]; ok {
				// 池子产出的建议不带展示字段，从清单补齐。
				p.Title, p.Kind, p.Format = e.Title, e.Kind, e.Format
				p.Extra, p.CoverImage, p.URL = e.Extra, e.CoverImage, e.URL
			}
			pending = append(pending, p)
		}
		sort.SliceStable(pending, func(i, j int) bool { return pending[i].Confidence > pending[j].Confidence })
	}

	// 源状态：只查配置，不轰库（提示「还没配置」级别的真话）。
	srcs := s.nodeSourceStatus()

	writeJSON(w, http.StatusOK, map[string]any{
		"workId": workID,
		"work": map[string]any{
			"id": work.ID, "title": work.Title, "kind": work.Kind,
			"medium": stringOf(work.Medium), "status": work.Status,
			"aliases": work.Aliases,
		},
		"links":       linksOut,
		"pending":     pending,
		"dismissed":   dismissedOut,
		"sources":     srcs,
		"generatedAt": generatedAt,
		"model":       model,
	})
}

func stringOf(m *domain.Medium) string {
	if m == nil {
		return ""
	}
	return string(*m)
}

// nodeSourceStatus：三源配置状态。配置以 settings 里存的 key 为准——
// 历史上设置页保存时不一定把 *ApiKeyConfigured 标记写进去（env 导入 /
// 旧版本保存路径），只信标记会把「配好了」误报成「没配置」。
func (s *Server) nodeSourceStatus() []nodeSourceOut {
	st, _ := s.store.GetSettings()
	lib := st.Library
	gaCfg := lib.GameAtlasAPIKey != nil && *lib.GameAtlasAPIKey != ""
	embyCfg := lib.EmbyAPIKey != nil && *lib.EmbyAPIKey != ""
	komgaCfg := lib.KomgaAPIKey != nil && *lib.KomgaAPIKey != ""
	out := make([]nodeSourceOut, 0, 3)
	if gaCfg {
		out = append(out, nodeSourceOut{Source: "gameatlas", Status: "ok"})
	} else {
		out = append(out, nodeSourceOut{Source: "gameatlas", Status: "unconfigured", Message: "还没配置 GameAtlas"})
	}
	if embyCfg {
		if len(lib.EmbyLibraryRoles) == 0 {
			out = append(out, nodeSourceOut{Source: "emby", Status: "unconfigured", Message: "库角色还没标：到设置页把正片 / 混杂内容库选上"})
		} else {
			out = append(out, nodeSourceOut{Source: "emby", Status: "ok"})
		}
	} else {
		out = append(out, nodeSourceOut{Source: "emby", Status: "unconfigured", Message: "还没配置 Emby"})
	}
	if komgaCfg {
		out = append(out, nodeSourceOut{Source: "komga", Status: "ok"})
	} else {
		out = append(out, nodeSourceOut{Source: "komga", Status: "unconfigured", Message: "还没配置 Komga"})
	}
	return out
}

// handleUnifiedLink 是 POST /api/library/link：统一挂链入口。
func (s *Server) handleUnifiedLink(w http.ResponseWriter, r *http.Request) {
	var body struct {
		WorkID   string `json:"workId"`
		Source   string `json:"source"`
		PublicID string `json:"publicId"`
		Origin   string `json:"origin"`
	}
	if err := decodeBody(r, &body); err != nil {
		writeErr(w, http.StatusBadRequest, "bad_json", err.Error())
		return
	}
	body.Source = strings.TrimSpace(body.Source)
	switch body.Source {
	case "emby", "komga", "gameatlas":
	default:
		writeErr(w, http.StatusBadRequest, "bad_request", "source 必须是 emby|komga|gameatlas")
		return
	}
	switch body.Origin {
	case "", domain.LinkBySweep, domain.LinkByPool, domain.LinkByManual:
	default:
		body.Origin = domain.LinkByManual
	}
	// 派发给各源既有 handler（查重 / 元数据 / URL 都在那里）；origin 走 context。
	// 各源 handler 自己解 {workId, publicId}，这里把统一请求体拆回去。
	payload, _ := json.Marshal(map[string]any{"workId": body.WorkID, "publicId": body.PublicID})
	r2, err := http.NewRequestWithContext(
		context.WithValue(r.Context(), linkOriginKey{}, body.Origin),
		r.Method, r.URL.RequestURI(), strings.NewReader(string(payload)))
	if err != nil {
		writeErr(w, http.StatusInternalServerError, "internal", err.Error())
		return
	}
	r2.Header.Set("Content-Type", "application/json")
	switch body.Source {
	case "emby":
		s.handleEmbyLink(w, r2)
	case "komga":
		s.handleKomgaLink(w, r2)
	case "gameatlas":
		s.handleGameAtlasLink(w, r2)
	}
}

// handleLinkRolePatch 是 PATCH /api/library/link：改判链的角色（work↔mix）。
func (s *Server) handleLinkRolePatch(w http.ResponseWriter, r *http.Request) {
	var body struct {
		ID   string `json:"id"`
		Role string `json:"role"`
	}
	if err := decodeBody(r, &body); err != nil {
		writeErr(w, http.StatusBadRequest, "bad_json", err.Error())
		return
	}
	body.ID = strings.TrimSpace(body.ID)
	if body.ID == "" {
		writeErr(w, http.StatusBadRequest, "bad_request", "id 必填")
		return
	}
	if err := s.store.SetLibraryLinkRole(body.ID, strings.TrimSpace(body.Role)); err != nil {
		writeStoreErr(w, err)
		return
	}
	l, err := s.store.GetLibraryLink(body.ID)
	if err != nil {
		writeStoreErr(w, err)
		return
	}
	idx := s.loadManifestIndex()
	writeJSON(w, http.StatusOK, map[string]any{"ok": true, "link": linkDisplayOf(*l, idx)})
}
