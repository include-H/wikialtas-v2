package store

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"strings"

	"wikiatlas/backend/internal/domain"
)

func scanWork(row interface{ Scan(...any) error }) (*domain.Work, error) {
	var (
		w         domain.Work
		parentID  sql.NullString
		medium    sql.NullString
		aliases   string
		tags      string
		contentMd sql.NullString
	)
	err := row.Scan(&w.ID, &parentID, &w.Kind, &medium, &w.Title,
		&aliases, &tags, &contentMd, &w.ContentVer, &w.Status, &w.Visibility, &w.SortOrder,
		&w.CreatedAt, &w.UpdatedAt)
	if err != nil {
		return nil, err
	}
	if parentID.Valid {
		w.ParentID = &parentID.String
	}
	if medium.Valid && medium.String != "" {
		m := domain.Medium(medium.String)
		w.Medium = &m
	}
	_ = json.Unmarshal([]byte(aliases), &w.Aliases)
	if w.Aliases == nil {
		w.Aliases = []string{}
	}
	_ = json.Unmarshal([]byte(tags), &w.Tags)
	if w.Tags == nil {
		w.Tags = []string{}
	}
	if contentMd.Valid {
		w.ContentMd = &contentMd.String
	}
	return &w, nil
}

const workCols = `id, parent_id, kind, medium, title, aliases_json, tags_json, content_md, content_ver, status, visibility, sort_order, created_at, updated_at`

// CreateWork inserts a new work node.
func (s *Store) CreateWork(body domain.CreateWorkBody) (*domain.Work, error) {
	if body.Title == "" {
		return nil, ErrValidation{Message: "title is required"}
	}
	switch body.Kind {
	case domain.WorkKindUniverse, domain.WorkKindSeries, domain.WorkKindWork:
	default:
		return nil, ErrValidation{Message: "kind must be universe|series|work"}
	}
	id := NewID()
	now := Now()
	aliasesJSON, tagsJSON := "[]", "[]"
	if body.Aliases != nil {
		b, _ := json.Marshal(*body.Aliases)
		aliasesJSON = string(b)
	}
	if body.Tags != nil {
		b, _ := json.Marshal(*body.Tags)
		tagsJSON = string(b)
	}
	var parent any
	if body.ParentID != nil && *body.ParentID != "" {
		// verify parent exists
		var n int
		if err := s.DB.QueryRow(`SELECT COUNT(*) FROM works WHERE id = ?`, *body.ParentID).Scan(&n); err != nil {
			return nil, err
		}
		if n == 0 {
			return nil, ErrValidation{Message: "parent not found"}
		}
		parent = *body.ParentID
	}
	var medium any
	if body.Medium != nil {
		medium = string(*body.Medium)
	}
	visibility := domain.VisibilityPrivate
	if body.Visibility != nil && (*body.Visibility == domain.VisibilityPublic || *body.Visibility == domain.VisibilityPrivate) {
		visibility = *body.Visibility
	} else if v := s.NewNodeVisibility(); v != "" {
		visibility = v
	}
	_, err := s.DB.Exec(`INSERT INTO works (id, parent_id, kind, medium, title, aliases_json, tags_json, content_md, content_ver, status, visibility, sort_order, created_at, updated_at)
		VALUES (?, ?, ?, ?, ?, ?, ?, NULL, 0, 'stub', ?, 0, ?, ?)`,
		id, parent, string(body.Kind), medium, body.Title, aliasesJSON, tagsJSON, string(visibility), now, now)
	if err != nil {
		return nil, err
	}
	return s.GetWork(id)
}

// GetWork returns one work by id.
func (s *Store) GetWork(id string) (*domain.Work, error) {
	w, err := scanWork(s.DB.QueryRow(`SELECT `+workCols+` FROM works WHERE id = ?`, id))
	if err == sql.ErrNoRows {
		return nil, ErrNotFound{What: "work"}
	}
	return w, err
}

