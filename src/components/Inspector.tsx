import {
  BrainCircuit,
  Camera,
  Copy,
  ExternalLink,
  FileText,
  FolderOpen,
  FolderPlus,
  HardDrive,
  Heart,
  MapPin,
  ScanText,
  Sparkles,
  Tags,
  Trash2,
} from 'lucide-react'
import { categoryLabel, formatBytes, formatDateTime } from '../lib/format'
import { useAssetImage } from '../lib/images'
import type { Album, Asset, AssetLocation } from '../lib/types'

interface InspectorProps {
  asset: Asset | null
  analyzing: boolean
  ocring: boolean
  onFavorite: (asset: Asset) => void
  onAnalyze: (asset: Asset) => void
  onOcr: (asset: Asset) => void
  onReveal: (asset: Asset) => void
  albums: Album[]
  locations: AssetLocation[]
  locationsLoading: boolean
  onAddToAlbum: (asset: Asset) => void
  onEditTags: (asset: Asset) => void
  onTrashDuplicate: (asset: Asset, location: AssetLocation) => void
}

function Preview({ asset }: { asset: Asset }) {
  const [a, b, c] = asset.palette ?? ['#496b65', '#d6ae78', '#829ba0']
  const imageSource = useAssetImage(asset)
  if (imageSource) return <img src={imageSource} alt={asset.filename} />
  return (
    <div className="demo-photo inspector-demo" style={{ '--tone-a': a, '--tone-b': b, '--tone-c': c } as React.CSSProperties} aria-hidden="true">
      <span className="demo-sky" /><span className="demo-horizon" /><span className="demo-subject" /><span className="demo-grain" />
    </div>
  )
}

export function Inspector({ asset, analyzing, ocring, onFavorite, onAnalyze, onOcr, onReveal, albums, locations, locationsLoading, onAddToAlbum, onEditTags, onTrashDuplicate }: InspectorProps) {
  if (!asset) {
    return (
      <aside className="inspector inspector-empty">
        <span className="inspector-empty-icon"><FileText size={22} /></span>
        <p>选择一张图片查看信息</p>
      </aside>
    )
  }

  return (
    <aside className="inspector">
      <div className="inspector-preview"><Preview asset={asset} /></div>
      <div className="inspector-title-row">
        <div>
          <h2 title={asset.filename}>{asset.filename}</h2>
          <p>{categoryLabel(asset.category)}</p>
        </div>
        <button className="icon-button" type="button" aria-label={asset.favorite ? '取消收藏' : '收藏'} onClick={() => onFavorite(asset)}>
          <Heart size={18} fill={asset.favorite ? 'currentColor' : 'none'} />
        </button>
      </div>

      <section className="inspector-section">
        <h3>图片信息</h3>
        <dl className="metadata-list">
          <div><dt><Camera size={15} />拍摄时间</dt><dd>{formatDateTime(asset.capturedAt)}</dd></div>
          <div><dt><HardDrive size={15} />文件</dt><dd>{formatBytes(asset.fileSize)} · {asset.width} × {asset.height}</dd></div>
          {asset.camera ? <div><dt><Camera size={15} />设备</dt><dd>{asset.camera}</dd></div> : null}
          {asset.location ? <div><dt><MapPin size={15} />地点</dt><dd>{asset.location}</dd></div> : null}
        </dl>
      </section>

      <section className="inspector-section">
        <div className="section-heading-row"><h3>智能内容</h3><BrainCircuit size={15} /></div>
        {asset.description ? <p className="description-copy">{asset.description}</p> : <p className="muted-copy">尚未生成图片描述</p>}
        <div className="tag-list">
          {asset.tags.map((tag) => <span key={tag}>{tag}</span>)}
        </div>
        <div className="inspector-inline-actions">
          <button className="text-button" type="button" onClick={() => onEditTags(asset)}><Tags size={14} />编辑标签</button>
          <button className="text-button" type="button" onClick={() => onAddToAlbum(asset)}><FolderPlus size={14} />加入相册</button>
        </div>
        {asset.ocrText ? <div className="ocr-preview"><ScanText size={15} /><p>{asset.ocrText}</p></div> : null}
        {(asset.category === 'screenshot' || asset.category === 'document') ? (
          <button className="secondary-button full-width inspector-action" type="button" onClick={() => onOcr(asset)} disabled={ocring}>
            <ScanText size={16} />
            {ocring ? '正在提取文字…' : asset.ocrText ? '重新提取文字' : '本地提取文字'}
          </button>
        ) : null}
        <button className="secondary-button full-width" type="button" onClick={() => onAnalyze(asset)} disabled={analyzing}>
          <Sparkles size={16} />
          {analyzing ? '正在分析…' : asset.aiAnalyzed ? '重新分析' : 'AI 分析这张图片'}
        </button>
      </section>

      <section className="inspector-section">
        <h3>所属相册</h3>
        <div className="tag-list album-tag-list">
          {albums.filter((album) => asset.albumIds.includes(album.id)).map((album) => <span key={album.id}>{album.name}</span>)}
          {!asset.albumIds.length ? <p className="muted-copy">尚未加入手动相册</p> : null}
        </div>
      </section>

      {asset.duplicateCount > 0 || asset.similarCount > 0 ? (
        <section className="inspector-section duplicate-section">
          <div className="section-heading-row"><h3>重复与相似</h3><Copy size={15} /></div>
          {asset.similarCount > 0 ? <p className="similar-note">发现 {asset.similarCount} 张视觉相似图片。它们可能是连拍或编辑版本，不会自动删除。</p> : null}
          {asset.duplicateCount > 0 ? (
            <div className="location-list" aria-busy={locationsLoading}>
              {locations.filter((location) => location.available).map((location) => (
                <div className="location-row" key={location.id}>
                  <div><strong>{location.source}</strong><span title={location.path}>{location.path}</span></div>
                  <button className="icon-button danger-icon-button" type="button" aria-label="将这个重复副本移入回收站" onClick={() => onTrashDuplicate(asset, location)} disabled={locations.filter((item) => item.available).length <= 1}><Trash2 size={15} /></button>
                </div>
              ))}
            </div>
          ) : null}
        </section>
      ) : null}

      <section className="inspector-section path-section">
        <h3>文件位置</h3>
        <p title={asset.path}>{asset.path}</p>
        <button className="text-button" type="button" onClick={() => onReveal(asset)}><FolderOpen size={15} />在资源管理器中显示<ExternalLink size={13} /></button>
      </section>
    </aside>
  )
}
