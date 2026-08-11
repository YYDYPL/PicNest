import * as Dialog from '@radix-ui/react-dialog'
import * as Switch from '@radix-ui/react-switch'
import {
  AlertTriangle,
  ArrowLeft,
  ArrowRight,
  CheckCircle2,
  Eye,
  EyeOff,
  FileDown,
  FolderPlus,
  KeyRound,
  LoaderCircle,
  LockKeyhole,
  ShieldCheck,
  Sparkles,
  Tags,
  Trash2,
  Wifi,
  X,
  ZoomIn,
  ZoomOut,
} from 'lucide-react'
import { useEffect, useState } from 'react'
import { formatBytes, formatDateTime } from '../lib/format'
import { useAssetImage } from '../lib/images'
import type { Album, AppSettings, Asset, ConnectionTestResult, OrganizePlan, RecoveryJob, SaveSettingsInput } from '../lib/types'

function CloseButton() {
  return (
    <Dialog.Close asChild>
      <button className="icon-button dialog-close" type="button" aria-label="关闭"><X size={18} /></button>
    </Dialog.Close>
  )
}

export function OrganizeDialog({ open, plan, applying, onOpenChange, onApply }: { open: boolean; plan: OrganizePlan | null; applying: boolean; onOpenChange: (open: boolean) => void; onApply: () => void }) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content className="dialog-content organize-dialog">
          <div className="dialog-header">
            <div><Dialog.Title>预览整理结果</Dialog.Title><Dialog.Description>确认后才会移动原文件，整个操作可以撤销。</Dialog.Description></div>
            <CloseButton />
          </div>

          {plan ? (
            <>
              <div className="plan-summary">
                <div><strong>{plan.items.length}</strong><span>张图片</span></div>
                <div><strong>{formatBytes(plan.totalBytes)}</strong><span>需要移动</span></div>
                <div data-warning={plan.conflicts > 0}><strong>{plan.conflicts}</strong><span>个路径冲突</span></div>
                <div data-warning={!plan.diskSpaceOk}><strong>{plan.requiredCopyBytes ? formatBytes(plan.availableBytes) : '原子移动'}</strong><span>{plan.requiredCopyBytes ? `可用 / 需 ${formatBytes(plan.requiredCopyBytes)}` : '不额外占用空间'}</span></div>
              </div>
              <div className="plan-list" role="list">
                {plan.items.map((item) => (
                  <div className="plan-item" key={item.assetId} role="listitem">
                    <span className="plan-file-icon"><Sparkles size={16} /></span>
                    <div className="plan-paths">
                      <strong>{item.filename}</strong>
                      <span title={item.sourcePath}>{item.sourcePath}</span>
                      <span className="plan-target" title={item.targetPath}><ArrowRight size={13} />{item.targetPath}</span>
                    </div>
                    <span className="reason-badge">{item.reason}</span>
                    {item.conflict ? <AlertTriangle className="warning-icon" size={17} /> : <CheckCircle2 className="success-icon" size={17} />}
                  </div>
                ))}
              </div>
              <div className="safety-note"><ShieldCheck size={17} /><span>跨磁盘移动会先复制并校验内容，PicNest 永不覆盖同名文件。</span></div>
              <div className="dialog-actions">
                <Dialog.Close asChild><button className="secondary-button" type="button" disabled={applying}>取消</button></Dialog.Close>
                <button className="primary-button" type="button" onClick={onApply} disabled={applying || plan.items.length === 0 || !plan.diskSpaceOk}>
                  {applying ? <LoaderCircle className="spin" size={16} /> : <CheckCircle2 size={16} />}
                  {applying ? '正在安全移动…' : `整理 ${plan.items.length} 张图片`}
                </button>
              </div>
            </>
          ) : <div className="dialog-loading"><LoaderCircle className="spin" size={22} />正在生成整理预案…</div>}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  )
}