// PatchWork applies partial updates.
func (s *Store) PatchWork(id string, body domain.PatchWorkBody) (*domain.Work, error) {
	w, err := s.GetWork(id)
	if err != nil {
		return nil, err
	}
	now := Now()
	if body.Title != nil {
		if *body.Title == "" {
			return nil, ErrValidation{Message: "title cannot be empty"}
		}
		if _, err := s.DB.Exec(`UPDATE works SET title = ?, updated_at = ? WHERE id = ?`, *body.Title, now, id); err != nil {
			return nil, err
		}
	}
	if body.ParentID != nil {
		if *body.ParentID == id {
			return nil, ErrValidation{Message: "work cannot be its own parent"}
		}
		if *body.ParentID != "" {
			var n int
			if err := s.DB.QueryRow(`SELECT COUNT(*) FROM works WHERE id = ?`, *body.ParentID).Scan(&n); err != nil {
				return nil, err
			}
			if n == 0 {
				return nil, ErrValidation{Message: "parent not found"}
			}
			// 禁止把节点移到自己的子树里（否则树会成环，UI 与工具都会迷路）
			isDescendant, err := s.isDescendantOf(*body.ParentID, id)
			if err != nil {
				return nil, err
			}
			if isDescendant {
				return nil, ErrValidation{Message: "cannot move a node into its own subtree"}
			}
		}
		var parent any
		if *body.ParentID != "" {
			parent = *body.ParentID
		}
		if _, err := s.DB.Exec(`UPDATE works SET parent_id = ?, updated_at = ? WHERE id = ?`, parent, now, id); err != nil {
			return nil, err
		}
	}
	if body.Medium != nil {
		var medium any
		if *body.Medium != "" {
			medium = string(*body.Medium)
		}
		if _, err := s.DB.Exec(`UPDATE works SET medium = ?, updated_at = ? WHERE id = ?`, medium, now, id); err != nil {
			return nil, err
		}
	}
	if body.Status != nil {
		switch *body.Status {
		case domain.WorkStatusStub, domain.WorkStatusDraft, domain.WorkStatusReady, domain.WorkStatusArchived:
		default:
			return nil, ErrValidation{Message: "status must be stub|draft|ready|archived"}
		}
		if _, err := s.DB.Exec(`UPDATE works SET status = ?, updated_at = ? WHERE id = ?`, string(*body.Status), now, id); err != nil {
			return nil, err
		}
	}
	if body.Visibility != nil {
		if *body.Visibility != domain.VisibilityPublic && *body.Visibility != domain.VisibilityPrivate {
			return nil, ErrValidation{Message: "visibility must be public|private"}
		}
		if _, err := s.DB.Exec(`UPDATE works SET visibility = ?, updated_at = ? WHERE id = ?`, string(*body.Visibility), now, id); err != nil {
			return nil, err
		}
	}
	if body.SortOrder != nil {
		if _, err := s.DB.Exec(`UPDATE works SET sort_order = ?, updated_at = ? WHERE id = ?`, *body.SortOrder, now, id); err != nil {
			return nil, err
		}
	}
	if body.Aliases != nil {
		b, _ := json.Marshal(*body.Aliases)
		if _, err := s.DB.Exec(`UPDATE works SET aliases_json = ?, updated_at = ? WHERE id = ?`, string(b), now, id); err != nil {
			return nil, err
		}
	}
	if body.Tags != nil {
		b, _ := json.Marshal(*body.Tags)
		if _, err := s.DB.Exec(`UPDATE works SET tags_json = ?, updated_at = ? WHERE id = ?`, string(b), now, id); err != nil {
			return nil, err
		}
	}
	_ = w
	return s.GetWork(id)
}

