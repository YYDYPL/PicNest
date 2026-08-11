import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { revealItemInDir } from '@tauri-apps/plugin-opener'
import {
  demoActivity,
  demoAlbums,
  demoAssets,
  demoSettings,
  statsFor,
} from './demo'
import type {
  AiAnalysis,
  AiConnectionInput,
  AppSettings,
  AssetLocation,
  Asset,
  AssetPage,
  AssetQuery,
  BootstrapPayload,
  ConnectionTestResult,
  DiagnosticsResult,
  OrganizePlan,
  OrganizeResult,
  RemoveSourcePreview,
  RemoveSourceResult,
  SaveSettingsInput,
  ScanResult,
} from './types'

const isTauri = () => '__TAURI_INTERNALS__' in window

let browserAssets = structuredClone(demoAssets)
let browserSettings = structuredClone(demoSettings)
let browserLastPlan: OrganizePlan | null = null
const thumbnailCache = new Map<number, Promise<string | null>>()
const previewCache = new Map<number, Promise<string | null>>()

function filterBrowserAssets(query: AssetQuery) {
  let items = browserAssets
  if (query.view === 'inbox') items = items.filter((asset) => asset.needsOrganize)
  if (query.view === 'recent') items = items.filter((asset) => Date.parse(asset.capturedAt) >= Date.parse('2026-07-12'))
  if (query.view === 'favorites') items = items.filter((asset) => asset.favorite)
  if (query.view === 'duplicates') items = items.filter((asset) => asset.duplicateCount > 0 || asset.similarCount > 0)
  if (query.view === 'missing') items = items.filter((asset) => asset.missing)
  if (query.view === 'album' && query.albumId) items = items.filter((asset) => asset.albumIds.includes(query.albumId!))
  if (query.category) items = items.filter((asset) => asset.category === query.category)
  if (query.source) items = items.filter((asset) => asset.source.toLocaleLowerCase().includes(query.source!.toLocaleLowerCase()))
  if (query.location) items = items.filter((asset) => asset.location?.toLocaleLowerCase().includes(query.location!.toLocaleLowerCase()))
  if (query.dateFrom) items = items.filter((asset) => asset.capturedAt.slice(0, 10) >= query.dateFrom!)
  if (query.dateTo) items = items.filter((asset) => asset.capturedAt.slice(0, 10) <= query.dateTo!)
  if (query.search?.trim()) {
    const terms = query.search.toLocaleLowerCase().split(/\s+/).filter(Boolean)
    items = items.filter((asset) => {
      const haystack = [
        asset.filename,
        asset.path,
        asset.description,
        asset.ocrText,
        asset.camera,
        asset.location,
        ...asset.tags,
      ]
        .filter(Boolean)
        .join(' ')
        .toLocaleLowerCase()
      return terms.every((term) => haystack.includes(term))
    })
  }
  return [...items].sort((a, b) => Date.parse(b.capturedAt) - Date.parse(a.capturedAt))
}

function normalizedSourcePath(value: string): string {
  return value.replace(/[\\/]+$/, '').toLocaleLowerCase()
}

function browserPathStartsWith(path: string, root: string): boolean {
  const pathValue = normalizedSourcePath(path)
  const rootValue = normalizedSourcePath(root)
  return pathValue === rootValue || pathValue.startsWith(`${rootValue}\\`) || pathValue.startsWith(`${rootValue}/`)
}

function browserPathInScope(path: string, root: string, recursive: boolean): boolean {
  if (!browserPathStartsWith(path, root)) return false
  if (recursive) return true
  const rootValue = normalizedSourcePath(root)
  const rest = normalizedSourcePath(path).slice(rootValue.length).replace(/^[\\/]+/, '')
  return Boolean(rest) && !rest.includes('\\') && !rest.includes('/')
}

function browserRemovedPaths(path: string, includeSubdirs: boolean): string[] {
  return browserSettings.sourcePaths.filter((candidate) => {
    if (normalizedSourcePath(candidate) === normalizedSourcePath(path)) return true
    return includeSubdirs && browserPathStartsWith(candidate, path)
  })
}

