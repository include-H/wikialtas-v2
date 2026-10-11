package run

import (
	"strings"
	"testing"

	"wikiatlas/backend/internal/domain"
)

// 思考是**展示用**的（不回喂模型），所以不设上限：以前截到 4000 rune，长思考
// 写到一半突然停住，被读成"模型卡住/网关截断"（真实误判）。
// 这条测试钉住"有多少显示多少"——别再让某个上限悄悄回来。
func TestReasoningIsNotTruncated(t *testing.T) {
	m := newTestManager(t)
	r, err := m.store.CreateRun(domain.RunIntentAnswer, "在", "test", "home")
	if err != nil {
		t.Fatal(err)
	}
	em := m.newRespEmitter(r.ID, "test", false)

	head := strings.Repeat("思考", 3000) // 6000 rune，是旧上限 4000 的 1.5 倍
	tail := "尾巴必须还在。"
	em.reasoningDelta(head)
	em.reasoningDelta(tail)
	em.closeReasoning()

	events, err := m.store.ListRunEvents(r.ID, 0, 10000)
	if err != nil {
		t.Fatal(err)
	}
	var (
		delta strings.Builder
		done  string
	)
	for _, ev := range events {
		switch ev.Type {
		case EvReasonDelta:
			if d, ok := ev.Payload["delta"].(string); ok {
				delta.WriteString(d)
			}
		case EvReasonDone:
			done, _ = ev.Payload["text"].(string)
		}
	}

	want := head + tail
	if got := delta.String(); got != want {
		t.Fatalf("增量应完整：期望 %d rune，得到 %d rune", len([]rune(want)), len([]rune(got)))
	}
	if done != want {
		t.Fatalf("done.text 应完整：期望 %d rune，得到 %d rune", len([]rune(want)), len([]rune(done)))
	}
	if !strings.HasSuffix(done, tail) {
		t.Fatal("结尾被截断了")
	}
}
