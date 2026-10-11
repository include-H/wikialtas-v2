package store

import (
	"database/sql"
	"encoding/json"
	"strings"

	"wikiatlas/backend/internal/domain"
)

// GetSettings loads settings from the settings key-value table, merging defaults.
func (s *Store) GetSettings() (*domain.Settings, error) {
	cfg := domain.DefaultSettings()
	var raw sql.NullString
	err := s.DB.QueryRow(`SELECT value FROM settings WHERE key = 'app'`).Scan(&raw)
	if err == sql.ErrNoRows {
		return &cfg, nil
	}
	if err != nil {
		return nil, err
	}
	if raw.Valid && raw.String != "" {
		// 存下来的 JSON 覆盖到默认值上（递归合并）
		if err := mergeSettings(&cfg, raw.String); err != nil {
			return nil, err
		}
	}
	// API key presence from env is not reflected here; handler may override.
	return &cfg, nil
}

// SaveSettings persists the full settings object.
func (s *Store) SaveSettings(st *domain.Settings) error {
	b, err := json.Marshal(st)
	if err != nil {
		return err
	}
	_, err = s.DB.Exec(`INSERT INTO settings (key, value) VALUES ('app', ?)
		ON CONFLICT(key) DO UPDATE SET value = excluded.value`, string(b))
	return err
}

// mergeSettings 把存下来的 JSON 覆盖到默认值上（递归按 key 合并）。
//
// 通用实现，不是逐字段手写——手写的那个版本已经漏过三个字段：
// ContextWindow（设置页填了窗口，刷新又变回默认）和整个 Admin 段
// （管理员标识改了名、刷新又变回 admin；新节点可见性同理）。
// 漏字段这种 bug 不该靠人记得，所以这里改成结构无关的合并，
// 再加一个"每个字段都存得回来"的round-trip 测试兜底。
//
// 语义：**key 缺席 = 用默认值；key 存在（哪怕是零值）= 用户的值**。
// 带 omitempty 的零值字段本来就不会落盘，所以不会出现"零值盖掉默认"的意外。
func mergeSettings(dst *domain.Settings, storedJSON string) error {
	if strings.TrimSpace(storedJSON) == "" {
		return nil
	}
	base, err := json.Marshal(dst)
	if err != nil {
		return err
	}
	var baseMap, storedMap map[string]any
	if err := json.Unmarshal(base, &baseMap); err != nil {
		return err
	}
	if err := json.Unmarshal([]byte(storedJSON), &storedMap); err != nil {
		return err
	}
	mergeMap(baseMap, storedMap)
	merged, err := json.Marshal(baseMap)
	if err != nil {
		return err
	}
	return json.Unmarshal(merged, dst)
}

// mergeMap 把 src 的键值递归并进 dst；src 里显式为 null 的键不动 dst。
func mergeMap(dst, src map[string]any) {
	for k, v := range src {
		if v == nil {
			continue
		}
		if sv, ok := v.(map[string]any); ok {
			if dv, ok := dst[k].(map[string]any); ok {
				mergeMap(dv, sv)
				continue
			}
		}
		dst[k] = v
	}
}

// GetSettingValue 读一个通用 KV（媒体库匹配清单等缓存用）。
func (s *Store) GetSettingValue(key string) (string, bool) {
	var raw sql.NullString
	if err := s.DB.QueryRow(`SELECT value FROM settings WHERE key = ?`, key).Scan(&raw); err != nil || !raw.Valid {
		return "", false
	}
	return raw.String, true
}

// SetSettingValue 写一个通用 KV。
func (s *Store) SetSettingValue(key, value string) error {
	_, err := s.DB.Exec(`INSERT INTO settings (key, value) VALUES (?, ?)
		ON CONFLICT(key) DO UPDATE SET value = excluded.value`, key, value)
	return err
}

const (
	libraryManifestKVKey  = "library_manifest"
	libraryAiSuggestKVKey = "library_ai_suggestions"
	komgaJudgmentsKVKey   = "komga_judgments"
)

// GetLibraryManifest 读媒体库扫描清单；没有（或坏了）返回 nil。
func (s *Store) GetLibraryManifest() (*domain.LibraryManifest, error) {
	raw, ok := s.GetSettingValue(libraryManifestKVKey)
	if !ok || strings.TrimSpace(raw) == "" {
		return nil, nil
	}
	var m domain.LibraryManifest
	if err := json.Unmarshal([]byte(raw), &m); err != nil || m.ScannedAt == "" {
		return nil, nil
	}
	return &m, nil
}

// SaveLibraryManifest 写媒体库扫描清单。
func (s *Store) SaveLibraryManifest(m *domain.LibraryManifest) error {
	b, err := json.Marshal(m)
	if err != nil {
		return err
	}
	return s.SetSettingValue(libraryManifestKVKey, string(b))
}

// GetLibraryAiSuggestions 读「AI 对一遍」的建议；没有返回 nil。
// 注意：建议列表为空不代表记录不存在——忽略记录（Dismissed）也住在同一条里，
// 不能因为 suggestions 空就把整条吞掉（真实踩到：只 dismiss 掉最后一条建议后，dismissed 读没了）。
func (s *Store) GetLibraryAiSuggestions() (*domain.LibraryAiSuggestions, error) {
	raw, ok := s.GetSettingValue(libraryAiSuggestKVKey)
	if !ok || strings.TrimSpace(raw) == "" {
		return nil, nil
	}
	var out domain.LibraryAiSuggestions
	if err := json.Unmarshal([]byte(raw), &out); err != nil {
		return nil, nil
	}
	return &out, nil
}

