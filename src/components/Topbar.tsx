import * as Tooltip from '@radix-ui/react-tooltip'
import {
  FolderPlus,
  PanelRightClose,
  PanelRightOpen,
  RotateCcw,
  Search,
  Sparkles,
  Tags,
  X,
} from 'lucide-react'

interface TopbarProps {
  title: string
  total: number
  search: string
  selectedCount: number
  inspectorOpen: boolean
  canUndo: boolean
  busy: boolean
  onSearch: (value: string) => void
  onAddSource: () => void
  onOrganize: () => void
  onAddToAlbum: () => void
  onEditTags: () => void
  onUndo: () => void
  onClearSelection: () => void
  onToggleInspector: () => void
}

function IconButton({ label, children, onClick, disabled = false }: { label: string; children: React.ReactNode; onClick: () => void; disabled?: boolean }) {
  return (
    <Tooltip.Root>
      <Tooltip.Trigger asChild>
        <button className="icon-button" type="button" aria-label={label} onClick={onClick} disabled={disabled}>
          {children}
        </button>
      </Tooltip.Trigger>
      <Tooltip.Portal><Tooltip.Content className="tooltip" sideOffset={8}>{label}<Tooltip.Arrow className="tooltip-arrow" /></Tooltip.Content></Tooltip.Portal>
    </Tooltip.Root>
  )
}

export function Topbar(props: TopbarProps) {
  const selectionMode = props.selectedCount > 0
  return (
    <header className="topbar">
      {selectionMode ? (
        <div className="selection-toolbar">
          <IconButton label="取消选择" onClick={props.onClearSelection}><X size={18} /></IconButton>
          <strong>已选择 {props.selectedCount} 张</strong>
          <button className="primary-button compact" type="button" onClick={props.onOrganize} disabled={props.busy}>
            <Sparkles size={16} aria-hidden="true" />
            预览整理
          </button>
          <IconButton label="加入相册" onClick={props.onAddToAlbum} disabled={props.busy}><FolderPlus size={18} /></IconButton>
          <IconButton label="编辑标签" onClick={props.onEditTags} disabled={props.busy}><Tags size={18} /></IconButton>
        </div>
      ) : (
        <div className="view-heading">
          <h1>{props.title}</h1>
          <span>{props.total} 张</span>
        </div>
      )}

      <label className="search-box">
        <Search size={17} strokeWidth={1.8} aria-hidden="true" />
        <input
          value={props.search}
          onChange={(event) => props.onSearch(event.target.value)}
          placeholder="搜索照片内容、文字或地点"
          aria-label="搜索图片"
        />
        {props.search ? <button type="button" aria-label="清空搜索" onClick={() => props.onSearch('')}><X size={15} /></button> : null}
      </label>

      <div className="topbar-actions">
        <IconButton label="撤销上次整理" onClick={props.onUndo} disabled={!props.canUndo || props.busy}><RotateCcw size={18} /></IconButton>
        <button className="secondary-button compact" type="button" onClick={props.onAddSource} disabled={props.busy}>
          <FolderPlus size={16} aria-hidden="true" />
          添加文件夹
        </button>
        <IconButton label={props.inspectorOpen ? '隐藏信息面板' : '显示信息面板'} onClick={props.onToggleInspector}>
          {props.inspectorOpen ? <PanelRightClose size={18} /> : <PanelRightOpen size={18} />}
        </IconButton>
      </div>
    </header>
  )
}
