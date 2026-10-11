package store

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"strings"

	"wikiatlas/backend/internal/domain"
)

// 审计体检的确定性规则。只报不修：每条发现是 id+title+一句人话，
// 动作仍走普通工具（upsert_work 搬家/改状态、delete_work 清残、…）。
//
// 查什么（全库级，get_tree 有 80 条截断、查不了）：
//   - misplaced  层级错挂：单作挂根上 / 单作下面挂了子节点 / 宇宙挂在系列下
//   - driftStub  状态漂移：stub 却已有大段正文（该升级）
//   - driftReady 状态漂移：ready 却是空正文（质量门被绕过或事后被清空）
//   - duplicates 重复同名：规范化标题撞车，或标题落在别节点的 aliases 里
//   - noMedium   单作缺介质
//
// 不查"死链"：relations 两端 FK + 删除级联 + 三元组唯一 + 写边时的 DAG
// 环检，悬空边在结构上不可能出现；"边指到错的节点"是语义问题，
// 由模型 read_work 看两端判，没有一条 SQL 能代劳。

const (
	auditDriftCharFloor = 500 // stub 正文超过这个量就该升级，与 skill 篇幅下限一个量级
	auditListCap        = 20  // 每类发现最多落几条（防止审计结果本身撑爆上下文）
)

// Audit 跑一遍库级体检。零 LLM，纯 SQL；空类返回空切片。
func (s *Store) Audit() (*domain.AuditReport, error) {
	rep := &domain.AuditReport{}
	if err := s.DB.QueryRow(`SELECT COUNT(*) FROM works WHERE status != 'archived'`).Scan(&rep.Total); err != nil {
		return nil, err
	}
	if err := s.DB.QueryRow(`SELECT COUNT(*) FROM works WHERE status = 'archived'`).Scan(&rep.Archived); err != nil {
		return nil, err
	}
	if err := s.DB.QueryRow(`SELECT COUNT(*) FROM relations`).Scan(&rep.Relations); err != nil {
		return nil, err
	}
	if err := s.DB.QueryRow(`SELECT COUNT(*) FROM works WHERE status = 'stub' AND (content_md IS NULL OR content_md = '')`).Scan(&rep.StubsEmpty); err != nil {
		return nil, err
	}

	var err error
	if rep.DriftStub, err = s.auditDriftStub(); err != nil {
		return nil, fmt.Errorf("audit driftStub: %w", err)
	}
	if rep.DriftReady, err = s.auditDriftReady(); err != nil {
		return nil, fmt.Errorf("audit driftReady: %w", err)
	}
	if rep.Misplaced, err = s.auditMisplaced(); err != nil {
		return nil, fmt.Errorf("audit misplaced: %w", err)
	}
	if rep.NoMedium, err = s.auditNoMedium(); err != nil {
		return nil, fmt.Errorf("audit noMedium: %w", err)
	}
	if rep.Duplicates, err = s.auditDuplicates(); err != nil {
		return nil, fmt.Errorf("audit duplicates: %w", err)
	}
	return rep, nil
}

func auditCap(list []domain.AuditFinding) []domain.AuditFinding {
	if len(list) > auditListCap {
		list = list[:auditListCap]
	}
	return list
}

// auditDriftStub: stub 却已有大段正文。
func (s *Store) auditDriftStub() ([]domain.AuditFinding, error) {
	rows, err := s.DB.Query(`
		SELECT id, title, kind, status, LENGTH(content_md) AS n
		FROM works
		WHERE status = 'stub' AND content_md IS NOT NULL AND LENGTH(content_md) > ?`, auditDriftCharFloor)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := make([]domain.AuditFinding, 0)
	for rows.Next() {
		var (
			f domain.AuditFinding
			n int
		)
		if err := rows.Scan(&f.ID, &f.Title, &f.Kind, &f.Status, &n); err != nil {
			return nil, err
		}
		f.Detail = fmt.Sprintf("stub 状态却已有约 %d 字正文，该升级 draft/ready", n)
		out = append(out, f)
	}
	return auditCap(out), rows.Err()
}