// ListWorks 按结构字段筛节点（list_works 工具与 GET /api/list-works 共用）。
// 语义：Status 省略 = 只看"活着的库"（排除 archived）；显式 status=archived 查归档。
// 空结果一律返回空切片（前端 .length 直用）。
func (s *Store) ListWorks(f domain.ListWorksFilter) ([]domain.WorkSummary, error) {
	where := []string{"1=1"}
	args := []any{}
	if f.Kind != "" {
		where = append(where, "kind = ?")
		args = append(args, f.Kind)
	}
	if f.Medium != "" {
		where = append(where, "medium = ?")
		args = append(args, f.Medium)
	}
	switch f.Status {
	case "":
		where = append(where, `status != 'archived'`)
	default:
		where = append(where, "status = ?")
		args = append(args, f.Status)
	}
	if f.HasContent != nil {
		expr := "content_md IS NOT NULL AND content_md != ''"
		if !*f.HasContent {
			expr = "(content_md IS NULL OR content_md = '')"
		}
		where = append(where, expr)
	}
	limit := f.Limit
	if limit <= 0 {
		limit = 100
	}
	if limit > 200 {
		limit = 200
	}
	// workSummaryQuery 自带 WHERE status != 'archived' 与 ORDER BY；
	// 归档查询走这里的分支（显式 status=archived），其余筛并进去。
	var query string
	if f.Status == "archived" {
		query = `
	SELECT w.id, w.parent_id, w.kind, w.medium, w.title, w.aliases_json, w.status, w.visibility, w.sort_order, w.updated_at,
	       CASE WHEN w.content_md IS NOT NULL AND w.content_md != '' THEN 1 ELSE 0 END AS has_content,
	       CASE WHEN EXISTS (SELECT 1 FROM library_links l WHERE l.work_id = w.id) THEN 1 ELSE 0 END AS has_link
	FROM works w
	WHERE ` + strings.Join(where, " AND ") + `
	ORDER BY w.updated_at DESC LIMIT ` + fmt.Sprint(limit)
	} else {
		query = `
	SELECT w.id, w.parent_id, w.kind, w.medium, w.title, w.aliases_json, w.status, w.visibility, w.sort_order, w.updated_at,
	       CASE WHEN w.content_md IS NOT NULL AND w.content_md != '' THEN 1 ELSE 0 END AS has_content,
	       CASE WHEN EXISTS (SELECT 1 FROM library_links l WHERE l.work_id = w.id) THEN 1 ELSE 0 END AS has_link
	FROM works w
	WHERE ` + strings.Join(where, " AND ") + `
	ORDER BY w.sort_order, w.title LIMIT ` + fmt.Sprint(limit)
	}
	rows, err := s.DB.Query(query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	// 与 ListTree 同一套扫描逻辑
	out, err := scanWorkSummaries(rows)
	if err != nil {
		return nil, err
	}
	return out, nil
}

// DeleteWork removes a work after checking it has no children.
func (s *Store) DeleteWork(id string) error {
	w, err := s.GetWork(id)
	if err != nil {
		return err
	}
	var children int
	if err := s.DB.QueryRow(`SELECT COUNT(*) FROM works WHERE parent_id = ?`, w.ID).Scan(&children); err != nil {
		return err
	}
	if children > 0 {
		return ErrValidation{Message: "cannot delete work with children; move or delete children first"}
	}
	// remove dependent docs, relations, links, revisions
	tx, err := s.DB.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if _, err := tx.Exec(`DELETE FROM docs WHERE folder_of = ?`, id); err != nil {
		return err
	}
	if _, err := tx.Exec(`DELETE FROM relations WHERE from_id = ? OR to_id = ?`, id, id); err != nil {
		return err
	}
	if _, err := tx.Exec(`DELETE FROM library_links WHERE work_id = ?`, id); err != nil {
		return err
	}
	if _, err := tx.Exec(`DELETE FROM revisions WHERE target_type = 'work' AND target_id = ?`, id); err != nil {
		return err
	}
	if _, err := tx.Exec(`DELETE FROM works WHERE id = ?`, id); err != nil {
		return err
	}
	return tx.Commit()
}

// ListTree returns flat work summaries ordered for tree display.
func (s *Store) ListTree() ([]domain.WorkSummary, error) {
	rows, err := s.DB.Query(workSummaryQuery)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	return scanWorkSummaries(rows)
}

// workSummaryQuery 是 ListTree / ListWorks 共用的行形状（ListWorks 在外拼 WHERE）。
// 树=活着的库：archived 不在 ListTree 里；要看归档用 ListWorks{Status:"archived"}。
const workSummaryQuery = `
	SELECT w.id, w.parent_id, w.kind, w.medium, w.title, w.aliases_json, w.status, w.visibility, w.sort_order, w.updated_at,
	       CASE WHEN w.content_md IS NOT NULL AND w.content_md != '' THEN 1 ELSE 0 END AS has_content,
	       CASE WHEN EXISTS (SELECT 1 FROM library_links l WHERE l.work_id = w.id) THEN 1 ELSE 0 END AS has_link
	FROM works w
	WHERE w.status != 'archived'
	ORDER BY w.sort_order, w.title`

func scanWorkSummaries(rows *sql.Rows) ([]domain.WorkSummary, error) {
	out := make([]domain.WorkSummary, 0)
	for rows.Next() {
		var (
			n          domain.WorkSummary
			parentID   sql.NullString
			medium     sql.NullString
			aliasesRaw sql.NullString
			hasC, hasL int
		)
		if err := rows.Scan(&n.ID, &parentID, &n.Kind, &medium, &n.Title, &aliasesRaw, &n.Status, &n.Visibility, &n.SortOrder, &n.UpdatedAt, &hasC, &hasL); err != nil {
			return nil, err
		}
		if aliasesRaw.Valid && aliasesRaw.String != "" {
			_ = json.Unmarshal([]byte(aliasesRaw.String), &n.Aliases)
		}
		if parentID.Valid {
			n.ParentID = &parentID.String
		}
		if medium.Valid && medium.String != "" {
			m := domain.Medium(medium.String)
			n.Medium = &m
		}
		n.HasContent = hasC != 0
		n.HasLibraryLink = hasL != 0
		out = append(out, n)
	}
	return out, rows.Err()
}

// PutWorkContent transactionally commits new content with optional version check.
func (s *Store) PutWorkContent(id string, body domain.PutContentBody) (*domain.ContentCommitResult, error) {
	tx, err := s.DB.Begin()
	if err != nil {
		return nil, err
	}
	defer tx.Rollback()

	var (
		contentMd  sql.NullString
		contentVer int64
		status     string
	)
	err = tx.QueryRow(`SELECT content_md, content_ver, status FROM works WHERE id = ?`, id).Scan(&contentMd, &contentVer, &status)
	if err == sql.ErrNoRows {
		return nil, ErrNotFound{What: "work"}
	}
	if err != nil {
		return nil, err
	}
	if body.ExpectedVersion != nil && *body.ExpectedVersion != contentVer {
		return nil, ErrConflict{Message: fmt.Sprintf("version conflict: expected %d, current %d", *body.ExpectedVersion, contentVer)}
	}
	switch body.Author {
	case domain.AuthorHuman, domain.AuthorLLM, domain.AuthorImport:
	default:
		return nil, ErrValidation{Message: "author must be human|llm|import"}
	}
	now := Now()
	newVer := contentVer + 1
	summary := ""
	if body.Summary != nil {
		summary = *body.Summary
	}
	// status: empty content stays stub unless already draft/ready; non-empty promotes stub→draft
	newStatus := status
	if body.ContentMd == "" {
		if status == string(domain.WorkStatusDraft) {
			// keep draft; content can be cleared
		}
	} else if status == string(domain.WorkStatusStub) {
		newStatus = string(domain.WorkStatusDraft)
	}
	if _, err := tx.Exec(`UPDATE works SET content_md = ?, content_ver = ?, status = ?, updated_at = ? WHERE id = ?`,
		body.ContentMd, newVer, newStatus, now, id); err != nil {
		return nil, err
	}
	revID := NewID()
	var runID any
	if body.RunID != nil && *body.RunID != "" {
		runID = *body.RunID
	}
	if _, err := tx.Exec(`INSERT INTO revisions (id, target_type, target_id, version, content_md, author, run_id, summary, created_at)
		VALUES (?, 'work', ?, ?, ?, ?, ?, ?, ?)`,
		revID, id, newVer, body.ContentMd, string(body.Author), runID, summary, now); err != nil {
		return nil, err
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return &domain.ContentCommitResult{ID: id, ContentVer: newVer, RevisionID: revID}, nil
}

// listIDs is a small helper.
func nullStr(s *string) any {
	if s == nil || *s == "" {
		return nil
	}
	return *s
}

var _ = strings.TrimSpace