function browserIndexCount(removedPaths: string[]): number {
  const remainingPaths = browserSettings.sourcePaths.filter((path) => !removedPaths.includes(path))
  return browserAssets.filter((asset) => {
    const removed = removedPaths.some((path) => browserPathInScope(asset.path, path, browserSettings.sourceRecursive?.[path] ?? true))
    const remaining = remainingPaths.some((path) => browserPathInScope(asset.path, path, browserSettings.sourceRecursive?.[path] ?? true))
    return removed && !remaining
  }).length
}

export const bridge = {
  isDesktop: isTauri,

  async bootstrap(): Promise<BootstrapPayload> {
    if (isTauri()) return invoke('bootstrap')
    return {
      settings: browserSettings,
      stats: statsFor(browserAssets),
      albums: demoAlbums,
      recentActivity: demoActivity,
      demoMode: true,
      recoveryJobs: [],
    }
  },

  async onLibraryChanged(callback: () => void): Promise<() => void> {
    if (!isTauri()) return () => undefined
    return listen('library-changed', callback)
  },

  async listAssets(query: AssetQuery): Promise<AssetPage> {
    if (isTauri()) return invoke('list_assets', { query })
    const filtered = filterBrowserAssets(query)
    const start = query.cursor ?? 0
    const limit = query.limit ?? 200
    return {
      items: filtered.slice(start, start + limit),
      nextCursor: start + limit < filtered.length ? start + limit : null,
      total: filtered.length,
    }
  },

  async pickSourceFolders(): Promise<string[]> {
    if (!isTauri()) return []
    const result = await open({ directory: true, multiple: true, title: '选择图片来源文件夹' })
    if (!result) return []
    return Array.isArray(result) ? result : [result]
  },

  async pickLibraryFolder(): Promise<string | null> {
    if (!isTauri()) return browserSettings.libraryPath
    const result = await open({ directory: true, multiple: false, title: '选择 PicNest 图库位置' })
    return typeof result === 'string' ? result : null
  },

  async scanPaths(paths: string[]): Promise<ScanResult> {
    if (isTauri()) return invoke('scan_paths', { paths })
    return { discovered: browserAssets.length, indexed: browserAssets.length, duplicates: 3, unsupported: 0, failed: 0, skipped: browserAssets.length, cancelled: false }
  },

  async cancelScan(): Promise<boolean> {
    if (isTauri()) return invoke('cancel_scan')
    return false
  },

  async previewRemoveSource(path: string): Promise<RemoveSourcePreview> {
    if (isTauri()) return invoke('preview_remove_source', { path })
    const current = browserRemovedPaths(path, false)
    const withSubdirs = browserRemovedPaths(path, true)
    return {
      path,
      current: { monitoredCount: current.length, indexCount: browserIndexCount(current) },
      withSubdirs: { monitoredCount: withSubdirs.length, indexCount: browserIndexCount(withSubdirs) },
    }
  },

  async removeSource(path: string, includeSubdirs: boolean): Promise<RemoveSourceResult> {
    if (isTauri()) return invoke('remove_source', { path, includeSubdirs })
    const removedPaths = browserRemovedPaths(path, includeSubdirs)
    const removedIndexes = browserIndexCount(removedPaths)
    const remainingPaths = browserSettings.sourcePaths.filter((candidate) => !removedPaths.includes(candidate))
    const recursive = { ...(browserSettings.sourceRecursive ?? {}) }
    browserSettings = {
      ...browserSettings,
      sourcePaths: remainingPaths,
      sourceRecursive: Object.fromEntries(
        Object.entries(browserSettings.sourceRecursive ?? {}).filter(([candidate]) => !removedPaths.includes(candidate)),
      ),
    }
    browserAssets = browserAssets.filter((asset) => {
      const removed = removedPaths.some((path) => browserPathInScope(asset.path, path, recursive[path] ?? true))
      const remaining = remainingPaths.some((path) => browserPathInScope(asset.path, path, browserSettings.sourceRecursive?.[path] ?? true))
      return !(removed && !remaining)
    })
    return { removedPaths, removedIndexes }
  },

  async getAssetThumbnail(assetId: number): Promise<string | null> {
    if (!isTauri()) return browserAssets.find((asset) => asset.id === assetId)?.thumbnailDataUrl ?? null
    if (!thumbnailCache.has(assetId)) thumbnailCache.set(assetId, invoke('get_asset_thumbnail', { assetId }))
    return thumbnailCache.get(assetId)!
  },

  async getAssetPreview(assetId: number): Promise<string | null> {
    if (!isTauri()) return browserAssets.find((asset) => asset.id === assetId)?.thumbnailDataUrl ?? null
    if (!previewCache.has(assetId)) previewCache.set(assetId, invoke('get_asset_preview', { assetId }))
    return previewCache.get(assetId)!
  },

  async createOrganizePlan(assetIds: number[]): Promise<OrganizePlan> {
    if (isTauri()) return invoke('create_organize_plan', { assetIds })
    const items = browserAssets
      .filter((asset) => assetIds.includes(asset.id))
      .map((asset) => {
        const date = new Date(asset.capturedAt)
        const month = String(date.getMonth() + 1).padStart(2, '0')
        return {
          assetId: asset.id,
          filename: asset.filename,
          sourcePath: asset.path,
          targetPath: `${browserSettings.libraryPath}\\${date.getFullYear()}\\${month}\\${asset.filename}`,
          reason: asset.category === 'screenshot' ? '检测到截图特征' : `按拍摄时间 ${date.getFullYear()}-${month} 归档`,
          conflict: false,
          bytes: asset.fileSize,
        }
      })
    browserLastPlan = {
      id: crypto.randomUUID(),
      items,
      totalBytes: items.reduce((sum, item) => sum + item.bytes, 0),
      conflicts: 0,
      requiredCopyBytes: 0,
      availableBytes: 128 * 1024 * 1024 * 1024,
      diskSpaceOk: true,
    }
    return browserLastPlan
  },

  async applyOrganizePlan(planId: string): Promise<OrganizeResult> {
    if (isTauri()) return invoke('apply_organize_plan', { planId })
    if (!browserLastPlan || browserLastPlan.id !== planId) throw new Error('整理预案已失效')
    for (const item of browserLastPlan.items) {
      const asset = browserAssets.find((candidate) => candidate.id === item.assetId)
      if (asset) {
        asset.path = item.targetPath
        asset.needsOrganize = false
      }
    }
    return { jobId: planId, moved: browserLastPlan.items.length, failed: 0 }
  },

  async undoLastOperation(): Promise<number> {
    if (isTauri()) return invoke('undo_last_operation')
    if (!browserLastPlan) return 0
    for (const item of browserLastPlan.items) {
      const asset = browserAssets.find((candidate) => candidate.id === item.assetId)
      if (asset) {
        asset.path = item.sourcePath
        asset.needsOrganize = true
      }
    }
    const count = browserLastPlan.items.length
    browserLastPlan = null
    return count
  },

  async rollbackOrganizePlan(planId: string): Promise<number> {
    if (isTauri()) return invoke('rollback_organize_plan', { planId })
    return this.undoLastOperation()
  },

  async toggleFavorite(assetId: number, favorite: boolean): Promise<void> {
    if (isTauri()) return invoke('set_favorite', { assetId, favorite })
    const asset = browserAssets.find((candidate) => candidate.id === assetId)
    if (asset) asset.favorite = favorite
  },

  async analyzeAsset(assetId: number): Promise<AiAnalysis> {
    if (isTauri()) return invoke('analyze_asset', { assetId })
    const asset = browserAssets.find((candidate) => candidate.id === assetId)
    if (!asset) throw new Error('图片不存在')
    await new Promise((resolve) => window.setTimeout(resolve, 650))
    asset.aiAnalyzed = true
    asset.description ||= `${asset.tags.join('、')}场景的图片。`
    return {
      description: asset.description,
      tags: asset.tags,
      imageType: asset.category === 'screenshot' ? 'screenshot' : 'photo',
      scene: asset.tags[0] ?? '生活记录',
      objects: asset.tags.slice(0, 3),
      confidence: 0.91,
      model: browserSettings.visionModel,
    }
  },

  async testAiConnection(input: AiConnectionInput): Promise<ConnectionTestResult> {
    if (isTauri()) return invoke('test_ai_connection', { input })
    await new Promise((resolve) => window.setTimeout(resolve, 450))
    return { ok: Boolean(input.baseUrl && input.model), latencyMs: 128, message: '演示模式连接正常' }
  },

  async clearAiResults(assetIds: number[] = []): Promise<number> {
    if (isTauri()) return invoke('clear_ai_results', { assetIds })
    const targets = assetIds.length ? browserAssets.filter((asset) => assetIds.includes(asset.id)) : browserAssets
    for (const asset of targets) {
      asset.aiAnalyzed = false
      asset.description = null
    }
    return targets.length
  },

  async deleteApiKey(): Promise<AppSettings> {
    if (isTauri()) return invoke('delete_api_key')
    browserSettings = { ...browserSettings, apiKeyConfigured: false }
    return browserSettings
  },

  async ocrAsset(assetId: number): Promise<string> {
    if (isTauri()) return invoke('ocr_asset', { assetId })
    const asset = browserAssets.find((candidate) => candidate.id === assetId)
    if (!asset) throw new Error('图片不存在')
    await new Promise((resolve) => window.setTimeout(resolve, 500))
    asset.ocrText ||= asset.category === 'screenshot' ? 'docker compose failed connection refused redis' : '演示图片中的本地 OCR 文字'
    return asset.ocrText
  },

  async createAlbum(name: string): Promise<number> {
    if (isTauri()) return invoke('create_album', { name })
    const id = Math.max(0, ...demoAlbums.map((album) => album.id)) + 1
    demoAlbums.unshift({ id, name, kind: 'manual', count: 0 })
    return id
  },

  async assignAssetsToAlbum(albumId: number, assetIds: number[]): Promise<number> {
    if (isTauri()) return invoke('assign_assets_to_album', { albumId, assetIds })
    let added = 0
    for (const asset of browserAssets) {
      if (assetIds.includes(asset.id) && !asset.albumIds.includes(albumId)) {
        asset.albumIds.push(albumId)
        added += 1
      }
    }
    const album = demoAlbums.find((candidate) => candidate.id === albumId)
    if (album) album.count += added
    return added
  },

  async setAssetTags(assetIds: number[], tags: string[]): Promise<number> {
    if (isTauri()) return invoke('set_asset_tags', { assetIds, tags })
    let updated = 0
    for (const asset of browserAssets) {
      if (assetIds.includes(asset.id)) {
        asset.tags = tags
        updated += 1
      }
    }
    return updated
  },

  async listAssetLocations(assetId: number): Promise<AssetLocation[]> {
    if (isTauri()) return invoke('list_asset_locations', { assetId })
    const asset = browserAssets.find((candidate) => candidate.id === assetId)
    if (!asset) return []
    return Array.from({ length: asset.duplicateCount + 1 }, (_, index) => ({
      id: asset.id * 10 + index,
      path: index === 0 ? asset.path : `D:\\Photo Copies\\${index}\\${asset.filename}`,
      source: index === 0 ? asset.source : '重复副本',
      available: true,
      needsOrganize: index === 0 ? asset.needsOrganize : false,
      fileSize: asset.fileSize,
      modifiedAt: Math.floor(Date.parse(asset.importedAt) / 1000),
    }))
  },

  async moveDuplicateToTrash(assetId: number, path: string): Promise<void> {
    if (isTauri()) return invoke('move_duplicate_to_trash', { assetId, path })
    const asset = browserAssets.find((candidate) => candidate.id === assetId)
    if (!asset || path === asset.path || asset.duplicateCount < 1) throw new Error('需要保留至少一个可用副本')
    asset.duplicateCount -= 1
  },

  async saveSettings(settings: SaveSettingsInput): Promise<AppSettings> {
    if (isTauri()) return invoke('save_settings', { input: settings })
    browserSettings = {
      ...settings,
      configured: true,
      apiKeyConfigured: Boolean(settings.apiKey) || settings.apiKeyConfigured,
      sourceRecursive: settings.sourceRecursive ?? {},
    }
    return browserSettings
  },

  async revealPath(path: string): Promise<void> {
    if (isTauri()) await revealItemInDir(path)
  },

  async pickDiagnosticsFolder(): Promise<string | null> {
    if (!isTauri()) return null
    const result = await open({ directory: true, multiple: false, title: '选择诊断包保存位置' })
    return typeof result === 'string' ? result : null
  },

  async exportDiagnostics(directory: string): Promise<DiagnosticsResult> {
    if (isTauri()) return invoke('export_diagnostics', { directory })
    throw new Error('诊断导出仅在桌面版可用')
  },
}

export function mergeAssetUpdate(items: Asset[], id: number, update: Partial<Asset>) {
  return items.map((asset) => (asset.id === id ? { ...asset, ...update } : asset))
}
