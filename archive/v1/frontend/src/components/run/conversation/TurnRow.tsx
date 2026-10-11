import type { TurnItem } from '../../../lib/runItems'

// run 边界事实行：运行前的准备事实 / 收尾结论。**不是模型说的话**——是宿主在说
// "已加载 skill""质量自检通过"。所以它比叙事句弱（细灰、点状前缀），且只出现在
// 整条对话的两端（由 runView 的边界分割决定）。
export default function TurnRow({ item }: { item: TurnItem }) {
  return (
    <div className={`ai-turn-row is-${item.tone || 'info'}`}>
      <span className="ai-turn-dot" />
      <span className="ai-turn-text">{item.text}</span>
    </div>
  )
}
