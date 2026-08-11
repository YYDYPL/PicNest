import { Album, BrainCircuit, Clock3, FolderMinus, Plus, Sparkles } from 'lucide-react'
import { formatDateTime } from '../lib/format'
import type { ActivityItem, Album as AlbumType } from '../lib/types'

export function AlbumOverview({ albums, onCreate, onOpen }: { albums: AlbumType[]; onCreate: () => void; onOpen: (album: AlbumType) => void }) {
  return (
    <div className="special-view album-overview">
      <div className="special-view-heading"><div><h2>相册</h2><p>相册只保存关联，不会复制原文件。</p></div><button className="primary-button compact" type="button" onClick={onCreate}><Plus size={16} />新建相册</button></div>
      <div className="album-grid">
        {albums.map((album, index) => (
          <button className="album-tile" key={album.id} type="button" aria-label={`打开相册 ${album.name}`} onClick={() => onOpen(album)}>
            <span className={`album-cover album-cover-${index % 4}`}><Album size={24} /></span>
            <span className="album-info"><strong>{album.name}</strong><small>{album.kind === 'smart' ? <><BrainCircuit size={13} />智能相册</> : '手动相册'} · {album.count} 张</small></span>
          </button>
        ))}
      </div>
    </div>
  )
}

export function ActivityView({ items }: { items: ActivityItem[] }) {
  return (
    <div className="special-view activity-view">
      <div className="special-view-heading"><div><h2>整理记录</h2><p>安全移动保留完整记录，可撤销最近一次操作。</p></div></div>
      <div className="activity-list">
        {items.map((item) => (
          <div className="activity-row" key={item.id}>
            <span className="activity-icon">{item.kind === 'ai' ? <Sparkles size={17} /> : item.kind === 'source' ? <FolderMinus size={17} /> : <Clock3 size={17} />}</span>
            <div><strong>{item.title}</strong><p>{item.detail}</p></div>
            <time>{formatDateTime(item.createdAt)}</time>
            {item.reversible ? <span className="reversible">可撤销</span> : null}
          </div>
        ))}
      </div>
    </div>
  )
}