export function SettingsDialog({ open, settings, saving, testingConnection, connectionResult, exportingDiagnostics, onOpenChange, onSave, onTestConnection, onDeleteApiKey, onClearAiResults, onExportDiagnostics }: { open: boolean; settings: AppSettings; saving: boolean; testingConnection: boolean; connectionResult: ConnectionTestResult | null; exportingDiagnostics: boolean; onOpenChange: (open: boolean) => void; onSave: (input: SaveSettingsInput) => void; onTestConnection: (input: SaveSettingsInput) => void; onDeleteApiKey: () => void; onClearAiResults: () => void; onExportDiagnostics: () => void }) {
  const [draft, setDraft] = useState<SaveSettingsInput>(settings)
  const [showKey, setShowKey] = useState(false)

  useEffect(() => setDraft(settings), [settings, open])

  const update = <K extends keyof SaveSettingsInput,>(key: K, value: SaveSettingsInput[K]) => setDraft((current) => ({ ...current, [key]: value }))

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content className="dialog-content settings-dialog">
          <div className="dialog-header">
            <div><Dialog.Title>设置</Dialog.Title><Dialog.Description>图库、隐私和云端分析选项。</Dialog.Description></div>
            <CloseButton />
          </div>

          <div className="settings-layout">
            <section className="settings-section">
              <h3>本地图库</h3>
              <label className="field"><span>图库位置</span><input value={draft.libraryPath} onChange={(event) => update('libraryPath', event.target.value)} /></label>
              <div className="source-list">
                <span>正在监控的文件夹</span>
                {draft.sourcePaths.map((path) => <div key={path}>{path}</div>)}
              </div>
            </section>

            <section className="settings-section">
              <div className="settings-switch-row">
                <div><h3>云端 AI</h3><p>默认不会自动上传图片。</p></div>
                <Switch.Root className="switch-root" checked={draft.cloudAiEnabled} onCheckedChange={(checked) => update('cloudAiEnabled', checked)} aria-label="启用云端 AI"><Switch.Thumb className="switch-thumb" /></Switch.Root>
              </div>

              <label className="field"><span>兼容接口地址</span><input value={draft.aiBaseUrl} onChange={(event) => update('aiBaseUrl', event.target.value)} disabled={!draft.cloudAiEnabled} /></label>
              <div className="field-grid">
                <label className="field"><span>视觉模型</span><input value={draft.visionModel} onChange={(event) => update('visionModel', event.target.value)} disabled={!draft.cloudAiEnabled} /></label>
                <label className="field"><span>嵌入模型</span><input value={draft.embeddingModel} onChange={(event) => update('embeddingModel', event.target.value)} disabled={!draft.cloudAiEnabled} /></label>
              </div>
              <label className="field"><span>单次批量上限</span><input type="number" min={1} max={100} value={draft.aiBatchLimit} onChange={(event) => update('aiBatchLimit', Number(event.target.value))} disabled={!draft.cloudAiEnabled} /></label>
              <label className="field"><span>API Key</span><div className="secret-input"><KeyRound size={16} /><input type={showKey ? 'text' : 'password'} value={draft.apiKey ?? ''} placeholder={settings.apiKeyConfigured ? '已安全保存；留空表示不修改' : '输入服务商密钥'} onChange={(event) => update('apiKey', event.target.value)} disabled={!draft.cloudAiEnabled} /><button type="button" aria-label={showKey ? '隐藏密钥' : '显示密钥'} onClick={() => setShowKey((value) => !value)}>{showKey ? <EyeOff size={16} /> : <Eye size={16} />}</button></div></label>
              <div className="settings-action-row">
                <button className="secondary-button compact" type="button" onClick={() => onTestConnection(draft)} disabled={!draft.cloudAiEnabled || testingConnection}><Wifi size={15} />{testingConnection ? '正在测试…' : '测试连接'}</button>
                <button className="text-button danger-text" type="button" onClick={onDeleteApiKey} disabled={!settings.apiKeyConfigured}><Trash2 size={14} />移除已保存密钥</button>
              </div>
              {connectionResult ? <p className="connection-result" data-ok={connectionResult.ok}>{connectionResult.message} · {connectionResult.latencyMs} ms</p> : null}
              <div className="privacy-banner"><LockKeyhole size={17} /><p>密钥保存在 Windows 凭据管理器。只有点击“AI 分析”时，去除元数据后的压缩预览才会发送。</p></div>
              <button className="secondary-button compact danger-outline" type="button" onClick={onClearAiResults}><Trash2 size={15} />删除全部 AI 描述与标签</button>
            </section>

            <section className="settings-section">
              <div className="settings-switch-row">
                <div><h3>匿名遥测</h3><p>默认关闭。当前测试版不会自动发送诊断数据。</p></div>
                <Switch.Root className="switch-root" checked={draft.telemetryEnabled} onCheckedChange={(checked) => update('telemetryEnabled', checked)} aria-label="启用匿名遥测"><Switch.Thumb className="switch-thumb" /></Switch.Root>
              </div>
              <div className="diagnostics-row">
                <div><strong>本地诊断包</strong><p>只包含版本、计数和运行状态，不包含路径、密钥或图片内容。</p></div>
                <button className="secondary-button compact" type="button" onClick={onExportDiagnostics} disabled={exportingDiagnostics}><FileDown size={15} />{exportingDiagnostics ? '正在导出…' : '导出'}</button>
              </div>
            </section>
          </div>

          <div className="dialog-actions">
            <Dialog.Close asChild><button className="secondary-button" type="button" disabled={saving}>取消</button></Dialog.Close>
            <button className="primary-button" type="button" onClick={() => onSave(draft)} disabled={saving || !draft.libraryPath.trim()}>{saving ? <LoaderCircle className="spin" size={16} /> : null}保存设置</button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  )
}

