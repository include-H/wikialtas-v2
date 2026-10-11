package store

import (
	"database/sql"
	"fmt"
	"strings"

	"wikiatlas/backend/internal/domain"
)

// CreateRelation inserts a typed edge between works.
func (s *Store) CreateRelation(body domain.CreateRelationBody) (*domain.Relation, error) {
	if body.FromID == "" || body.ToID == "" {
		return nil, ErrValidation{Message: "fromId and toId are required"}
	}
	if body.FromID == body.ToID {
		return nil, ErrValidation{Message: "cannot relate a work to itself"}
	}
	if !domain.ValidRelationTypes[body.Type] {
		return nil, ErrValidation{Message: "invalid relation type; see frozen dictionary"}
	}
	// both ends must exist
	for _, id := range []string{body.FromID, body.ToID} {
		var n int
		if err := s.DB.QueryRow(`SELECT COUNT(*) FROM works WHERE id = ?`, id).Scan(&n); err != nil {
			return nil, err
		}
		if n == 0 {
			return nil, ErrValidation{Message: "work not found: " + id}
		}
	}
	// 环检查：方向性关系（改编/续作/衍生/重制/扩充）合起来必须是 DAG。
	// adapter §3 一直写着"不允许环"，但代码从没查过——结果是 A→B 与 B→A 可以
	// 同时存在，读取侧看到两句互相矛盾的话（"A 是 B 的续作" / "B 是 A 的续作"）。
	// `same_series` / `references` 天然互相，不参与（见 domain.DirectionalRelationTypes）。
	if domain.DirectionalRelationTypes[body.Type] {
		reaches, hops, err := s.relationReaches(body.ToID, body.FromID)
		if err != nil {
			return nil, err
		}
		if reaches {
			return nil, ErrValidation{Message: s.cycleMessage(hops)}
		}
	}
	id := NewID()
	now := Now()
	_, err := s.DB.Exec(`INSERT INTO relations (id, from_id, to_id, type, created_at) VALUES (?, ?, ?, ?, ?)`,
		id, body.FromID, body.ToID, string(body.Type), now)
	if err != nil {
		// unique violation
		return nil, ErrConflict{Message: "relation already exists"}
	}
	return s.GetRelation(id)
}

