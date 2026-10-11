package run

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"strconv"
	"strings"
	"time"

	"wikiatlas/backend/internal/domain"
	"wikiatlas/backend/internal/skill"
)

// executeMock is the no-LLM path. It still reads skill files and emits a
// realistic Feishu/MiMo-style event sequence so the frontend can be exercised.
//
// 它走的是**和真模型完全一样**的发射器：每个假工具调用也开一个 function_call 项、
// 每个假播报也开一个 message 项。所以 mock 能验的就是前端真实的渲染路径，
// 不存在"演示一套、真跑另一套"。
//
// **但有一条它验不了**：mock 的话是 `em.message(...)` 直接发的，而真实模型的话来自它
// 自己回复里的 content（流式直通 message 项）。所以"模型会不会主动开口"这件事
// mock 永远显示为"会"——那是写死的，不是模型行为。判它只能真跑（见
// `scripts/` 下的观测口径：带工具调用的轮里有没有 content）。
func (m *Manager) executeMock(ctx context.Context, runID string, intent domain.RunIntent, goal string, ctxMap map[string]any, fromStep int) {
	workID, _ := ctxMap["workId"].(string)
	docID, _ := ctxMap["docId"].(string)
	mediumStr, _ := ctxMap["medium"].(string)
	medium := domain.Medium(mediumStr)
	if medium == "" && workID != "" {
		if w, err := m.store.GetWork(workID); err == nil && w.Medium != nil {
			medium = *w.Medium
		}
	}

	em := m.newRespEmitter(runID, "mock", fromStep > 0)
	em.workID, em.docID = workID, docID
	em.created(fromStep == 0)
	em.emit(EvWAMeta, map[string]any{
		"runId": runID, "sessionId": ctxMap["session"], "goal": goal,
		"intent": string(intent), "model": "mock",
	})

	loader := m.skillLoader()
	var skillNames []string
	var skillOK bool
	if loader != nil {
		if files, err := loader.FilesForIntent(intent, medium); err == nil {
			skillNames = skill.Names(files)
			skillOK = len(skillNames) > 0
		}
	}

	// mockTool 发一次完整的假工具调用：参数 → 结果，一路走规范事件。
	seq := 0
	mockTool := func(name, args, outputSummary string) {
		seq++
		callID := fmt.Sprintf("call_mock_%d", seq)
		em.callArgsDone(callID, name, args)
		em.toolResult(callID, name, args, true, map[string]any{
			"outputSummary": outputSummary, "ok": true, "durationMs": 20,
		})
	}

	steps := []struct {
		title string
		fn    func() error
	}{
		{"理解目标", func() error {
			// 思考项也要走一遍：mock 的价值就是"前端真实渲染路径全被走一遍"，
			// 少了 reasoning，思考行那条路在联调里永远没被覆盖过。
			em.reasoningDelta("用户要的是《" + truncate(goal, 20) + "》的条目。先读 wiki-writing skill 定骨架，")
			em.reasoningDelta("再查站内是否已有同名节点以免重复建档，最后联网核实关键事实。")
			em.closeReasoning()
			em.message("我先读 skill 和站内已有条目，再联网核实关键事实。")
			if skillOK {
				mockTool("read_skill", `{"path":"SKILL.md"}`, "已加载 "+strings.Join(skillNames, " + "))
			} else {
				mockTool("read_skill", `{"path":"SKILL.md"}`, "skill 未配置，按通用边界继续")
			}
			return nil
		}},
		{"检索资料", func() error {
			hits, _ := m.store.Search(goal, "", 5)
			mockTool("search_works", `{"q":`+mustJSONString(truncate(goal, 40))+`}`,
				fmt.Sprintf("命中 %d 条", len(hits)))
			mockTool("search_web", `{"q":`+mustJSONString(truncate(goal, 40))+`}`, "no web：未配置 Exa key")
			em.message("站内检索完毕；联网检索不可用，事实项将标「待核实」。")
			return nil
		}},
		{"产出内容", func() error {
			em.message("资料核实完毕。现在按骨架写入条目（无模型演示内容）。")
			return nil
		}},
		{"写入并完成", func() error {
			if workID == "" && docID == "" {
				em.notice("（未绑定作品/资料，跳过写入。配置 WIKIATLAS_LLM_* 后可调用真实模型。）", NoticeInfo)
				return nil
			}
			md := mockMarkdown(goal, intent, medium, skillNames)
			targetType, targetID := "work", workID
			if targetID == "" {
				targetType, targetID = "doc", docID
			}
			seq++
			callID := fmt.Sprintf("call_mock_%d", seq)
			// 参数按真实形状组装，并走真实的分流路径（callArgsStream → 正文流）：
			// 默认一次性喂完（等价非流式）；WIKIATLAS_MOCK_WRITE_DELAY_MS>0 时
			// 按节拍逐段喂，模拟"模型正在吐正文"——正文流、提前 staging、
			// 编辑器实时长草稿这条链路全都会被真实走一遍。
			args := fmt.Sprintf(`{"targetType":%q,"targetId":%q,"contentMd":%s,"summary":"mock librarian write"}`,
				targetType, targetID, mustJSONString(md))
			if delay := mockWriteDelay(); delay > 0 {
				runes := []rune(args)
				chunks := 60
				if chunks > len(runes) {
					chunks = len(runes)
				}
				if chunks > 0 {
					step := (len(runes) + chunks - 1) / chunks
					per := delay / time.Duration(chunks)
					for cut := step; cut < len(runes); cut += step {
						em.callArgsStream(callID, "write_content", string(runes[:cut]))
						select {
						case <-ctx.Done():
							return ctx.Err()
						case <-time.After(per):
						}
					}
				}
			}
			em.callArgsStream(callID, "write_content", args)
			em.callArgsDone(callID, "write_content", args)
			author := domain.AuthorLLM
			summary := "mock librarian write"
			var err error
			if targetType == "doc" {
				_, err = m.store.PutDocContent(targetID, domain.PutContentBody{
					ContentMd: md,
					Author:    author,
					RunID:     &runID,
					Summary:   &summary,
				})
			} else {
				var res *domain.ContentCommitResult
				res, err = m.store.PutWorkContent(targetID, domain.PutContentBody{
					ContentMd: md,
					Author:    author,
					RunID:     &runID,
					Summary:   &summary,
				})
				if err == nil {
					em.emit(EvWACommit, map[string]any{
						"targetType": targetType,
						"targetId":   targetID,
						"version":    res.ContentVer,
					})
				}
			}
			if err != nil {
				return err
			}
			if targetType == "doc" {
				// committed event for docs
				if d, e := m.store.GetDoc(targetID); e == nil {
					em.emit(EvWACommit, map[string]any{
						"targetType": "doc",
						"targetId":   targetID,
						"version":    d.ContentVer,
					})
				}
			}
			em.toolResult(callID, "write_content", "", true, map[string]any{
				"outputSummary": "已提交 revision", "ok": true, "durationMs": 30,
			})

			// quality gate（与真实执行器同纪律：归档节点不改状态）
			archived := false
			if targetType == "work" {
				if w, err := m.store.GetWork(targetID); err == nil && w.Status == domain.WorkStatusArchived {
					archived = true
				}
			}
			q := CheckWikiQuality(md)
			if q.OK {
				if targetType == "work" && !archived {
					st := domain.WorkStatusReady
					_, _ = m.store.PatchWork(targetID, domain.PatchWorkBody{Status: &st})
				}
				em.emit(EvWATree, map[string]any{
					"works": []map[string]any{{"id": targetID, "status": "ready"}},
				})
				em.notice("质量自检通过，已标记 ready。", NoticeInfo)
			} else {
				if targetType == "work" && !archived {
					st := domain.WorkStatusDraft
					_, _ = m.store.PatchWork(targetID, domain.PatchWorkBody{Status: &st})
				}
				em.emit(EvWATree, map[string]any{
					"works": []map[string]any{{"id": targetID, "status": "draft"}},
				})
				em.notice("质量自检未达标（演示稿），保留 draft："+strings.Join(q.Issues, "；"), NoticeWarn)
			}
			em.message("已写入正文并提交 revision。")
			return nil
		}},
	}

	for i := fromStep; i < len(steps); i++ {
		select {
		case <-ctx.Done():
			em.sealOpenCalls("工单已取消，这次调用没有结果")
			_ = m.store.InterruptRun(runID)
			em.runEnd(RunEndInterrupted, "工单已取消")
			return
		default:
		}
		if err := steps[i].fn(); err != nil {
			// 取消不是失败：写正文那一步会在 ctx 结束时把 ctx.Err() 当错误返回，
			// 落到下面那条 fail 分支就会把"用户点了停止"记成"工具坏了"——终态也错
			// （failed 而不是 interrupted）。真实执行器分开处理，这里对齐。
			if ctx.Err() != nil {
				em.sealOpenCalls("工单已取消，这次调用没有结果")
				_ = m.store.InterruptRun(runID)
				em.runEnd(RunEndInterrupted, "工单已取消")
				return
			}
			em.failed("mock_error", err.Error())
			_ = m.store.FailRun(runID, map[string]any{"error": err.Error(), "step": i})
			return
		}
		cp := map[string]any{"lastStep": i + 1, "context": ctxMap}
		_ = m.store.UpdateRunCheckpoint(runID, cp, map[string]any{})
	}

	em.completed("工单完成（mock executor）", false)
	_ = m.store.CompleteRun(runID, map[string]any{"summary": "ok", "intent": string(intent), "mock": true})
}