function ViewerImage({ asset, zoom }: { asset: Asset; zoom: number }) {
  const [a, b, c] = asset.palette ?? ['#496b65', '#d6ae78', '#829ba0']
  const imageSource = useAssetImage(asset, true)
  if (imageSource) return <img src={imageSource} alt={asset.filename} style={{ transform: `scale(${zoom})` }} />
  return <div className="demo-photo viewer-demo" style={{ '--tone-a': a, '--tone-b': b, '--tone-c': c, transform: `scale(${zoom})` } as React.CSSProperties}><span className="demo-sky" /><span className="demo-horizon" /><span className="demo-subject" /><span className="demo-grain" /></div>
}

export function ViewerDialog({ open, asset, hasPrevious, hasNext, onOpenChange, onPrevious, onNext }: { open: boolean; asset: Asset | null; hasPrevious: boolean; hasNext: boolean; onOpenChange: (open: boolean) => void; onPrevious: () => void; onNext: () => void }) {
  const [zoom, setZoom] = useState(1)
  useEffect(() => setZoom(1), [asset?.id, open])
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="viewer-overlay" />
        <Dialog.Content className="viewer-dialog" onKeyDown={(event) => { if (event.key === 'ArrowLeft') onPrevious(); if (event.key === 'ArrowRight') onNext() }}>
          <Dialog.Title className="sr-only">{asset?.filename ?? '图片查看器'}</Dialog.Title>
          <div className="viewer-toolbar">
            <span>{asset?.filename}</span>
            <div>
              <button className="viewer-icon-button" type="button" aria-label="缩小" onClick={() => setZoom((value) => Math.max(0.5, value - 0.25))}><ZoomOut size={19} /></button>
              <span className="zoom-value">{Math.round(zoom * 100)}%</span>
              <button className="viewer-icon-button" type="button" aria-label="放大" onClick={() => setZoom((value) => Math.min(3, value + 0.25))}><ZoomIn size={19} /></button>
              <Dialog.Close asChild><button className="viewer-icon-button" type="button" aria-label="关闭"><X size={20} /></button></Dialog.Close>
            </div>
          </div>
          <div className="viewer-stage">
            <button className="viewer-nav previous" type="button" aria-label="上一张" onClick={onPrevious} disabled={!hasPrevious}><ArrowLeft size={23} /></button>
            {asset ? <ViewerImage asset={asset} zoom={zoom} /> : null}
            <button className="viewer-nav next" type="button" aria-label="下一张" onClick={onNext} disabled={!hasNext}><ArrowRight size={23} /></button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  )
}

