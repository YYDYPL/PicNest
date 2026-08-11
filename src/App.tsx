import * as Tooltip from '@radix-ui/react-tooltip'
import { AlertCircle, CheckCircle2, Inbox, LoaderCircle, ScanSearch, Sparkles, X } from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import './App.css'
import { AlbumPickerDialog, CreateAlbumDialog, OrganizeDialog, RecoveryDialog, SettingsDialog, TagDialog, ViewerDialog } from './components/Dialogs'
import { Inspector } from './components/Inspector'
import { Onboarding } from './components/Onboarding'
import { PhotoGrid } from './components/PhotoGrid'
import { Sidebar } from './components/Sidebar'
import { ActivityView, AlbumOverview } from './components/SpecialViews'
import { Topbar } from './components/Topbar'
import { bridge, mergeAssetUpdate } from './lib/bridge'
import { demoSettings } from './lib/demo'
import type {
  ActivityItem,
  Album,
  AppSettings,
  Asset,
  AssetLocation,
  BootstrapPayload,
  ConnectionTestResult,
  LibraryStats,
  OrganizePlan,
  RecoveryJob,
  SaveSettingsInput,
  ViewId,
} from './lib/types'
import { useLibraryUi } from './store/library'

const viewCopy: Record<ViewId, { title: string; emptyTitle: string; emptyBody: string }> = {
  inbox: { title: '待整理', emptyTitle: '收件箱已经清空', emptyBody: '新的桌面图片和下载图片会继续出现在这里。' },
  all: { title: '全部图片', emptyTitle: '图库里还没有图片', emptyBody: '添加一个文件夹，PicNest 会在本地建立索引。' },
  recent: { title: '最近导入', emptyTitle: '最近没有导入图片', emptyBody: '新扫描的图片会按导入时间显示在这里。' },
  albums: { title: '相册', emptyTitle: '还没有相册', emptyBody: '创建相册来组织旅行、项目和生活记录。' },
  favorites: { title: '收藏', emptyTitle: '还没有收藏', emptyBody: '点击图片信息面板中的心形按钮即可收藏。' },
  duplicates: { title: '重复图片', emptyTitle: '没有发现重复图片', emptyBody: 'PicNest 会同时检查完全重复和视觉相似的图片。' },
  missing: { title: '缺失文件', emptyTitle: '没有缺失文件', emptyBody: '所有已索引的文件目前都可以访问。' },
  history: { title: '整理记录', emptyTitle: '还没有整理记录', emptyBody: '扫描、移动和撤销操作会记录在这里。' },
  album: { title: '相册', emptyTitle: '这个相册还是空的', emptyBody: '选择图片后，可以从顶部工具栏将它们加入相册。' },
}

interface ToastState {
  id: number
  tone: 'success' | 'error' | 'info'
  message: string
}

const emptyStats: LibraryStats = { total: 0, inbox: 0, favorites: 0, duplicates: 0, missing: 0, albums: 0, storageBytes: 0 }

