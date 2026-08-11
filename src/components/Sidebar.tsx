import {
  Album,
  Clock3,
  Copy,
  FileQuestion,
  Heart,
  History,
  Images,
  Inbox,
  Settings,
} from 'lucide-react'
import type { LibraryStats, ViewId } from '../lib/types'

interface SidebarProps {
  activeView: ViewId
  stats: LibraryStats
  onViewChange: (view: ViewId) => void
  onSettings: () => void
}

const items: Array<{ id: ViewId; label: string; icon: typeof Inbox; stat?: keyof LibraryStats }> = [
  { id: 'inbox', label: '待整理', icon: Inbox, stat: 'inbox' },
  { id: 'all', label: '全部图片', icon: Images, stat: 'total' },
  { id: 'recent', label: '最近导入', icon: Clock3 },
  { id: 'albums', label: '相册', icon: Album, stat: 'albums' },
  { id: 'favorites', label: '收藏', icon: Heart, stat: 'favorites' },
  { id: 'duplicates', label: '重复图片', icon: Copy, stat: 'duplicates' },
  { id: 'missing', label: '缺失文件', icon: FileQuestion, stat: 'missing' },
  { id: 'history', label: '整理记录', icon: History },
]

export function Sidebar({ activeView, stats, onViewChange, onSettings }: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="brand" aria-label="PicNest">
        <span className="brand-mark" aria-hidden="true">
          <span />
          <span />
          <span />
        </span>
        <span className="brand-name">PicNest</span>
      </div>

      <nav className="sidebar-nav" aria-label="图库导航">
        {items.map((item) => {
          const Icon = item.icon
          const count = item.stat ? stats[item.stat] : undefined
          return (
            <button
              key={item.id}
              className="nav-item"
              data-active={activeView === item.id}
              type="button"
              onClick={() => onViewChange(item.id)}
            >
              <Icon size={17} strokeWidth={1.8} aria-hidden="true" />
              <span>{item.label}</span>
              {typeof count === 'number' && count > 0 ? <span className="nav-count">{count}</span> : null}
            </button>
          )
        })}
      </nav>

      <div className="sidebar-footer">
        <div className="storage-meter" aria-label="本地索引状态">
          <div className="storage-meter-heading">
            <span>本地图库</span>
            <span>{stats.total} 张</span>
          </div>
          <div className="storage-track"><span style={{ width: `${Math.min(84, 18 + stats.total / 2)}%` }} /></div>
          <p>原图始终保存在你的电脑上</p>
        </div>
        <button className="nav-item settings-link" type="button" onClick={onSettings}>
          <Settings size={17} strokeWidth={1.8} aria-hidden="true" />
          <span>设置</span>
        </button>
      </div>
    </aside>
  )
}
