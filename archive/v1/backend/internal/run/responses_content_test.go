package run

import (
	"strings"
	"testing"

	"wikiatlas/backend/internal/domain"
)

// 正文流：write_content / patch_section 的流式参数要被投影成
// wikiatlas.content.delta 的"段"，而不是逐条 function_call_arguments.delta。
func TestContentStreamProjectsArgsToDeltas(t *testing.T) {
	m := newTestManager(t)
	r, err := m.store.CreateRun(domain.RunIntentCreateWiki, "测试", "test", "work:w1")
	if err != nil {
		t.Fatal(err)
	}
	em := m.newRespEmitter(r.ID, "test", false)
	em.workID = "w1"

	full := `{"targetType":"work","targetId":"w1","contentMd":"# 标题\n\n第一段正文。","summary":"s"}`
	// 第一段：目标还没解析出来 + 正文开头
	em.callArgsStream("call_1", "write_content", full[:30])
	// 第二段：正文继续长（200ms 节流窗口内，先进缓冲）
	em.callArgsStream("call_1", "write_content", full[:45])
	// 定稿兜底：尾巴必须落下去
	em.callArgsDone("call_1", "write_content", full)

	events, err := m.store.ListRunEvents(r.ID, 0, 1000)
	if err != nil {
		t.Fatal(err)
	}
	var (
		stagedSeen bool
		stagedID   string
		text       strings.Builder
		argDeltas  int
	)
	for _, ev := range events {
		switch ev.Type {
		case EvWAContentDelta:
			if d, ok := ev.Payload["delta"].(string); ok {
				text.WriteString(d)
			}
			if name := ev.Payload["name"]; name != "write_content" {
				t.Fatalf("content.delta 的 name 应为 write_content，得到 %v", name)
			}
		case EvWAStage:
			stagedSeen = true
			stagedID, _ = ev.Payload["targetId"].(string)
		case EvFnArgsDelta:
			argDeltas++
		}
	}
	if !stagedSeen {
		t.Fatal("staging 应在开写时就发出（而不是等工具执行）")
	}
	if stagedID != "w1" {
		t.Fatalf("staging 的 targetId 应为 w1，得到 %q", stagedID)
	}
	if argDeltas != 0 {
		t.Fatalf("正文工具的逐条参数增量不应再转发，得到 %d 条", argDeltas)
	}
	if got := text.String(); got != "# 标题\n\n第一段正文。" {
		t.Fatalf("content.delta 拼接后应为完整正文，得到 %q", got)
	}
}

// 目标缺省回退：参数里没有 targetId 时用 Context 的 workId（与工具自身规则一致）。
func TestContentStreamTargetFallbackToContext(t *testing.T) {
	m := newTestManager(t)
	r, err := m.store.CreateRun(domain.RunIntentContinueWiki, "测试", "test", "work:w9")
	if err != nil {
		t.Fatal(err)
	}
	em := m.newRespEmitter(r.ID, "test", false)
	em.workID = "w9"

	em.callArgsStream("call_1", "patch_section", `{"heading":"作品概览","newMarkdown":"概览正文`)
	events, _ := m.store.ListRunEvents(r.ID, 0, 200)
	for _, ev := range events {
		if ev.Type != EvWAContentDelta || ev.Seq < 1 {
			continue
		}
		if ev.Payload["targetId"] != "w9" || ev.Payload["targetType"] != "work" {
			t.Fatalf("targetId 应回退到 ctx 的 workId，得到 %v/%v",
				ev.Payload["targetType"], ev.Payload["targetId"])
		}
		if ev.Payload["heading"] != "作品概览" {
			t.Fatalf("patch_section 的 content.delta 应带 heading，得到 %v", ev.Payload["heading"])
		}
		return
	}
	t.Fatal("应至少发出一条 content.delta")
}