// mustJSONString 把一个字符串编成 JSON 字符串字面量（拼 mock 的工具参数用）。
func mustJSONString(s string) string {
	b, err := json.Marshal(s)
	if err != nil {
		return `""`
	}
	return string(b)
}

// mockWriteDelay 写入演示的节奏（0 = 关闭，一次性喂完参数）。
// >0 时 mock 把 write_content 的参数按这个总时长逐段喂给发射器，
// 模拟"模型正在吐正文"——正文流 / 提前 staging / 编辑器实时长草稿整条链路都会被真实走一遍。
func mockWriteDelay() time.Duration {
	if v := os.Getenv("WIKIATLAS_MOCK_WRITE_DELAY_MS"); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n > 0 {
			return time.Duration(n) * time.Millisecond
		}
	}
	return 0
}

func mockMarkdown(goal string, intent domain.RunIntent, medium domain.Medium, skillNames []string) string {
	var b strings.Builder
	title := goal
	if title == "" {
		title = "未命名条目"
	}
	b.WriteString("# " + title + "\n\n")
	b.WriteString("> 说明：mock Altas 生成的演示稿。配置 `WIKIATLAS_LLM_API_KEY` 后将调用真实模型并按 skill 写作。\n")
	if len(skillNames) > 0 {
		b.WriteString("> 已参考 skill：" + strings.Join(skillNames, "、") + "。\n")
	}
	b.WriteString("> 剧透提示：以下为结构演示，不含真实剧情细节。\n\n")
	b.WriteString(":::epigraph\n演示题记：此处应由场景式题记占据。\n:::\n\n")

	if intent == domain.RunIntentWriteDoc {
		b.WriteString("## 资料说明\n\n本篇为资料文档演示，不强制 9 章骨架。\n\n")
		b.WriteString("## 要点\n\n- 唯一正文真相是 `content_md`\n- 写完即 commit，无审批\n- 回滚靠 revision\n\n")
		return b.String()
	}

	b.WriteString("## 1. 作品概览\n\n本条目由 Altas 工单自动写入，用于验证 SSE 推送与 revision 落库。介质：" + string(medium) + "。\n\n")
	b.WriteString("## 2. 基础信息速览\n\n### 2.1 基本资料\n\n- 原名：待核实\n- 首发日期：待核实\n- 首发平台：待核实\n\n### 2.2 版本关系\n\n- 版本关系：待核实\n\n### 2.3 核心卖点\n\n- 结构演示，非真实卖点\n\n### 2.4 关键词\n\n- 待核实\n\n")
	b.WriteString("## 3. 故事背景与设定\n\n背景设定演示段落。查不到的事实一律标「待核实」。\n\n")
	b.WriteString("## 4. 剧情概要\n\n### 4.1 开端\n\n演示性开端描述，真实条目应按作品结构分节写到结局。\n\n### 4.2 转折\n\n演示性转折。\n\n### 4.3 结局\n\n演示性结局。\n\n")
	b.WriteString("## 5. 创作特色\n\n### 5.1 手法\n\n机制/镜头/叙事手法应挂到具体场景。\n\n### 5.2 效果\n\n读者实际感受到的结果。\n\n")
	b.WriteString("## 6. 主要角色与创作团队\n\n### 6.1 角色\n\n角色名：说明（演示）\n\n### 6.2 关系简述\n\n用一小段点出 2–3 层关键关系。\n\n")
	b.WriteString("## 7. 评价与历史定位\n\n- 评分：待核实\n- 历史定位：待核实\n\n")
	b.WriteString("## 8. 一句话总结\n\n这是一篇用于验证管线的演示稿。\n\n")
	b.WriteString("## 9. 参考资料\n\n1. [待核实](about:blank)\n")
	return b.String()
}
