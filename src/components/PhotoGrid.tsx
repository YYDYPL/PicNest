import { useVirtualizer } from '@tanstack/react-virtual'
import { Check, Copy, FileQuestion, Heart, LoaderCircle, Sparkles } from 'lucide-react'
import { useEffect, useMemo, useRef, useState } from 'react'
import { categoryLabel, monthKey, monthLabel } from '../lib/format'
import { useAssetImage } from '../lib/images'
import type { Asset } from '../lib/types'

interface PhotoGridProps {
  assets: Asset[]
  selectedIds: number[]
  activeAssetId: number | null
  emptyTitle: string
  emptyBody: string
  onSelect: (asset: Asset, additive: boolean) => void
  onOpen: (asset: Asset) => void
  hasMore: boolean
  loadingMore: boolean
  onLoadMore: () => void
}

type GridRow =
  | { kind: 'heading'; key: string; label: string; count: number }
  | { kind: 'photos'; key: string; assets: Asset[] }

function useElementWidth(ref: React.RefObject<HTMLElement | null>) {
  const [width, setWidth] = useState(900)
  useEffect(() => {
    if (!ref.current) return
    const observer = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width))
    observer.observe(ref.current)
    return () => observer.disconnect()
  }, [ref])
  return width
}

function DemoPhoto({ asset }: { asset: Asset }) {
  const [a, b, c] = asset.palette ?? ['#496b65', '#d6ae78', '#829ba0']
  return (
    <div className="demo-photo" style={{ '--tone-a': a, '--tone-b': b, '--tone-c': c } as React.CSSProperties} aria-hidden="true">
      <span className="demo-sky" />
      <span className="demo-horizon" />
      <span className="demo-subject" />
      <span className="demo-grain" />
    </div>
  )
}

function PhotoCard({ asset, selected, active, onSelect, onOpen }: { asset: Asset; selected: boolean; active: boolean; onSelect: (asset: Asset, additive: boolean) => void; onOpen: (asset: Asset) => void }) {
  const imageSource = useAssetImage(asset)
  return (
    <article
      className="photo-card"
      data-selected={selected}
      data-active={active}
      onClick={(event) => onSelect(asset, event.ctrlKey || event.metaKey)}
      onDoubleClick={() => onOpen(asset)}
    >
      <button className="photo-hit-target" type="button" aria-label={`选择 ${asset.filename}`} />
      <div className="photo-frame">
        {imageSource ? <img src={imageSource} alt={asset.filename} loading="lazy" /> : <DemoPhoto asset={asset} />}
        {asset.missing ? <span className="photo-status missing"><FileQuestion size={14} /> 文件缺失</span> : null}
        {asset.aiAnalyzed ? <span className="photo-ai" title="已有 AI 描述"><Sparkles size={13} /></span> : null}
        {asset.favorite ? <Heart className="photo-favorite" size={15} fill="currentColor" aria-label="已收藏" /> : null}
        {asset.duplicateCount > 0 || asset.similarCount > 0 ? <span className="photo-duplicate" title={asset.duplicateCount > 0 ? `${asset.duplicateCount} 个完全重复副本` : `${asset.similarCount} 张相似图片`}><Copy size={13} /></span> : null}
        <span className="selection-check"><Check size={14} strokeWidth={3} /></span>
      </div>
      <div className="photo-meta">
        <span title={asset.filename}>{asset.filename}</span>
        <small>{categoryLabel(asset.category)}</small>
      </div>
    </article>
  )
}

export function PhotoGrid(props: PhotoGridProps) {
  const { hasMore, loadingMore, onLoadMore } = props
  const parentRef = useRef<HTMLDivElement>(null)
  const width = useElementWidth(parentRef)
  const columns = width > 1260 ? 6 : width > 1000 ? 5 : width > 760 ? 4 : 3

  const rows = useMemo<GridRow[]>(() => {
    const groups = new Map<string, Asset[]>()
    for (const asset of props.assets) {
      const key = monthKey(asset.capturedAt)
      groups.set(key, [...(groups.get(key) ?? []), asset])
    }
    const result: GridRow[] = []
    for (const [key, assets] of groups) {
      result.push({ kind: 'heading', key: `heading-${key}`, label: monthLabel(key), count: assets.length })
      for (let index = 0; index < assets.length; index += columns) {
        result.push({ kind: 'photos', key: `${key}-${index}`, assets: assets.slice(index, index + columns) })
      }
    }
    return result
  }, [props.assets, columns])

  const photoRowHeight = Math.max(154, Math.min(208, ((width - 60 - (columns - 1) * 9) / columns) * 0.78 + 42))
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (index) => rows[index]?.kind === 'heading' ? 55 : photoRowHeight,
    overscan: 5,
  })
  const virtualItems = virtualizer.getVirtualItems()
  const lastVirtualIndex = virtualItems.at(-1)?.index ?? -1

  useEffect(() => {
    if (hasMore && !loadingMore && lastVirtualIndex >= rows.length - 3) onLoadMore()
  }, [hasMore, lastVirtualIndex, loadingMore, onLoadMore, rows.length])

  if (!props.assets.length) {
    return (
      <div className="empty-state">
        <span className="empty-mark"><Sparkles size={24} /></span>
        <h2>{props.emptyTitle}</h2>
        <p>{props.emptyBody}</p>
      </div>
    )
  }

  return (
    <div className="photo-scroll" ref={parentRef} aria-label={`照片时间轴，共 ${props.assets.length} 张`}>
      <div className="virtual-canvas" style={{ height: virtualizer.getTotalSize() }}>
        {virtualItems.map((virtualRow) => {
          const row = rows[virtualRow.index]
          if (!row) return null
          return (
            <div
              key={row.key}
              className={row.kind === 'heading' ? 'timeline-heading-row' : 'photo-grid-row'}
              style={{ transform: `translateY(${virtualRow.start}px)`, height: virtualRow.size }}
            >
              {row.kind === 'heading' ? (
                <>
                  <div className="date-rail"><span /></div>
                  <h2>{row.label}</h2>
                  <small>{row.count} 张</small>
                </>
              ) : (
                <div className="photo-row-inner" style={{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}>
                  {row.assets.map((asset) => (
                    <PhotoCard
                      key={asset.id}
                      asset={asset}
                      selected={props.selectedIds.includes(asset.id)}
                      active={props.activeAssetId === asset.id}
                      onSelect={props.onSelect}
                      onOpen={props.onOpen}
                    />
                  ))}
                </div>
              )}
            </div>
          )
        })}
      </div>
      {props.loadingMore ? <div className="load-more-status"><LoaderCircle className="spin" size={15} />正在载入更多图片</div> : null}
    </div>
  )
}