// 半截 JSON 抠字段：转义、\uXXXX（含代理对）、断口都要稳。
func TestPartialStringArg(t *testing.T) {
	cases := []struct {
		raw, key, want string
	}{
		{`{"contentMd":"abc"}`, "contentMd", "abc"},
		{`{"contentMd":"a\nb"}`, "contentMd", "a\nb"},
		{`{"contentMd":"日本語"}`, "contentMd", "日本語"},
		{`{"contentMd":"😀 表情"}`, "contentMd", "😀 表情"},
		{`{"contentMd":"半截\u67`, "contentMd", "半截"},                    // \uXXXX 未到齐：到此为止
		{`{"contentMd":"断在转义\`, "contentMd", "断在转义"},                   // 转义符断在末尾
		{`{"targetId":"w1","contentMd":"` + "长正文", "contentMd", "长正文"}, // 未闭合
		{`{"text":"控制工具"}`, "text", "控制工具"},
		{`{"other":"x"}`, "contentMd", ""},
	}
	for _, c := range cases {
		if got := partialStringArg(c.raw, c.key); got != c.want {
			t.Errorf("partialStringArg(%q, %q) = %q, want %q", c.raw, c.key, got, c.want)
		}
	}
}

// 波次投影：patch_sections 的 sections 数组按节各自成段——每段带自己的
// heading，前端据此逐位拼接；逐条参数增量同样不再转发。
func TestContentStreamWaveProjectsPerSection(t *testing.T) {
	m := newTestManager(t)
	r, err := m.store.CreateRun(domain.RunIntentCreateWiki, "测试", "test", "work:w1")
	if err != nil {
		t.Fatal(err)
	}
	em := m.newRespEmitter(r.ID, "test", false)
	em.workID = "w1"

	full := `{"targetId":"w1","sections":[{"heading":"第一章","newMarkdown":"第一章正文。"},{"heading":"第二章","newMarkdown":"第二章正文。"}]}`
	// 渐进喂：目标 → 第一节标题+正文 → 第二节标题
	em.callArgsStream("w1", "patch_sections", full[:14])
	em.callArgsStream("w1", "patch_sections", full[:44])
	em.callArgsStream("w1", "patch_sections", full[:58])
	em.callArgsDone("w1", "patch_sections", full)

	events, _ := m.store.ListRunEvents(r.ID, 0, 1000)
	byHeading := map[string]string{}
	staged := 0
	var argDeltas int
	for _, ev := range events {
		switch ev.Type {
		case EvWAContentDelta:
			h, _ := ev.Payload["heading"].(string)
			if d, ok := ev.Payload["delta"].(string); ok && h != "" {
				byHeading[h] += d
			}
		case EvWAStage:
			staged++
		case EvFnArgsDelta:
			argDeltas++
		}
	}
	if staged == 0 {
		t.Fatal("波次 staging 应在开波时发出")
	}
	if argDeltas != 0 {
		t.Fatalf("波次的逐条参数增量不应转发，得到 %d 条", argDeltas)
	}
	if byHeading["第一章"] != "第一章正文。" {
		t.Fatalf("第一节投影错误：%q", byHeading["第一章"])
	}
	if byHeading["第二章"] != "第二章正文。" {
		t.Fatalf("第二节投影错误：%q", byHeading["第二章"])
	}
}

// 乱序参数：模型先写 newMarkdown、heading 后闭合——每节仍要配对自己的标题。
func TestContentStreamWaveHeadingAfterBody(t *testing.T) {
	m := newTestManager(t)
	r, err := m.store.CreateRun(domain.RunIntentCreateWiki, "测试", "test", "work:w1")
	if err != nil {
		t.Fatal(err)
	}
	em := m.newRespEmitter(r.ID, "test", false)
	em.workID = "w1"

	// 第一帧：正文已流、标题还没闭合（断在转义前）
	em.callArgsStream("w1", "patch_sections", `{"targetId":"w1","sections":[{"newMarkdown":"先来的正文。","heading":"第一`)
	// 第二帧：标题闭合
	mid := `{"targetId":"w1","sections":[{"newMarkdown":"先来的正文。","heading":"第一章条"}`
	em.callArgsStream("w1", "patch_sections", mid)
	em.callArgsDone("w1", "patch_sections", mid+"]}")

	events, _ := m.store.ListRunEvents(r.ID, 0, 1000)
	var first string
	for _, ev := range events {
		if ev.Type != EvWAContentDelta {
			continue
		}
		if h, _ := ev.Payload["heading"].(string); h == "第一章条" {
			if d, ok := ev.Payload["delta"].(string); ok {
				first += d
			}
		}
	}
	if first != "先来的正文。" {
		t.Fatalf("heading 后到仍应配对第一节：%q", first)
	}
}