export function CreateAlbumDialog({ open, saving, onOpenChange, onCreate }: { open: boolean; saving: boolean; onOpenChange: (open: boolean) => void; onCreate: (name: string) => void }) {
  const [name, setName] = useState('')
  useEffect(() => { if (!open) setName('') }, [open])
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content className="dialog-content compact-dialog">
          <div className="dialog-header">
            <div><Dialog.Title>新建相册</Dialog.Title><Dialog.Description>相册不会复制或移动原文件。</Dialog.Description></div>
            <CloseButton />
          </div>
          <div className="compact-dialog-body">
            <label className="field"><span>相册名称</span><input autoFocus value={name} maxLength={80} placeholder="例如：韩国旅行" onChange={(event) => setName(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter' && name.trim()) onCreate(name.trim()) }} /></label>
          </div>
          <div className="dialog-actions">
            <Dialog.Close asChild><button className="secondary-button" type="button">取消</button></Dialog.Close>
            <button className="primary-button" type="button" disabled={saving || !name.trim()} onClick={() => onCreate(name.trim())}>{saving ? <LoaderCircle className="spin" size={16} /> : null}创建相册</button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  )
}

export function AlbumPickerDialog({ open, albums, count, saving, onOpenChange, onAssign }: { open: boolean; albums: Album[]; count: number; saving: boolean; onOpenChange: (open: boolean) => void; onAssign: (albumId: number) => void }) {
  const manualAlbums = albums.filter((album) => album.kind === 'manual')
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content className="dialog-content compact-dialog">
          <div className="dialog-header">
            <div><Dialog.Title>加入相册</Dialog.Title><Dialog.Description>为已选择的 {count} 张图片建立关联，不移动原文件。</Dialog.Description></div>
            <CloseButton />
          </div>
          <div className="picker-list">
            {manualAlbums.map((album) => (
              <button key={album.id} type="button" onClick={() => onAssign(album.id)} disabled={saving}>
                <span><FolderPlus size={17} /></span><strong>{album.name}</strong><small>{album.count} 张</small>
              </button>
            ))}
            {!manualAlbums.length ? <p className="picker-empty">请先在“相册”视图中新建手动相册。</p> : null}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  )
}

export function TagDialog({ open, count, initialTags, saving, onOpenChange, onSave }: { open: boolean; count: number; initialTags: string[]; saving: boolean; onOpenChange: (open: boolean) => void; onSave: (tags: string[]) => void }) {
  const [value, setValue] = useState('')
  useEffect(() => { if (open) setValue(initialTags.join('，')) }, [initialTags, open])
  const parsedTags = Array.from(new Set(value.split(/[,，\n]/).map((tag) => tag.trim()).filter(Boolean))).slice(0, 40)
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content className="dialog-content compact-dialog">
          <div className="dialog-header">
            <div><Dialog.Title>编辑标签</Dialog.Title><Dialog.Description>将覆盖已选择 {count} 张图片的本地标签。</Dialog.Description></div>
            <CloseButton />
          </div>
          <div className="compact-dialog-body">
            <label className="field"><span>标签（逗号或换行分隔）</span><textarea autoFocus value={value} rows={4} maxLength={1200} placeholder="旅行，海边，日落" onChange={(event) => setValue(event.target.value)} /></label>
            <div className="tag-list tag-dialog-preview">{parsedTags.map((tag) => <span key={tag}>{tag}</span>)}</div>
          </div>
          <div className="dialog-actions">
            <Dialog.Close asChild><button className="secondary-button" type="button">取消</button></Dialog.Close>
            <button className="primary-button" type="button" disabled={saving} onClick={() => onSave(parsedTags)}>{saving ? <LoaderCircle className="spin" size={16} /> : <Tags size={16} />}保存标签</button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  )
}

export function RecoveryDialog({ open, jobs, busyPlanId, onOpenChange, onResume, onRollback }: { open: boolean; jobs: RecoveryJob[]; busyPlanId: string | null; onOpenChange: (open: boolean) => void; onResume: (planId: string) => void; onRollback: (planId: string) => void }) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content className="dialog-content recovery-dialog">
          <div className="dialog-header">
            <div><Dialog.Title>发现未完成的整理任务</Dialog.Title><Dialog.Description>PicNest 已按文件哈希核对现状。你可以继续剩余步骤，或恢复到原位置。</Dialog.Description></div>
            <CloseButton />
          </div>
          <div className="recovery-list">
            {jobs.map((job) => (
              <div className="recovery-row" key={job.planId}>
                <span className="recovery-mark"><ShieldCheck size={18} /></span>
                <div><strong>{job.moved} 张已移动 · {job.remaining} 张待继续 · {job.failed} 张需复查</strong><time>{formatDateTime(job.createdAt)}</time></div>
                <button className="secondary-button compact" type="button" onClick={() => onRollback(job.planId)} disabled={Boolean(busyPlanId)}>回滚</button>
                <button className="primary-button compact" type="button" onClick={() => onResume(job.planId)} disabled={Boolean(busyPlanId)}>{busyPlanId === job.planId ? <LoaderCircle className="spin" size={15} /> : null}继续</button>
              </div>
            ))}
          </div>
          <div className="dialog-actions"><Dialog.Close asChild><button className="secondary-button" type="button" disabled={Boolean(busyPlanId)}>稍后处理</button></Dialog.Close></div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  )
}
