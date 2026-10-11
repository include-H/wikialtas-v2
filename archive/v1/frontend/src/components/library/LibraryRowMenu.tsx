import { Button, Dropdown } from '@douyinfe/semi-ui'
import { IconMore } from '@douyinfe/semi-icons'

interface Props {
  url?: string
  /** 当前角色:菜单里显示「设为 X」的切换项 */
  role: 'work' | 'mix'
  onToggleRole: (role: 'work' | 'mix') => void
  onUnlink: () => void
  unlinkBusy?: boolean
  /** 行标题,用于无障碍 label */
  label?: string
}

/**
 * 媒体行的行尾「⋯」菜单:把「打开 / 改判角色 / 解除」收进下拉,
 * 行内只留封面 + 标题 + (来源)——480px 侧栏里标题才放得下。
 */
export default function LibraryRowMenu({
  url,
  role,
  onToggleRole,
  onUnlink,
  unlinkBusy,
  label,
}: Props) {
  const target = role === 'work' ? 'mix' : 'work'
  return (
    <Dropdown
      position="bottomRight"
      trigger="click"
      render={
        <Dropdown.Menu>
          {url ? (
            <Dropdown.Item onClick={() => window.open(url, '_blank', 'noreferrer')}>
              在库中打开
            </Dropdown.Item>
          ) : null}
          <Dropdown.Item onClick={() => onToggleRole(target)}>
            {target === 'mix' ? '设为混杂' : '设为正片'}
          </Dropdown.Item>
          <Dropdown.Divider />
          <Dropdown.Item type="danger" onClick={() => onUnlink()}>
            解除关联
          </Dropdown.Item>
        </Dropdown.Menu>
      }
    >
      <Button
        theme="borderless"
        icon={<IconMore />}
        className="lm-row-more"
        type="tertiary"
        title={label ? `更多操作:${label}` : '更多操作'}
        loading={unlinkBusy}
      />
    </Dropdown>
  )
}
