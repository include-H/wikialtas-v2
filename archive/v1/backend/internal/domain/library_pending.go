package domain

// 这里曾有 PendingLibrarySuggestions（"待确认建议"的过滤口径）：随「AI 建议」这套退役——
// 池子页改成"从库里批量挑、批量建档"的进料台，不再有"建议"这个对象。
// 下面两个键工具仍被**节点页扫库**与池子复用（拒绝记录按"挂法"去重 / 回找）。

// RejectionID 是"挂法"的唯一键：(条目, 动作, 目标节点)。
// 同一键只能有一条拒绝记录——再拒一次就是用新理由覆盖旧的。
func RejectionID(key, action, targetNodeID string) string {
	return key + "|" + action + "|" + targetNodeID
}

// RejectionIndex 建 RejectionID → 拒绝记录 的索引（节点页扫库与池子共用）。
func RejectionIndex(ai *LibraryAiSuggestions) map[string]*LibraryAiRejection {
	out := map[string]*LibraryAiRejection{}
	if ai == nil {
		return out
	}
	for i := range ai.Rejected {
		r := &ai.Rejected[i]
		out[RejectionID(r.Key, r.Action, r.TargetNodeID)] = r
	}
	return out
}