// auditDriftReady: ready 却是空正文。
func (s *Store) auditDriftReady() ([]domain.AuditFinding, error) {
	rows, err := s.DB.Query(`
		SELECT id, title, kind, status
		FROM works
		WHERE status = 'ready' AND (content_md IS NULL OR content_md = '')`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := make([]domain.AuditFinding, 0)
	for rows.Next() {
		var f domain.AuditFinding
		if err := rows.Scan(&f.ID, &f.Title, &f.Kind, &f.Status); err != nil {
			return nil, err
		}
		f.Detail = "ready 却是空正文（质量门被绕过或事后被清空）"
		out = append(out, f)
	}
	return auditCap(out), rows.Err()
}

// auditMisplaced: 层级错挂（单作挂根 / 单作有子 / 宇宙挂系列下）。
func (s *Store) auditMisplaced() ([]domain.AuditFinding, error) {
	rows, err := s.DB.Query(`
		SELECT w.id, w.title, w.kind, w.status,
		       CASE WHEN w.parent_id IS NULL THEN 'root' ELSE COALESCE(p.kind, 'gone') END
		FROM works w
		LEFT JOIN works p ON p.id = w.parent_id
		WHERE w.status != 'archived' AND (
			  (w.kind = 'work' AND w.parent_id IS NULL)
		   OR  p.kind = 'work'
		   OR  (w.kind = 'universe' AND p.kind = 'series')
		)`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := make([]domain.AuditFinding, 0)
	for rows.Next() {
		var (
			f          domain.AuditFinding
			parentKind string
		)
		if err := rows.Scan(&f.ID, &f.Title, &f.Kind, &f.Status, &parentKind); err != nil {
			return nil, err
		}
		switch parentKind {
		case "root":
			f.Detail = "单作挂在根上，应归入某个系列或宇宙"
		case "work":
			f.Detail = "单作下面挂了子节点——单作不该再有层级，把子节点改挂到系列上"
		case "series":
			f.Detail = "宇宙挂在系列下，层级倒挂"
		default:
			f.Detail = "父节点查不到了（数据损坏，建议人工核对）"
		}
		out = append(out, f)
	}
	return auditCap(out), rows.Err()
}

// auditNoMedium: 单作缺介质。
func (s *Store) auditNoMedium() ([]domain.AuditFinding, error) {
	rows, err := s.DB.Query(`
		SELECT id, title, kind, status
		FROM works
		WHERE status != 'archived' AND kind = 'work' AND (medium IS NULL OR medium = '')`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := make([]domain.AuditFinding, 0)
	for rows.Next() {
		var f domain.AuditFinding
		if err := rows.Scan(&f.ID, &f.Title, &f.Kind, &f.Status); err != nil {
			return nil, err
		}
		f.Detail = "单作缺介质（game|movie|tv|manga|book|other）"
		out = append(out, f)
	}
	return auditCap(out), rows.Err()
}

// auditDuplicates: 规范化标题撞车，或标题落在别节点的 aliases 里。
// 每个撞名节点出一条发现，detail 点名撞了谁。
func (s *Store) auditDuplicates() ([]domain.AuditFinding, error) {
	rows, err := s.DB.Query(`
		SELECT id, title, kind, status, aliases_json
		FROM works
		WHERE status != 'archived'`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	type row struct {
		id      string
		title   string
		kind    string
		status  string
		aliases []string
	}
	var all []row
	for rows.Next() {
		var (
			r         row
			aliasJSON sql.NullString
		)
		if err := rows.Scan(&r.id, &r.title, &r.kind, &r.status, &aliasJSON); err != nil {
			return nil, err
		}
		if aliasJSON.Valid && aliasJSON.String != "" {
			_ = json.Unmarshal([]byte(aliasJSON.String), &r.aliases)
		}
		all = append(all, r)
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}

	byTitle := map[string][]int{} // norm(title) → 下标
	titleNorm := func(t string) string { return strings.ToLower(strings.Join(strings.Fields(t), " ")) }
	for i, r := range all {
		byTitle[titleNorm(r.title)] = append(byTitle[titleNorm(r.title)], i)
	}
	out := make([]domain.AuditFinding, 0)
	for i, r := range all {
		// 1) 同标题
		if idxs := byTitle[titleNorm(r.title)]; len(idxs) > 1 {
			names := make([]string, 0, len(idxs)-1)
			for _, j := range idxs {
				if j != i {
					names = append(names, all[j].title)
				}
			}
			out = append(out, domain.AuditFinding{
				ID: r.id, Title: r.title, Kind: r.kind, Status: r.status,
				Detail: "与以下节点同名：" + strings.Join(names, "、"),
			})
			continue
		}
		// 2) 标题落在别节点的 aliases 里
		myNorm := titleNorm(r.title)
		for _, o := range all {
			if o.id == r.id {
				continue
			}
			for _, a := range o.aliases {
				if titleNorm(a) == myNorm {
					out = append(out, domain.AuditFinding{
						ID: r.id, Title: r.title, Kind: r.kind, Status: r.status,
						Detail: "标题与「" + o.title + "」的别名撞名",
					})
					break
				}
			}
		}
		if len(out) >= auditListCap {
			break
		}
	}
	return auditCap(out), nil
}
