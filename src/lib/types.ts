export type ViewId =
  | 'inbox'
  | 'all'
  | 'recent'
  | 'albums'
  | 'favorites'
  | 'duplicates'
  | 'missing'
  | 'history'
  | 'album'

export type AssetCategory =
  | 'camera'
  | 'screenshot'
  | 'wechat'
  | 'download'
  | 'document'
  | 'other'

export interface Asset {
  id: number
  filename: string
  path: string
  thumbnailDataUrl?: string | null
  width: number
  height: number
  capturedAt: string
  importedAt: string
  fileSize: number
  source: string
  category: AssetCategory
  favorite: boolean
  missing: boolean
  needsOrganize: boolean
  duplicateCount: number
  similarCount: number
  contentHash: string
  camera?: string | null
  location?: string | null
  description?: string | null
  ocrText?: string | null
  tags: string[]
  albumIds: number[]
  palette?: [string, string, string]
  aiAnalyzed: boolean
}

export interface Album {
  id: number
  name: string
  kind: 'manual' | 'smart'
  count: number
  coverThumbnail?: string | null
}

export interface ActivityItem {
  id: number
  kind: 'scan' | 'organize' | 'undo' | 'ai' | 'source'
  title: string
  detail: string
  createdAt: string
  reversible: boolean
}

export interface AppSettings {
  configured: boolean
  libraryPath: string
  sourcePaths: string[]
  sourceRecursive?: Record<string, boolean>
  locale: 'zh-CN' | 'en-US'
  cloudAiEnabled: boolean
  aiBaseUrl: string
  visionModel: string
  embeddingModel: string
  aiBatchLimit: number
  apiKeyConfigured: boolean
  telemetryEnabled: boolean
}

export interface LibraryStats {
  total: number
  inbox: number
  favorites: number
  duplicates: number
  missing: number
  albums: number
  storageBytes: number
}

export interface BootstrapPayload {
  settings: AppSettings
  stats: LibraryStats
  albums: Album[]
  recentActivity: ActivityItem[]
  demoMode: boolean
  recoveryJobs: RecoveryJob[]
}

export interface AssetQuery {
  view: ViewId
  search?: string
  limit?: number
  cursor?: number | null
  albumId?: number | null
  dateFrom?: string | null
  dateTo?: string | null
  category?: AssetCategory | null
  source?: string | null
  location?: string | null
}

export interface AssetPage {
  items: Asset[]
  nextCursor: number | null
  total: number
}

export interface ScanResult {
  discovered: number
  indexed: number
  duplicates: number
  unsupported: number
  failed: number
  skipped: number
  cancelled: boolean
}

export interface OrganizePlanItem {
  assetId: number
  filename: string
  sourcePath: string
  targetPath: string
  reason: string
  conflict: boolean
  bytes: number
}

export interface OrganizePlan {
  id: string
  items: OrganizePlanItem[]
  totalBytes: number
  conflicts: number
  requiredCopyBytes: number
  availableBytes: number
  diskSpaceOk: boolean
}

export interface OrganizeResult {
  jobId: string
  moved: number
  failed: number
}

export interface AiAnalysis {
  description: string
  tags: string[]
  imageType: string
  scene: string
  objects: string[]
  confidence: number
  model: string
}

export interface SaveSettingsInput extends AppSettings {
  apiKey?: string
}

export interface RecoveryJob {
  planId: string
  moved: number
  remaining: number
  failed: number
  createdAt: string
}

export interface AssetLocation {
  id: number
  path: string
  source: string
  available: boolean
  needsOrganize: boolean
  fileSize: number
  modifiedAt: number
}

export interface AiConnectionInput {
  baseUrl: string
  model: string
  apiKey?: string
}

export interface ConnectionTestResult {
  ok: boolean
  latencyMs: number
  message: string
}

export interface DiagnosticsResult {
  path: string
  bytes: number
}

export interface RemoveSourcePreviewEntry {
  monitoredCount: number
  indexCount: number
}

export interface RemoveSourcePreview {
  path: string
  current: RemoveSourcePreviewEntry
  withSubdirs: RemoveSourcePreviewEntry
}

export interface RemoveSourceResult {
  removedPaths: string[]
  removedIndexes: number
}