// CreateRelationsBatch 是 upsert_relations 工具的后端：一批边 = 一次性事务。
// 与 patch_sections 同一纪律：预检全在前，任一条不过**整批都不落**。
// 单条规则与 CreateRelation 完全一致（端点存在/无自环/词表冻结/DAG 查环/
// 三元组唯一），差别只在环检查还要叠上本批已验证的前几条边——
// 批内 A→B、B→C 再接 C→A 也是环。
func (s *Store) CreateRelationsBatch(bodies []domain.CreateRelationBody) ([]domain.Relation, error) {
	if len(bodies) == 0 {
		return nil, ErrValidation{Message: "relations 为空"}
	}
	if len(bodies) > 20 {
		return nil, ErrValidation{Message: "单次最多 20 条"}
	}
	// 批内重复三元组 + 端点收集
	seenInBatch := map[string]bool{}
	endpoints := map[string]bool{}
	for i, b := range bodies {
		if b.FromID == "" || b.ToID == "" {
			return nil, ErrValidation{Message: fmt.Sprintf("第 %d 条缺 fromId/toId", i+1)}
		}
		if b.FromID == b.ToID {
			return nil, ErrValidation{Message: fmt.Sprintf("第 %d 条：节点不能关联自己", i+1)}
		}
		if !domain.ValidRelationTypes[b.Type] {
			return nil, ErrValidation{Message: fmt.Sprintf("第 %d 条 type 不在冻结词表：adaptation_of|sequel_to|spin_off_of|remake_of|expansion_of|references|same_series", i+1)}
		}
		k := b.FromID + "|" + b.ToID + "|" + string(b.Type)
		if seenInBatch[k] {
			return nil, ErrValidation{Message: fmt.Sprintf("第 %d 条与前面某条重复（from/to/type 三元组相同）", i+1)}
		}
		seenInBatch[k] = true
		endpoints[b.FromID] = true
		endpoints[b.ToID] = true
	}
	// 端点存在性（一次查完）
	for id := range endpoints {
		var n int
		if err := s.DB.QueryRow(`SELECT COUNT(*) FROM works WHERE id = ?`, id).Scan(&n); err != nil {
			return nil, err
		}
		if n == 0 {
			return nil, ErrValidation{Message: "work not found: " + id}
		}
	}
	// 逐条全量预检（against DB + 批内已收的边），全部通过才开事务写。
	var plan []domain.CreateRelationBody
	var batchDirected []relHop // 批内方向性边（供后面的边查环）
	for i, b := range bodies {
		var n int
		if err := s.DB.QueryRow(`SELECT COUNT(*) FROM relations WHERE from_id = ? AND to_id = ? AND type = ?`, b.FromID, b.ToID, string(b.Type)).Scan(&n); err != nil {
			return nil, err
		}
		if n > 0 {
			return nil, ErrConflict{Message: fmt.Sprintf("第 %d 条边已存在（from/to/type 相同）", i+1)}
		}
		if domain.DirectionalRelationTypes[b.Type] {
			// DB 侧：已有方向性链让 to 绕回 from
			if reaches, hops, err := s.relationReaches(b.ToID, b.FromID); err != nil {
				return nil, err
			} else if reaches {
				return nil, ErrValidation{Message: fmt.Sprintf("第 %d 条%s", i+1, s.cycleMessage(hops))}
			}
			// 批内侧：本批前面的方向性边叠上去（同 DB 侧方向：To 绕回 From 才成环）
			if _, hops2, ok := reachesViaBatch(b.ToID, b.FromID, batchDirected); ok {
				return nil, ErrValidation{Message: fmt.Sprintf("第 %d 条会与本批前面的边成环：%s", i+1, batchHopMessage(hops2))}
			}
			batchDirected = append(batchDirected, relHop{From: b.FromID, To: b.ToID, Type: string(b.Type)})
		}
		plan = append(plan, b)
	}
	tx, err := s.DB.Begin()
	if err != nil {
		return nil, err
	}
	defer tx.Rollback()
	out := make([]domain.Relation, 0, len(plan))
	for _, b := range plan {
		id := NewID()
		if _, err := tx.Exec(`INSERT INTO relations (id, from_id, to_id, type, created_at) VALUES (?, ?, ?, ?, ?)`,
			id, b.FromID, b.ToID, string(b.Type), Now()); err != nil {
			return nil, err
		}
		out = append(out, domain.Relation{
			ID: id, FromID: b.FromID, ToID: b.ToID,
			Type: b.Type, CreatedAt: Now(),
		})
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return out, nil
}

// reachesViaBatch 在"批内已收方向性边"的有向图上查 from 能否走到 target。
func reachesViaBatch(from, target string, edges []relHop) (string, []relHop, bool) {
	if from == target {
		return "", nil, true
	}
	adj := map[string][]relHop{}
	for _, e := range edges {
		adj[e.From] = append(adj[e.From], e)
	}
	seen := map[string]bool{from: true}
	queue := []string{from}
	parent := map[string]relHop{}
	for len(queue) > 0 {
		cur := queue[0]
		queue = queue[1:]
		for _, e := range adj[cur] {
			if e.To == target {
				hops := pathUp(parent, from, cur)
				hops = append(hops, e)
				return cur, hops, true
			}
			if !seen[e.To] {
				seen[e.To] = true
				parent[e.To] = e
				queue = append(queue, e.To)
			}
		}
	}
	return "", nil, false
}

func batchHopMessage(hops []relHop) string {
	var b strings.Builder
	b.WriteString("「")
	b.WriteString(hops[0].From)
	for _, h := range hops {
		fmt.Fprintf(&b, "」 —(%s)→ 「%s", h.Type, h.To)
	}
	b.WriteString("」")
	return b.String()
}

// GetRelation returns one relation.
func (s *Store) GetRelation(id string) (*domain.Relation, error) {
	var r domain.Relation
	err := s.DB.QueryRow(`SELECT id, from_id, to_id, type, created_at FROM relations WHERE id = ?`, id).
		Scan(&r.ID, &r.FromID, &r.ToID, &r.Type, &r.CreatedAt)
	if err == sql.ErrNoRows {
		return nil, ErrNotFound{What: "relation"}
	}
	return &r, err
}

// ListRelationsByWork returns edges touching a work (either direction).
func (s *Store) ListRelationsByWork(workID string) ([]domain.Relation, error) {
	rows, err := s.DB.Query(`SELECT id, from_id, to_id, type, created_at FROM relations
		WHERE from_id = ? OR to_id = ? ORDER BY created_at`, workID, workID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := make([]domain.Relation, 0)
	for rows.Next() {
		var r domain.Relation
		if err := rows.Scan(&r.ID, &r.FromID, &r.ToID, &r.Type, &r.CreatedAt); err != nil {
			return nil, err
		}
		out = append(out, r)
	}
	return out, rows.Err()
}

// DeleteRelationByTriple removes one relation edge located by (from, to, type).
// 模型看不到关系边 id（没有任何工具把它端出去），三元组是它能拼出的唯一地址。
func (s *Store) DeleteRelationByTriple(fromID, toID string, t domain.RelationType) error {
	res, err := s.DB.Exec(`DELETE FROM relations WHERE from_id = ? AND to_id = ? AND type = ?`,
		fromID, toID, string(t))
	if err != nil {
		return err
	}
	n, _ := res.RowsAffected()
	if n == 0 {
		return ErrNotFound{What: "relation"}
	}
	return nil
}

// CreateLibraryLink attaches an external library reference. Role is the link's
// role (work=正片级 / mix=混杂级), LinkedBy its provenance (sweep/pool/manual/archive).
func (s *Store) CreateLibraryLink(workID string, source domain.LibrarySource, externalID string, url, titleHint *string, role, linkedBy string) (*domain.LibraryLink, error) {
	switch source {
	case domain.LibraryEmby, domain.LibraryKomga, domain.LibraryGameAtlas:
	default:
		return nil, ErrValidation{Message: "source must be emby|komga|gameatlas"}
	}
	if externalID == "" {
		return nil, ErrValidation{Message: "externalId is required"}
	}
	var n int
	if err := s.DB.QueryRow(`SELECT COUNT(*) FROM works WHERE id = ?`, workID).Scan(&n); err != nil {
		return nil, err
	}
	if n == 0 {
		return nil, ErrNotFound{What: "work"}
	}
	if role == "" {
		role = domain.LinkRoleWork
	}
	if linkedBy == "" {
		linkedBy = domain.LinkByManual
	}
	id := NewID()
	now := Now()
	_, err := s.DB.Exec(`INSERT INTO library_links (id, work_id, source, external_id, url, title_hint, role, linked_by, created_at)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		id, workID, string(source), externalID, nullStr(url), nullStr(titleHint), role, linkedBy, now)
	if err != nil {
		return nil, ErrConflict{Message: "library link already exists for this source+externalId"}
	}
	return s.GetLibraryLink(id)
}

// GetLibraryLink returns one link.
const libraryLinkCols = `id, work_id, source, external_id, url, title_hint, role, linked_by, created_at`

func (s *Store) GetLibraryLink(id string) (*domain.LibraryLink, error) {
	var (
		l       domain.LibraryLink
		url, th sql.NullString
	)
	err := s.DB.QueryRow(`SELECT `+libraryLinkCols+` FROM library_links WHERE id = ?`, id).
		Scan(&l.ID, &l.WorkID, &l.Source, &l.ExternalID, &url, &th, &l.Role, &l.LinkedBy, &l.CreatedAt)
	if err == sql.ErrNoRows {
		return nil, ErrNotFound{What: "library_link"}
	}
	if err != nil {
		return nil, err
	}
	if url.Valid {
		l.URL = &url.String
	}
	if th.Valid {
		l.TitleHint = &th.String
	}
	return &l, nil
}

// ListLibraryLinksByWork lists links for a work.
func (s *Store) ListLibraryLinksByWork(workID string) ([]domain.LibraryLink, error) {
	rows, err := s.DB.Query(`SELECT `+libraryLinkCols+` FROM library_links WHERE work_id = ? ORDER BY source, created_at`, workID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := make([]domain.LibraryLink, 0)
	for rows.Next() {
		var (
			l       domain.LibraryLink
			url, th sql.NullString
		)
		if err := rows.Scan(&l.ID, &l.WorkID, &l.Source, &l.ExternalID, &url, &th, &l.Role, &l.LinkedBy, &l.CreatedAt); err != nil {
			return nil, err
		}
		if url.Valid {
			l.URL = &url.String
		}
		if th.Valid {
			l.TitleHint = &th.String
		}
		out = append(out, l)
	}
	return out, rows.Err()
}

// ListLibraryLinksBySource lists all links from one source (e.g. gameatlas)。
// 建档建议用它排除"已经挂过链"的条目。
func (s *Store) ListLibraryLinksBySource(source domain.LibrarySource) ([]domain.LibraryLink, error) {
	rows, err := s.DB.Query(`SELECT `+libraryLinkCols+` FROM library_links WHERE source = ? ORDER BY created_at`, string(source))
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := make([]domain.LibraryLink, 0)
	for rows.Next() {
		var (
			l       domain.LibraryLink
			url, th sql.NullString
		)
		if err := rows.Scan(&l.ID, &l.WorkID, &l.Source, &l.ExternalID, &url, &th, &l.Role, &l.LinkedBy, &l.CreatedAt); err != nil {
			return nil, err
		}
		if url.Valid {
			l.URL = &url.String
		}
		if th.Valid {
			l.TitleHint = &th.String
		}
		out = append(out, l)
	}
	return out, rows.Err()
}

// RoleForEntry 按扫描清单给条目定角色：Emby 条目看容器（视图）角色——
// 混杂库 = mix，正片库 = work；GA / Komga 库里天然是作品本体 = work。
// 清单里没有（没扫过 / 条目已删）退回 work：链是有意动作或过了判官的，
// 默认正文级。httpapi 与 tools 共用这一份判定。
func (s *Store) RoleForEntry(source, entryID string) string {
	m, err := s.GetLibraryManifest()
	if err != nil || m == nil {
		return domain.LinkRoleWork
	}
	key := source + ":" + entryID
	for i := range m.Entries {
		if m.Entries[i].Key == key {
			if source == string(domain.LibraryEmby) && m.Entries[i].ContainerKind == domain.EmbyLibraryRoleMixed {
				return domain.LinkRoleMix
			}
			return domain.LinkRoleWork
		}
	}
	return domain.LinkRoleWork
}

// SetLibraryLinkRole reclassifies a link between work (正片级) and mix (混杂级).
func (s *Store) SetLibraryLinkRole(id, role string) error {
	switch role {
	case domain.LinkRoleWork, domain.LinkRoleMix:
	default:
		return ErrValidation{Message: "role must be work|mix"}
	}
	res, err := s.DB.Exec(`UPDATE library_links SET role = ? WHERE id = ?`, role, id)
	if err != nil {
		return err
	}
	n, _ := res.RowsAffected()
	if n == 0 {
		return ErrNotFound{What: "library_link"}
	}
	return nil
}

// DeleteLibraryLink removes one link by id.
func (s *Store) DeleteLibraryLink(id string) error {
	res, err := s.DB.Exec(`DELETE FROM library_links WHERE id = ?`, id)
	if err != nil {
		return err
	}
	n, _ := res.RowsAffected()
	if n == 0 {
		return ErrNotFound{What: "library_link"}
	}
	return nil
}

// ArchiveMeta 是建档要写进新节点与链的元数据，由调用方提供：
// httpapi 从外部客户端拉、internal/tools 从扫描清单（manifest）拉——
// store 不认识外部库，别把客户端塞进这一层。
type ArchiveMeta struct {
	Title  string
	Medium domain.Medium
	URL    *string
}

// ArchiveLibraryEntry 建一个 stub 节点，并在同一次调用里把媒体条目挂上去；
// 挂链失败时把刚建的节点收回去，绝不留下没有链的孤儿 stub。
//
// 为什么下沉到这里：httpapi 的三源 handler 各写了一遍同样的
// 「查重 → CreateWork → CreateLibraryLink → 失败回滚」，而 internal/tools 够不着
// handler——于是模型永远建不了档，只能让用户去点按钮。抽到这里，两条路径同一实现。
//
// **兄弟位语义**（原来在 httpapi 的 archiveAnchor）：承载节点是单作且有父 →
// 上提到父节点，否则直接下挂。建档进的是"同系列另一部"，不是当前这部作品的子节点；
// 界面上建档与模型建档必须落在同一个位置，所以这条规则不能各写一份。
func (s *Store) ArchiveLibraryEntry(source domain.LibrarySource, entryID, parentNodeID string,
	meta ArchiveMeta) (*domain.Work, *domain.LibraryLink, error) {
	entryID = strings.TrimSpace(entryID)
	parentNodeID = strings.TrimSpace(parentNodeID)
	title := strings.TrimSpace(meta.Title)
	if entryID == "" {
		return nil, nil, ErrValidation{Message: "entryID 必填"}
	}
	// parentNodeID 空 = 建在**顶级**（界面的「（不归属）」选项）。
	if title == "" {
		return nil, nil, ErrValidation{Message: "这条没有标题：先刷新媒体库清单再建档"}
	}

	// 查重：这条已经挂过链就别再建一个重复节点。
	if links, err := s.ListLibraryLinksBySource(source); err == nil {
		for _, l := range links {
			if l.ExternalID == entryID {
				return nil, nil, ErrValidation{Message: "这条已经挂过链（应该已经建过档了）"}
			}
		}
	}

	var parentID *string
	if parentNodeID != "" {
		parent, err := s.GetWork(parentNodeID)
		if err != nil {
			return nil, nil, err
		}
		// 兄弟位语义：承载节点是单作且有父 → 上提到父节点，否则直接下挂。
		anchor := parent
		if parent.Kind == domain.WorkKindWork && parent.ParentID != nil {
			if gp, err := s.GetWork(*parent.ParentID); err == nil {
				anchor = gp
			}
		}
		parentID = &anchor.ID
	}

	medium := meta.Medium
	child, err := s.CreateWork(domain.CreateWorkBody{
		ParentID: parentID,
		Kind:     domain.WorkKindWork,
		Medium:   &medium,
		Title:    title,
	})
	if err != nil {
		return nil, nil, err
	}
	hint := title
	link, err := s.CreateLibraryLink(child.ID, source, entryID, meta.URL, &hint,
		domain.LinkRoleWork, domain.LinkByArchive)
	if err != nil {
		// 并发下挂链失败：把刚建的节点收回去（外部条目的唯一约束在 DB 上兜底）。
		_ = s.DeleteWork(child.ID)
		return nil, nil, err
	}
	return child, link, nil
}

// relHop 是环检查里"一段已有的方向性边"。
type relHop struct{ From, To, Type string }

// relationReaches 判断有向图上 from 能否走到 target——**只走方向性关系**，并带出
// 走到时的最短路径（BFS 天然最短）。建边前用它查环：新边 from→to 若让 to 能绕回
// from，就成环；路径给报错用——模型看不见边，错误必须把"撞上了什么"说全。
func (s *Store) relationReaches(from, target string) (bool, []relHop, error) {
	seen := map[string]bool{}
	inQueue := map[string]bool{from: true}
	parent := map[string]relHop{}
	queue := []string{from}
	for len(queue) > 0 {
		cur := queue[0]
		queue = queue[1:]
		if cur == target {
			return true, pathUp(parent, from, target), nil
		}
		if seen[cur] {
			continue
		}
		seen[cur] = true
		rows, err := s.DB.Query(`SELECT to_id, type FROM relations WHERE from_id = ?`, cur)
		if err != nil {
			return false, nil, err
		}
		for rows.Next() {
			var next, typ string
			if err := rows.Scan(&next, &typ); err != nil {
				rows.Close()
				return false, nil, err
			}
			// 对称关系不算"走得到"：same_series/references 各自成对出现是正常的
			if domain.DirectionalRelationTypes[domain.RelationType(typ)] && !inQueue[next] {
				inQueue[next] = true
				parent[next] = relHop{From: cur, To: next, Type: typ}
				queue = append(queue, next)
			}
		}
		rows.Close()
	}
	return false, nil, nil
}

// pathUp 从 parent 表逆推 start→target 的路径（BFS 保证最短）。
func pathUp(parent map[string]relHop, start, target string) []relHop {
	var hops []relHop
	for node := target; node != start; {
		h, ok := parent[node]
		if !ok {
			return nil
		}
		hops = append([]relHop{h}, hops...)
		node = h.From
	}
	return hops
}

// cycleMessage 把环错误变成可执行指令。模型看不见边：
// 直接反向边 → 报出现有边的类型与方向，指 delete_relation（按现有删、再建）；
// 多跳链 → 整条链摆出来，明说工具拆不动（模型枚举不出多跳路径），别让它撞墙。
func (s *Store) cycleMessage(hops []relHop) string {
	title := func(id string) string {
		if w, err := s.GetWork(id); err == nil && w != nil {
			return w.Title
		}
		return id
	}
	if len(hops) == 1 {
		h := hops[0]
		return fmt.Sprintf("这条边会成环：「%s」 —(%s)→ 「%s」 已存在（直接反向边）。先用 delete_relation 按这条边现有的 fromId/toId/type 把它删掉，再建这条。",
			title(h.From), h.Type, title(h.To))
	}
	var b strings.Builder
	b.WriteString("这条边会成环，已有方向性关系链：「")
	b.WriteString(title(hops[0].From))
	for _, h := range hops {
		fmt.Fprintf(&b, "」 —(%s)→ 「%s", h.Type, title(h.To))
	}
	b.WriteString("」。delete_relation 一次只能删一条边，这条链拆不动——告诉用户需要到界面手工处理。")
	return b.String()
}
