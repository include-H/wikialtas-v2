package run

import (
	"testing"

	"wikiatlas/backend/internal/llm"
)

func writeToolsIn(defs []llm.ToolDef) []string {
	var out []string
	for _, d := range defs {
		if writeToolNames[d.Function.Name] {
			out = append(out, d.Function.Name)
		}
	}
	return out
}

// 工具面**只剥两种用户显式选择**：只读模式（docMode=read）与一次性入口
// （oneShot，工具栏「翻译/解释」按钮的能力契约）。其余一律全集——意图只是
// 提示里的任务框定，猜错了（打错字、半路纠偏、问句其实暗示干活）也该能干活。
// 「翻译写回正文」事故的护栏：显式入口走 oneShot 剥工具（结构性），自由输入
// 靠提示（一次性输出不落盘）+ 可回滚修订兜底。
func TestToolSurfaceFollowsExplicitChoicesOnly(t *testing.T) {
	edit := buildToolDefsFor("edit", false)
	if bad := writeToolsIn(edit); len(bad) == 0 {
		t.Fatal("edit 模式必须下发写工具（意图不决定工具面）")
	}

	read := buildToolDefsFor("read", false)
	if len(read) == 0 {
		t.Fatal("只读模式仍应下发只读工具")
	}
	if bad := writeToolsIn(read); len(bad) > 0 {
		t.Fatalf("只读模式不该有写工具：%v", bad)
	}

	oneShot := buildToolDefsFor("edit", true)
	if len(oneShot) == 0 {
		t.Fatal("oneShot 入口仍应下发只读工具")
	}
	if bad := writeToolsIn(oneShot); len(bad) > 0 {
		t.Fatalf("oneShot 入口不该有写工具：%v", bad)
	}
}

// edit 是"改一句"的正路（题记、说明行没有 ## 锚点，只有它够得着）：
// 非只读模式必须下发，只读模式必须收走。
func TestEditToolSurface(t *testing.T) {
	has := func(defs []llm.ToolDef) bool {
		for _, d := range defs {
			if d.Function.Name == "edit" {
				return true
			}
		}
		return false
	}
	if !has(buildToolDefsFor("edit", false)) {
		t.Fatal("非只读模式必须下发 edit")
	}
	if has(buildToolDefsFor("read", false)) {
		t.Fatal("只读模式不该下发 edit")
	}
	if has(buildToolDefsFor("edit", true)) {
		t.Fatal("oneShot 入口不该下发 edit")
	}
}

// 前缀检查：请求必须是上一次的追加延长；没有记录原因的改写要被抓出来。
// 这是 dsh"请求可从日志重建"的轻量落地——自建 vLLM 网关不回 cached_tokens，
// 缓存命中在 API 上不可观测，只能自己验。
func TestRequestShapeDetectsPrefixRewrite(t *testing.T) {
	base := []llm.Message{
		{Role: "system", Content: "S"},
		{Role: "user", Content: "U"},
	}
	s := requestShape{}
	if ok, _ := s.check(base); !ok {
		t.Fatal("首次请求不该报分歧")
	}
	s.prev = append([]llm.Message(nil), base...)
	s.seen = true

	extended := append(append([]llm.Message(nil), base...), llm.Message{Role: "assistant", Content: "A"})
	if ok, at := s.check(extended); !ok {
		t.Fatalf("追加延长应通过，却在 %d 处报分歧", at)
	}

	// 改写中间一条（压缩之外的任何改写都是 bug）
	rewritten := []llm.Message{{Role: "system", Content: "S"}, {Role: "user", Content: "被改了"}}
	if ok, at := s.check(rewritten); ok || at != 1 {
		t.Fatalf("改写应被抓到：ok=%v at=%d", ok, at)
	}
	// 变短
	if ok, _ := s.check(base[:1]); ok {
		t.Fatal("变短应被抓到")
	}
	// 有记录原因的重塑（压缩）→ 重建基线，不报
	s.reason = "compaction"
	if ok, _ := s.check(rewritten); !ok {
		t.Fatal("有记录的压缩不该报分歧")
	}
	// 工具调用的差异也要能看出来
	s2 := requestShape{seen: true, prev: []llm.Message{{
		Role: "assistant", ToolCalls: []llm.ToolCall{{ID: "a", Function: llm.FunctionCall{Name: "read_work"}}},
	}}}
	if ok, _ := s2.check([]llm.Message{{
		Role: "assistant", ToolCalls: []llm.ToolCall{{ID: "a", Function: llm.FunctionCall{Name: "write_content"}}},
	}}); ok {
		t.Fatal("工具调用变了却没被发现")
	}
}