export default function App() {
  const ui = useLibraryUi()
  const [boot, setBoot] = useState<BootstrapPayload | null>(null)
  const [settings, setSettings] = useState<AppSettings>(demoSettings)
  const [stats, setStats] = useState<LibraryStats>(emptyStats)
  const [albums, setAlbums] = useState<Album[]>([])
  const [activity, setActivity] = useState<ActivityItem[]>([])
  const [assets, setAssets] = useState<Asset[]>([])
  const [total, setTotal] = useState(0)
  const [nextCursor, setNextCursor] = useState<number | null>(null)
  const [loading, setLoading] = useState(true)
  const [loadingMore, setLoadingMore] = useState(false)
  const [busyLabel, setBusyLabel] = useState<string | null>(null)
  const [scanning, setScanning] = useState(false)
  const [organizePlan, setOrganizePlan] = useState<OrganizePlan | null>(null)
  const [applying, setApplying] = useState(false)
  const [analyzingId, setAnalyzingId] = useState<number | null>(null)
  const [ocringId, setOcringId] = useState<number | null>(null)
  const [savingSettings, setSavingSettings] = useState(false)
  const [albumDialogOpen, setAlbumDialogOpen] = useState(false)
  const [savingAlbum, setSavingAlbum] = useState(false)
  const [activeAlbum, setActiveAlbum] = useState<Album | null>(null)
  const [albumPickerOpen, setAlbumPickerOpen] = useState(false)
  const [savingAlbumAssignment, setSavingAlbumAssignment] = useState(false)
  const [tagDialogOpen, setTagDialogOpen] = useState(false)
  const [savingTags, setSavingTags] = useState(false)
  const [dialogAssetIds, setDialogAssetIds] = useState<number[]>([])
  const [tagDialogInitial, setTagDialogInitial] = useState<string[]>([])
  const [assetLocations, setAssetLocations] = useState<AssetLocation[]>([])
  const [locationsLoading, setLocationsLoading] = useState(false)
  const [recoveryJobs, setRecoveryJobs] = useState<RecoveryJob[]>([])
  const [recoveryOpen, setRecoveryOpen] = useState(false)
  const [recoveryBusyPlanId, setRecoveryBusyPlanId] = useState<string | null>(null)
  const [testingConnection, setTestingConnection] = useState(false)
  const [connectionResult, setConnectionResult] = useState<ConnectionTestResult | null>(null)
  const [exportingDiagnostics, setExportingDiagnostics] = useState(false)
  const [showDemoBanner, setShowDemoBanner] = useState(true)
  const [canUndo, setCanUndo] = useState(false)
  const [toast, setToast] = useState<ToastState | null>(null)

  const notify = useCallback((message: string, tone: ToastState['tone'] = 'info') => {
    setToast({ id: Date.now(), message, tone })
  }, [])

  useEffect(() => {
    if (!toast) return
    const timer = window.setTimeout(() => setToast(null), 3600)
    return () => window.clearTimeout(timer)
  }, [toast])

  const refreshBootstrap = useCallback(async () => {
    const payload = await bridge.bootstrap()
    setBoot(payload)
    setSettings(payload.settings)
    setStats(payload.stats)
    setAlbums(payload.albums)
    setActivity(payload.recentActivity)
    setRecoveryJobs(payload.recoveryJobs)
    setRecoveryOpen(payload.recoveryJobs.length > 0)
    setCanUndo(payload.recentActivity.some((item) => item.reversible))
    return payload
  }, [])

  const loadAssets = useCallback(async (view = ui.activeView, search = ui.search, cursor: number | null = null) => {
    if (view === 'albums' || view === 'history') return
    if (view === 'album' && !activeAlbum) return
    const appending = cursor !== null
    if (appending) setLoadingMore(true)
    else setLoading(true)
    try {
      const page = await bridge.listAssets({ view, search, limit: 240, cursor, albumId: view === 'album' ? activeAlbum?.id : null })
      setAssets((current) => {
        if (!appending) return page.items
        const known = new Set(current.map((asset) => asset.id))
        return [...current, ...page.items.filter((asset) => !known.has(asset.id))]
      })
      setTotal(page.total)
      setNextCursor(page.nextCursor)
      if (!appending && page.items.length && ui.selectedAssetId && !page.items.some((asset) => asset.id === ui.selectedAssetId)) ui.selectAsset(null)
    } catch (error) {
      notify(error instanceof Error ? error.message : '无法读取图库', 'error')
    } finally {
      if (appending) setLoadingMore(false)
      else setLoading(false)
    }
  }, [activeAlbum, notify, ui])

  useEffect(() => {
    void refreshBootstrap()
      .then((payload) => {
        if (payload.settings.configured) return loadAssets()
      })
      .catch((error) => notify(error instanceof Error ? error.message : 'PicNest 启动失败', 'error'))
      .finally(() => setLoading(false))
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (!boot?.settings.configured || ui.activeView === 'albums' || ui.activeView === 'history') return
    const timer = window.setTimeout(() => void loadAssets(ui.activeView, ui.search), ui.search ? 180 : 0)
    return () => window.clearTimeout(timer)
  }, [ui.activeView, ui.search, activeAlbum?.id, boot?.settings.configured]) // eslint-disable-line react-hooks/exhaustive-deps

  const selectedAsset = useMemo(() => assets.find((asset) => asset.id === ui.selectedAssetId) ?? null, [assets, ui.selectedAssetId])
  const selectedAssetId = selectedAsset?.id ?? null
  const selectedDuplicateCount = selectedAsset?.duplicateCount ?? 0
  const viewerIndex = selectedAsset ? assets.findIndex((asset) => asset.id === selectedAsset.id) : -1
  const inboxCounts = useMemo(() => ({
    screenshots: assets.filter((asset) => asset.category === 'screenshot').length,
    wechat: assets.filter((asset) => asset.category === 'wechat').length,
    camera: assets.filter((asset) => asset.category === 'camera').length,
    duplicates: assets.filter((asset) => asset.duplicateCount > 0 || asset.similarCount > 0).length,
  }), [assets])

  useEffect(() => {
    let active = true
    if (!selectedAssetId || selectedDuplicateCount === 0) {
      setAssetLocations([])
      return () => { active = false }
    }
    setLocationsLoading(true)
    void bridge.listAssetLocations(selectedAssetId)
      .then((locations) => { if (active) setAssetLocations(locations) })
      .catch((error) => notify(error instanceof Error ? error.message : '无法读取重复副本', 'error'))
      .finally(() => { if (active) setLocationsLoading(false) })
    return () => { active = false }
  }, [notify, selectedAssetId, selectedDuplicateCount])

  const reloadAll = useCallback(async () => {
    const payload = await refreshBootstrap()
    if (payload.settings.configured) await loadAssets()
  }, [loadAssets, refreshBootstrap])
  const reloadRef = useRef(reloadAll)
  useEffect(() => { reloadRef.current = reloadAll }, [reloadAll])

  const loadMore = useCallback(() => {
    if (nextCursor === null || loadingMore) return
    void loadAssets(ui.activeView, ui.search, nextCursor)
  }, [loadAssets, loadingMore, nextCursor, ui.activeView, ui.search])

  useEffect(() => {
    let unlisten: (() => void) | undefined
    let timer: number | undefined
    void bridge.onLibraryChanged(() => {
      window.clearTimeout(timer)
      timer = window.setTimeout(() => void reloadRef.current(), 500)
    }).then((dispose) => { unlisten = dispose })
    return () => {
      window.clearTimeout(timer)
      unlisten?.()
    }
  }, [])

  const changeView = (view: ViewId) => {
    if (view !== 'album') setActiveAlbum(null)
    ui.setActiveView(view)
  }

  const openAlbum = (album: Album) => {
    setActiveAlbum(album)
    ui.setActiveView('album')
  }

  const handleAddSource = async () => {
    const paths = await bridge.pickSourceFolders()
    if (!paths.length) return
    setBusyLabel('正在扫描新文件夹')
    setScanning(true)
    try {
      const nextSources = Array.from(new Set([...settings.sourcePaths, ...paths]))
      await bridge.saveSettings({ ...settings, sourcePaths: nextSources })
      const result = await bridge.scanPaths(paths)
      await reloadAll()
      notify(result.cancelled ? `扫描已停止，已保留 ${result.indexed} 张索引` : `已索引 ${result.indexed} 张图片，跳过 ${result.skipped} 张未变化文件`, result.cancelled ? 'info' : 'success')
    } catch (error) {
      notify(error instanceof Error ? error.message : '扫描失败', 'error')
    } finally {
      setScanning(false)
      setBusyLabel(null)
    }
  }

  const cancelScan = async () => {
    const cancelling = await bridge.cancelScan()
    if (cancelling) notify('正在安全停止扫描，已完成的索引会保留')
  }

  const openOrganize = async (ids?: number[]) => {
    const assetIds = ids?.length ? ids : ui.selectedIds.length ? ui.selectedIds : assets.filter((asset) => asset.needsOrganize).map((asset) => asset.id)
    if (!assetIds.length) return notify('当前没有需要整理的图片')
    setOrganizePlan(null)
    ui.setOrganizeOpen(true)
    try {
      setOrganizePlan(await bridge.createOrganizePlan(assetIds))
    } catch (error) {
      ui.setOrganizeOpen(false)
      notify(error instanceof Error ? error.message : '无法生成整理预案', 'error')
    }
  }

  const applyPlan = async () => {
    if (!organizePlan) return
    setApplying(true)
    try {
      const result = await bridge.applyOrganizePlan(organizePlan.id)
      ui.setOrganizeOpen(false)
      ui.clearSelection()
      setCanUndo(result.moved > 0)
      await reloadAll()
      notify(`已安全整理 ${result.moved} 张图片${result.failed ? `，${result.failed} 张未移动` : ''}`, result.failed ? 'info' : 'success')
    } catch (error) {
      notify(error instanceof Error ? error.message : '整理操作失败，原文件未被删除', 'error')
    } finally {
      setApplying(false)
    }
  }

  const undo = async () => {
    setBusyLabel('正在撤销上次整理')
    try {
      const count = await bridge.undoLastOperation()
      setCanUndo(false)
      await reloadAll()
      notify(count ? `已将 ${count} 张图片移回原位置` : '没有可以撤销的操作', count ? 'success' : 'info')
    } catch (error) {
      notify(error instanceof Error ? error.message : '无法撤销：文件可能已被修改', 'error')
    } finally {
      setBusyLabel(null)
    }
  }

  const toggleFavorite = async (asset: Asset) => {
    const favorite = !asset.favorite
    setAssets((items) => mergeAssetUpdate(items, asset.id, { favorite }))
    await bridge.toggleFavorite(asset.id, favorite)
    setStats((current) => ({ ...current, favorites: Math.max(0, current.favorites + (favorite ? 1 : -1)) }))
  }

  const analyze = async (asset: Asset) => {
    if (!boot?.demoMode && (!settings.cloudAiEnabled || !settings.apiKeyConfigured)) {
      ui.setSettingsOpen(true)
      return notify('请先在设置中启用云端 AI 并保存 API Key')
    }
    setAnalyzingId(asset.id)
    try {
      const result = await bridge.analyzeAsset(asset.id)
      setAssets((items) => mergeAssetUpdate(items, asset.id, { aiAnalyzed: true, description: result.description, tags: result.tags }))
      notify('AI 描述和标签已保存到本地索引', 'success')
    } catch (error) {
      notify(error instanceof Error ? error.message : 'AI 分析失败，本地功能不受影响', 'error')
    } finally {
      setAnalyzingId(null)
    }
  }

  const extractText = async (asset: Asset) => {
    setOcringId(asset.id)
    try {
      const ocrText = await bridge.ocrAsset(asset.id)
      setAssets((items) => mergeAssetUpdate(items, asset.id, { ocrText }))
      notify('文字已提取并加入本地搜索索引', 'success')
    } catch (error) {
      notify(error instanceof Error ? error.message : '本地 OCR 失败', 'error')
    } finally {
      setOcringId(null)
    }
  }

  const createAlbum = async (name: string) => {
    setSavingAlbum(true)
    try {
      await bridge.createAlbum(name)
      const payload = await refreshBootstrap()
      setAlbums(payload.albums)
      setAlbumDialogOpen(false)
      notify(`已创建相册“${name}”`, 'success')
    } catch (error) {
      notify(error instanceof Error ? error.message : '无法创建相册', 'error')
    } finally {
      setSavingAlbum(false)
    }
  }

  const openAlbumPicker = (ids: number[]) => {
    if (!ids.length) return
    setDialogAssetIds(ids)
    setAlbumPickerOpen(true)
  }

  const assignToAlbum = async (albumId: number) => {
    setSavingAlbumAssignment(true)
    try {
      const added = await bridge.assignAssetsToAlbum(albumId, dialogAssetIds)
      setAssets((items) => items.map((asset) => dialogAssetIds.includes(asset.id) && !asset.albumIds.includes(albumId) ? { ...asset, albumIds: [...asset.albumIds, albumId] } : asset))
      await refreshBootstrap()
      setAlbumPickerOpen(false)
      notify(`已将 ${added} 张图片加入相册`, 'success')
    } catch (error) {
      notify(error instanceof Error ? error.message : '无法加入相册', 'error')
    } finally {
      setSavingAlbumAssignment(false)
    }
  }

  const openTagEditor = (ids: number[]) => {
    if (!ids.length) return
    setDialogAssetIds(ids)
    const selected = assets.filter((asset) => ids.includes(asset.id))
    const shared = selected.length ? selected[0].tags.filter((tag) => selected.every((asset) => asset.tags.includes(tag))) : []
    setTagDialogInitial(shared)
    setTagDialogOpen(true)
  }

  const saveTags = async (tags: string[]) => {
    setSavingTags(true)
    try {
      const updated = await bridge.setAssetTags(dialogAssetIds, tags)
      setAssets((items) => items.map((asset) => dialogAssetIds.includes(asset.id) ? { ...asset, tags } : asset))
      setTagDialogOpen(false)
      notify(`已更新 ${updated} 张图片的标签`, 'success')
    } catch (error) {
      notify(error instanceof Error ? error.message : '标签保存失败', 'error')
    } finally {
      setSavingTags(false)
    }
  }

  const trashDuplicate = async (asset: Asset, location: AssetLocation) => {
    if (!window.confirm(`将这个重复副本移入系统回收站？\n\n${location.path}`)) return
    try {
      await bridge.moveDuplicateToTrash(asset.id, location.path)
      await reloadAll()
      const locations = await bridge.listAssetLocations(asset.id)
      setAssetLocations(locations)
      notify('重复副本已移入系统回收站', 'success')
    } catch (error) {
      notify(error instanceof Error ? error.message : '无法处理重复副本', 'error')
    }
  }

  const saveSettings = async (input: SaveSettingsInput) => {
    setSavingSettings(true)
    try {
      const saved = await bridge.saveSettings(input)
      setSettings(saved)
      setConnectionResult(null)
      setBoot((current) => current ? { ...current, settings: saved } : current)
      ui.setSettingsOpen(false)
      notify('设置已保存', 'success')
    } catch (error) {
      notify(error instanceof Error ? error.message : '设置保存失败', 'error')
    } finally {
      setSavingSettings(false)
    }
  }

  const testAiConnection = async (input: SaveSettingsInput) => {
    setTestingConnection(true)
    setConnectionResult(null)
    try {
      const result = await bridge.testAiConnection({ baseUrl: input.aiBaseUrl, model: input.visionModel, apiKey: input.apiKey })
      setConnectionResult(result)
    } catch (error) {
      setConnectionResult({ ok: false, latencyMs: 0, message: error instanceof Error ? error.message : '连接测试失败' })
    } finally {
      setTestingConnection(false)
    }
  }

  const deleteApiKey = async () => {
    if (!window.confirm('移除 Windows 凭据管理器中保存的 API Key？')) return
    try {
      const saved = await bridge.deleteApiKey()
      setSettings(saved)
      notify('已移除保存的 API Key', 'success')
    } catch (error) {
      notify(error instanceof Error ? error.message : '无法移除 API Key', 'error')
    }
  }

  const clearAiResults = async () => {
    if (!window.confirm('删除全部本地 AI 描述与标签？OCR、原图和普通索引不会受影响。')) return
    try {
      const count = await bridge.clearAiResults([])
      await reloadAll()
      notify(`已删除 ${count} 张图片的 AI 分析结果`, 'success')
    } catch (error) {
      notify(error instanceof Error ? error.message : '无法删除 AI 结果', 'error')
    }
  }

  const exportDiagnostics = async () => {
    const directory = await bridge.pickDiagnosticsFolder()
    if (!directory) return
    setExportingDiagnostics(true)
    try {
      const result = await bridge.exportDiagnostics(directory)
      await bridge.revealPath(result.path)
      notify('脱敏诊断包已导出', 'success')
    } catch (error) {
      notify(error instanceof Error ? error.message : '诊断包导出失败', 'error')
    } finally {
      setExportingDiagnostics(false)
    }
  }

  const finishOnboarding = async (nextSettings: AppSettings) => {
    const saved = await bridge.saveSettings(nextSettings)
    setSettings(saved)
    setBusyLabel('正在建立本地索引')
    setScanning(true)
    try {
      const result = await bridge.scanPaths(saved.sourcePaths)
      await refreshBootstrap()
      await loadAssets('inbox', '')
      if (result.cancelled) notify('扫描已停止，已完成的本地索引会保留')
    } finally {
      setScanning(false)
      setBusyLabel(null)
    }
  }

  const resumeRecovery = async (planId: string) => {
    setRecoveryBusyPlanId(planId)
    try {
      const result = await bridge.applyOrganizePlan(planId)
      await reloadAll()
      notify(`已继续整理 ${result.moved} 张图片`, result.failed ? 'info' : 'success')
    } catch (error) {
      notify(error instanceof Error ? error.message : '无法继续整理任务', 'error')
    } finally {
      setRecoveryBusyPlanId(null)
    }
  }

  const rollbackRecovery = async (planId: string) => {
    setRecoveryBusyPlanId(planId)
    try {
      const count = await bridge.rollbackOrganizePlan(planId)
      await reloadAll()
      notify(`已恢复 ${count} 张图片到原位置`, 'success')
    } catch (error) {
      notify(error instanceof Error ? error.message : '无法回滚整理任务', 'error')
    } finally {
      setRecoveryBusyPlanId(null)
    }
  }

  const navigateViewer = (direction: -1 | 1) => {
    const next = assets[viewerIndex + direction]
    if (next) ui.selectAsset(next.id)
  }

  if (!boot && loading) {
    return <div className="splash-screen"><span className="brand-mark"><span /><span /><span /></span><strong>PicNest</strong><LoaderCircle className="spin" size={19} /></div>
  }

  if (boot && !boot.settings.configured) {
    return (
      <Onboarding
        defaults={boot.settings}
        onPickLibrary={bridge.pickLibraryFolder}
        onPickSources={bridge.pickSourceFolders}
        onFinish={finishOnboarding}
      />
    )
  }

  const copy = ui.activeView === 'album' && activeAlbum
    ? { ...viewCopy.album, title: activeAlbum.name }
    : viewCopy[ui.activeView]

  return (
    <Tooltip.Provider delayDuration={350}>
      <div className="app-shell">
        <Sidebar activeView={ui.activeView === 'album' ? 'albums' : ui.activeView} stats={stats} onViewChange={changeView} onSettings={() => ui.setSettingsOpen(true)} />
        <div className="workspace">
          <Topbar
            title={copy.title}
            total={ui.activeView === 'albums' ? albums.length : ui.activeView === 'history' ? activity.length : total}
            search={ui.search}
            selectedCount={ui.selectedIds.length}
            inspectorOpen={ui.inspectorOpen}
            canUndo={canUndo}
            busy={Boolean(busyLabel)}
            onSearch={ui.setSearch}
            onAddSource={handleAddSource}
            onOrganize={() => openOrganize()}
            onAddToAlbum={() => openAlbumPicker(ui.selectedIds)}
            onEditTags={() => openTagEditor(ui.selectedIds)}
            onUndo={undo}
            onClearSelection={ui.clearSelection}
            onToggleInspector={() => ui.setInspectorOpen(!ui.inspectorOpen)}
          />

          <div className="content-row">
            <main className="library-main">
              {boot?.demoMode && showDemoBanner ? <div className="demo-banner"><ScanSearch size={15} /><span>浏览器预览使用演示图库；桌面版会读取你的本地文件。</span><button type="button" onClick={() => setShowDemoBanner(false)} aria-label="关闭提示"><X size={14} /></button></div> : null}
              {ui.activeView === 'inbox' && !ui.search ? (
                <section className="inbox-summary">
                  <div className="inbox-lead"><span className="inbox-icon"><Inbox size={20} /></span><div><strong>{stats.inbox} 张图片等待整理</strong><p>建议来自文件来源、拍摄时间和图片特征。</p></div></div>
                  <div className="suggestion-stats">
                    <span><strong>{inboxCounts.screenshots}</strong>截图</span>
                    <span><strong>{inboxCounts.wechat}</strong>微信图片</span>
                    <span><strong>{inboxCounts.camera}</strong>相机照片</span>
                    <span><strong>{inboxCounts.duplicates}</strong>重复项</span>
                  </div>
                  <button className="primary-button compact" type="button" onClick={() => openOrganize(assets.map((asset) => asset.id))} disabled={!assets.length}><Sparkles size={16} />预览全部建议</button>
                </section>
              ) : null}

              {ui.activeView === 'albums' ? <AlbumOverview albums={albums} onCreate={() => setAlbumDialogOpen(true)} onOpen={openAlbum} /> : ui.activeView === 'history' ? <ActivityView items={activity} /> : (
                <PhotoGrid
                  assets={assets}
                  selectedIds={ui.selectedIds}
                  activeAssetId={ui.selectedAssetId}
                  emptyTitle={copy.emptyTitle}
                  emptyBody={copy.emptyBody}
                  onSelect={(asset, additive) => ui.toggleSelected(asset.id, additive)}
                  onOpen={(asset) => { ui.selectAsset(asset.id); ui.setViewerOpen(true) }}
                  hasMore={nextCursor !== null}
                  loadingMore={loadingMore}
                  onLoadMore={loadMore}
                />
              )}
            </main>
            {ui.inspectorOpen && ui.activeView !== 'albums' && ui.activeView !== 'history' ? (
              <Inspector
                asset={selectedAsset}
                analyzing={analyzingId === selectedAsset?.id}
                ocring={ocringId === selectedAsset?.id}
                onFavorite={toggleFavorite}
                onAnalyze={analyze}
                onOcr={extractText}
                onReveal={(asset) => bridge.revealPath(asset.path)}
                albums={albums}
                locations={assetLocations}
                locationsLoading={locationsLoading}
                onAddToAlbum={(asset) => openAlbumPicker([asset.id])}
                onEditTags={(asset) => openTagEditor([asset.id])}
                onTrashDuplicate={trashDuplicate}
              />
            ) : null}
          </div>
        </div>

        {busyLabel ? <div className="busy-status"><LoaderCircle className="spin" size={16} /><span>{busyLabel}</span>{scanning ? <button type="button" onClick={cancelScan}>停止扫描</button> : null}</div> : null}
        {toast ? <div className="toast" data-tone={toast.tone}>{toast.tone === 'success' ? <CheckCircle2 size={17} /> : toast.tone === 'error' ? <AlertCircle size={17} /> : <ScanSearch size={17} />}<span>{toast.message}</span><button type="button" aria-label="关闭通知" onClick={() => setToast(null)}><X size={14} /></button></div> : null}

        <OrganizeDialog open={ui.organizeOpen} plan={organizePlan} applying={applying} onOpenChange={ui.setOrganizeOpen} onApply={applyPlan} />
        <SettingsDialog open={ui.settingsOpen} settings={settings} saving={savingSettings} testingConnection={testingConnection} connectionResult={connectionResult} exportingDiagnostics={exportingDiagnostics} onOpenChange={ui.setSettingsOpen} onSave={saveSettings} onTestConnection={testAiConnection} onDeleteApiKey={deleteApiKey} onClearAiResults={clearAiResults} onExportDiagnostics={exportDiagnostics} />
        <ViewerDialog open={ui.viewerOpen} asset={selectedAsset} hasPrevious={viewerIndex > 0} hasNext={viewerIndex >= 0 && viewerIndex < assets.length - 1} onOpenChange={ui.setViewerOpen} onPrevious={() => navigateViewer(-1)} onNext={() => navigateViewer(1)} />
        <CreateAlbumDialog open={albumDialogOpen} saving={savingAlbum} onOpenChange={setAlbumDialogOpen} onCreate={createAlbum} />
        <AlbumPickerDialog open={albumPickerOpen} albums={albums} count={dialogAssetIds.length} saving={savingAlbumAssignment} onOpenChange={setAlbumPickerOpen} onAssign={assignToAlbum} />
        <TagDialog open={tagDialogOpen} count={dialogAssetIds.length} initialTags={tagDialogInitial} saving={savingTags} onOpenChange={setTagDialogOpen} onSave={saveTags} />
        <RecoveryDialog open={recoveryOpen} jobs={recoveryJobs} busyPlanId={recoveryBusyPlanId} onOpenChange={setRecoveryOpen} onResume={resumeRecovery} onRollback={rollbackRecovery} />
      </div>
    </Tooltip.Provider>
  )
}