// SaveLibraryAiSuggestions 写「AI 对一遍」的建议。
func (s *Store) SaveLibraryAiSuggestions(out *domain.LibraryAiSuggestions) error {
	b, err := json.Marshal(out)
	if err != nil {
		return err
	}
	return s.SetSettingValue(libraryAiSuggestKVKey, string(b))
}

// UpdateLibraryManifest 在锁内做「读 → 你改 → 存」。
//
// 为什么必须有它：这两个 KV 以前是调用方各自 `Get → 改 → Save`，没有任何锁——
// 忽略（一个 handler）与扫描（另一个）同时改同一份 manifest 时，后写的会把先写的
// 整段吞掉（lost update）。锁下沉到这里之后，httpapi 与 internal/tools 走同一条
// 通道（tools 拿不到 httpapi.Server 上的私有锁，那正是它以前写不了这两个 KV 的原因）。
//
// 回调里**只改自己那份字段**：这就是 AGENTS.md「保存时只替换自己那份」的可执行版本。
// 清单不存在时直接报错——没有清单就没有"忽略某条目"的依据。
func (s *Store) UpdateLibraryManifest(f func(*domain.LibraryManifest) error) error {
	s.manifestMu.Lock()
	defer s.manifestMu.Unlock()
	man, err := s.GetLibraryManifest()
	if err != nil {
		return err
	}
	if man == nil {
		return ErrValidation{Message: "媒体库还没有扫描过：先扫描一次，忽略/建档才有依据"}
	}
	if err := f(man); err != nil {
		return err
	}
	return s.SaveLibraryManifest(man)
}

// ReplaceLibraryManifest 在锁内做「读上一版 → 你给一整份新的 → 存」，用于**整体替换**
// 的写点（扫描：条目全量重来，但忽略标记与首见时间要从上一版继承）。
// 与 UpdateLibraryManifest 的分工就是「替换 vs 局部改」；两者共用同一把锁。
// 回调拿到的是上一版（**可能为 nil**：第一版清单），返回 nil 表示不落库。
func (s *Store) ReplaceLibraryManifest(build func(prev *domain.LibraryManifest) (*domain.LibraryManifest, error)) error {
	s.manifestMu.Lock()
	defer s.manifestMu.Unlock()
	prev, err := s.GetLibraryManifest()
	if err != nil {
		return err
	}
	next, err := build(prev)
	if err != nil {
		return err
	}
	if next == nil {
		return nil
	}
	return s.SaveLibraryManifest(next)
}

// ReplaceLibraryAiSuggestions 同上，针对建议 KV 的整体重刷（「AI 对一遍」）。
func (s *Store) ReplaceLibraryAiSuggestions(build func(prev *domain.LibraryAiSuggestions) (*domain.LibraryAiSuggestions, error)) error {
	s.suggestionsMu.Lock()
	defer s.suggestionsMu.Unlock()
	prev, err := s.GetLibraryAiSuggestions()
	if err != nil {
		return err
	}
	next, err := build(prev)
	if err != nil {
		return err
	}
	if next == nil {
		return nil
	}
	return s.SaveLibraryAiSuggestions(next)
}

// UpdateLibraryAiSuggestions 同上，针对建议 KV。
// 记录不存在时给一个空壳进去（第一次"拒绝一条建议"不该因为 KV 还没建就失败）。
func (s *Store) UpdateLibraryAiSuggestions(f func(*domain.LibraryAiSuggestions) error) error {
	s.suggestionsMu.Lock()
	defer s.suggestionsMu.Unlock()
	ai, err := s.GetLibraryAiSuggestions()
	if err != nil {
		return err
	}
	if ai == nil {
		ai = &domain.LibraryAiSuggestions{}
	}
	if err := f(ai); err != nil {
		return err
	}
	return s.SaveLibraryAiSuggestions(ai)
}

// GetKomgaJudgments 读 Komga 系列层级的 LLM 判定缓存；没有返回 nil。
func (s *Store) GetKomgaJudgments() (*domain.KomgaJudgments, error) {
	raw, ok := s.GetSettingValue(komgaJudgmentsKVKey)
	if !ok || strings.TrimSpace(raw) == "" {
		return nil, nil
	}
	var out domain.KomgaJudgments
	if err := json.Unmarshal([]byte(raw), &out); err != nil {
		return nil, nil
	}
	return &out, nil
}

// SaveKomgaJudgments 写 Komga 系列层级的判定缓存。
func (s *Store) SaveKomgaJudgments(out *domain.KomgaJudgments) error {
	b, err := json.Marshal(out)
	if err != nil {
		return err
	}
	return s.SetSettingValue(komgaJudgmentsKVKey, string(b))
}

// HasAPIKey reports whether an API key is stored (never returns the key).
func (s *Store) HasAPIKey() bool {
	var raw sql.NullString
	err := s.DB.QueryRow(`SELECT value FROM settings WHERE key = 'llm_api_key'`).Scan(&raw)
	if err != nil || !raw.Valid {
		return false
	}
	return raw.String != ""
}

// SetAPIKey stores the LLM API key (dev convenience; production should use a secret store).
func (s *Store) SetAPIKey(key string) error {
	if key == "" {
		_, err := s.DB.Exec(`DELETE FROM settings WHERE key = 'llm_api_key'`)
		return err
	}
	_, err := s.DB.Exec(`INSERT INTO settings (key, value) VALUES ('llm_api_key', ?)
		ON CONFLICT(key) DO UPDATE SET value = excluded.value`, key)
	return err
}

// GetAPIKey returns the stored key or empty.
func (s *Store) GetAPIKey() string {
	var raw sql.NullString
	_ = s.DB.QueryRow(`SELECT value FROM settings WHERE key = 'llm_api_key'`).Scan(&raw)
	if raw.Valid {
		return raw.String
	}
	return ""
}
