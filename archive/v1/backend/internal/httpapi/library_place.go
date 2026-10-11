package httpapi

import (
	"net/http"
	"strings"

	"wikiatlas/backend/internal/domain"
)

// handleLibraryArchivePlace 是 POST /api/library/archive-place：
// 把一批库条目**按落位规则**建成"待写"，并挂上链。
//
// 为什么要有这个端点（而不是前端自己 createWork + link）：落位规则会**建节点**
// （系列 + 单作 + 介质后缀），错一次是一批错节点；放后端才有单测，
// 前端也没有测试基建。规则本身在 domain.Placer（纯函数）。
//
// 请求：{"items":[{"source":"emby","externalId":"80316"}], "parentId": ...}
//
//	parentId 省略      → 走规则：命中单作就关联；命中系列就在其下建子节点；都没有就"建系列+建单作+关联"
//	parentId = ""      → 「不归属」：直接在顶级建/复用节点（不建系列）
//	parentId = "<id>"  → 在你选的那个节点下建/复用子节点
func (s *Server) handleLibraryArchivePlace(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Items []struct {
			Source     string `json:"source"`
			ExternalID string `json:"externalId"`
		} `json:"items"`
		ParentID *string `json:"parentId"`
		// SeriesTitle 非空 = 先确保这个标题的 series 存在，整批都挂到它下面。
		// 这是"系列容器"的用法（GA/Komga 的容器本身就是作品系列，比如 丧尸围城1..4）——
		// 否则每条都会各自建一个系列，等于把系列拆散了。
		SeriesTitle string `json:"seriesTitle"`
	}
	if err := decodeBody(r, &body); err != nil {
		writeErr(w, http.StatusBadRequest, "bad_json", err.Error())
		return
	}
	if len(body.Items) == 0 {
		writeErr(w, http.StatusBadRequest, "bad_request", "items 必填")
		return
	}
	manifest, err := s.store.GetLibraryManifest()
	if err != nil {
		writeStoreErr(w, err)
		return
	}
	if manifest == nil {
		writeErr(w, http.StatusBadRequest, "no_manifest", "还没有扫描清单：先点「扫描媒体库」")
		return
	}
	entryByKey := make(map[string]domain.LibraryManifestEntry, len(manifest.Entries))
	for _, e := range manifest.Entries {
		entryByKey[e.Key] = e
	}
	nodes, err := s.store.ListTree()
	if err != nil {
		writeStoreErr(w, err)
		return
	}
	placer := domain.NewPlacer(nodes)

	// 系列容器：先把容器名对应的系列备好（没有就建），下面整批都用它当父节点。
	explicitParent := body.ParentID
	if st := strings.TrimSpace(body.SeriesTitle); st != "" {
		series, err := s.ensureSeries(st)
		if err != nil {
			writeStoreErr(w, err)
			return
		}
		explicitParent = &series.ID
	}

	type failedItem struct {
		ExternalID string `json:"externalId"`
		Title      string `json:"title"`
		Error      string `json:"error"`
	}
	created, linked, failed := 0, 0, make([]failedItem, 0)

	for _, item := range body.Items {
		key := strings.TrimSpace(item.Source) + ":" + strings.TrimSpace(item.ExternalID)
		entry, ok := entryByKey[key]
		if !ok {
			failed = append(failed, failedItem{item.ExternalID, "", "清单里没有这条"})
			continue
		}
		medium := domain.MediumOfEntry(entry.Source, entry.Kind, entry.Format)
		var plan domain.PlacementPlan
		switch {
		case explicitParent == nil:
			plan = placer.Plan(entry.Title, medium, "")
		case strings.TrimSpace(*explicitParent) == "":
			plan = placer.PlanTop(entry.Title, medium)
		default:
			plan = placer.Plan(entry.Title, medium, strings.TrimSpace(*explicitParent))
		}

		made, err := s.applyPlacement(placer, entry, plan)
		if err != nil {
			failed = append(failed, failedItem{item.ExternalID, entry.Title, err.Error()})
			continue
		}
		created += made
		linked++
	}

	writeJSON(w, http.StatusOK, map[string]any{
		"created": created, "linked": linked, "failed": failed,
	})
}

// applyPlacement 执行一条落位方案：必要时建系列/子节点，最后把链挂上。
// 返回（目标节点 id, 新建节点数, error）。新建的节点会 Register 回 placer——
// 同一批里的后续条目就能复用它们（幂等）。
func (s *Server) applyPlacement(
	placer *domain.Placer,
	entry domain.LibraryManifestEntry,
	plan domain.PlacementPlan,
) (int, error) {
	made := 0

	if plan.Action == domain.PlaceLink {
		return 0, s.attachEntryLink(entry, plan.TargetNodeID)
	}

	parentID := plan.ParentNodeID
	// 「建系列 + 建单作」：系列还不存在时先把它建出来。
	if plan.Action == domain.PlaceSeriesChild {
		series, err := s.store.CreateWork(domain.CreateWorkBody{
			Kind:   domain.WorkKindSeries,
			Title:  plan.SeriesTitle,
			Medium: &plan.Medium,
		})
		if err != nil {
			return made, err
		}
		made++
		placer.Register(domain.WorkSummary{
			ID: series.ID, Kind: series.Kind, Title: series.Title, Medium: series.Medium,
		})
		parentID = series.ID
	}

	var parentPtr *string
	if strings.TrimSpace(parentID) != "" {
		parentPtr = &parentID
	}
	node, err := s.store.CreateWork(domain.CreateWorkBody{
		ParentID: parentPtr,
		Kind:     domain.WorkKindWork,
		Title:    plan.NodeTitle,
		Medium:   &plan.Medium,
	})
	if err != nil {
		return made, err
	}
	made++
	placer.Register(domain.WorkSummary{
		ID: node.ID, ParentID: parentPtr, Kind: node.Kind, Title: node.Title, Medium: node.Medium,
	})

	if err := s.attachEntryLink(entry, node.ID); err != nil {
		// 挂链失败：把刚建的节点收回去，别留孤儿 stub（与单条建档同一取舍）。
		_ = s.store.DeleteWork(node.ID)
		return made, err
	}
	return made, nil
}

// ensureSeries 找到或创建"这个标题的系列"（用规范化标题找，含别名）。
func (s *Server) ensureSeries(title string) (*domain.Work, error) {
	nodes, err := s.store.ListTree()
	if err != nil {
		return nil, err
	}
	if n := domain.MatchNodeByPlacer(nodes, title); n != nil {
		return s.store.GetWork(n.ID)
	}
	m := domain.MediumOther
	return s.store.CreateWork(domain.CreateWorkBody{
		Kind: domain.WorkKindSeries, Title: title, Medium: &m,
	})
}

// attachEntryLink 把清单条目挂到节点上。
//
// url / 标题直接用**清单里扫描时取到的**，不再去外部客户端拉一次——
// 这条路径就不依赖 Emby/Komga/GameAtlas 此刻在线（扫描过期也不会因此建档失败）。
func (s *Server) attachEntryLink(entry domain.LibraryManifestEntry, workID string) error {
	var url *string
	if u := strings.TrimSpace(entry.URL); u != "" {
		url = &u
	}
	hint := entry.Title
	role := s.store.RoleForEntry(entry.Source, entry.EntryID)
	if role == "" {
		role = domain.LinkRoleWork
	}
	_, err := s.store.CreateLibraryLink(
		workID, domain.LibrarySource(entry.Source), entry.EntryID,
		url, &hint, role, domain.LinkByArchive,
	)
	return err
}
