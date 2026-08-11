import type {
  ActivityItem,
  Album,
  AppSettings,
  Asset,
  LibraryStats,
} from './types'

const palettes: Array<[string, string, string]> = [
  ['#173d48', '#dca15d', '#78a0a6'],
  ['#809db5', '#e7c59f', '#335c77'],
  ['#345348', '#d9b88f', '#9bb8a8'],
  ['#252b33', '#6aa1a4', '#c77958'],
  ['#6c4838', '#d8a56c', '#425c59'],
  ['#4f6653', '#adc39c', '#d7c6a1'],
  ['#b9c5c9', '#597281', '#db8b64'],
  ['#6a5143', '#d6b78e', '#829786'],
  ['#3e4951', '#c7a66d', '#8297a0'],
  ['#765b51', '#d7b9a2', '#657c70'],
  ['#38404b', '#b9a17f', '#788b99'],
  ['#60766d', '#c99a6b', '#dde0d4'],
]

const names = [
  'IMG_8302.jpg',
  '江边日落.jpg',
  '微信图片_20260811.jpg',
  'Screenshot_2026-08-11.png',
  '晚餐合照.jpg',
  'IMG_8241.jpg',
  '建筑参考_03.jpg',
  '项目笔记.png',
  'IMG_8012.jpg',
  '花市.jpg',
  '展览现场.jpg',
  'IMG_7830.jpg',
]

const tags = [
  ['街道', '夜景', '城市'],
  ['海边', '日落', '旅行'],
  ['猫', '室内', '窗边'],
  ['代码', '报错', 'Docker'],
  ['人物', '聚餐', '餐厅'],
  ['山野', '徒步', '自然'],
  ['建筑', '几何', '城市'],
  ['咖啡', '笔记', '桌面'],
  ['车站', '旅行', '站台'],
  ['花卉', '市集', '色彩'],
  ['展览', '室内', '艺术'],
  ['街道', '生活', '住宅'],
]

const categories: Asset['category'][] = [
  'camera',
  'camera',
  'wechat',
  'screenshot',
  'wechat',
  'camera',
  'download',
  'document',
  'camera',
  'camera',
  'camera',
  'camera',
]

export const demoAssets: Asset[] = Array.from({ length: 36 }, (_, index) => {
  const sample = index % 12
  const dayOffset = index < 14 ? index : index + 18
  const capturedAt = new Date(Date.UTC(2026, 7, 11 - dayOffset, 8 + (index % 9), 12))
  const category = categories[sample]
  const filename = index < 12 ? names[sample] : names[sample].replace('.', `_${index}.`)

  return {
    id: index + 1,
    filename,
    path: `${category === 'camera' ? 'D:\\DCIM' : 'C:\\Users\\me\\Desktop'}\\${filename}`,
    thumbnailDataUrl: null,
    width: category === 'screenshot' || category === 'document' ? 2560 : 4032,
    height: category === 'screenshot' || category === 'document' ? 1440 : 3024,
    capturedAt: capturedAt.toISOString(),
    importedAt: new Date(Date.UTC(2026, 7, 11, 9, index)).toISOString(),
    fileSize: 1_250_000 + index * 137_000,
    source: category === 'camera' ? '相机导入' : category === 'wechat' ? '微信图片' : '桌面',
    category,
    favorite: index === 1 || index === 4 || index === 10,
    missing: index === 31,
    needsOrganize: index < 18,
    duplicateCount: index === 2 || index === 14 || index === 27 ? 2 : 0,
    similarCount: index === 4 || index === 16 ? 1 : 0,
    contentHash: `demo-${String(index + 1).padStart(4, '0')}`,
    camera: category === 'camera' ? index % 2 === 0 ? 'FUJIFILM X-S20' : 'iPhone 15 Pro' : null,
    location: sample === 1 ? '青岛' : sample === 8 ? '上海' : sample === 5 ? '杭州' : null,
    description:
      sample === 3
        ? '深色代码编辑器中显示一段 Docker 容器启动错误。'
        : `${tags[sample].join('、')}场景的生活照片。`,
    ocrText: sample === 3 ? 'docker compose failed connection refused redis' : null,
    tags: tags[sample],
    albumIds: sample === 1 || sample === 5 || sample === 8 ? [1] : sample === 6 || sample === 7 ? [2] : [],
    palette: palettes[sample],
    aiAnalyzed: index % 3 !== 0,
  }
})

export const demoSettings: AppSettings = {
  configured: true,
  libraryPath: 'C:\\Users\\me\\Pictures\\PicNest Library',
  sourcePaths: ['C:\\Users\\me\\Desktop', 'C:\\Users\\me\\Downloads'],
  locale: 'zh-CN',
  cloudAiEnabled: false,
  aiBaseUrl: 'https://api.openai.com/v1',
  visionModel: 'gpt-4.1-mini',
  embeddingModel: 'text-embedding-3-small',
  aiBatchLimit: 20,
  apiKeyConfigured: false,
  telemetryEnabled: false,
}

export const demoAlbums: Album[] = [
  { id: 1, name: '夏日旅行', kind: 'manual', count: 9 },
  { id: 2, name: '设计参考', kind: 'manual', count: 7 },
  { id: 3, name: '包含文字', kind: 'smart', count: 11 },
  { id: 4, name: '本月收藏', kind: 'smart', count: 3 },
]

export const demoActivity: ActivityItem[] = [
  {
    id: 1,
    kind: 'scan',
    title: '扫描了桌面和下载目录',
    detail: '发现 36 张图片，其中 18 张等待整理',
    createdAt: '2026-08-11T10:02:00.000Z',
    reversible: false,
  },
  {
    id: 2,
    kind: 'organize',
    title: '整理了 12 张相机照片',
    detail: '已归档到 2026 / 08',
    createdAt: '2026-08-10T16:28:00.000Z',
    reversible: true,
  },
  {
    id: 3,
    kind: 'ai',
    title: '分析了 8 张图片',
    detail: '已生成描述与标签，未保留上传副本',
    createdAt: '2026-08-09T09:14:00.000Z',
    reversible: false,
  },
]

export function statsFor(assets: Asset[]): LibraryStats {
  return {
    total: assets.length,
    inbox: assets.filter((asset) => asset.needsOrganize).length,
    favorites: assets.filter((asset) => asset.favorite).length,
    duplicates: assets.filter((asset) => asset.duplicateCount > 0 || asset.similarCount > 0).length,
    missing: assets.filter((asset) => asset.missing).length,
    albums: demoAlbums.length,
    storageBytes: assets.reduce((sum, asset) => sum + asset.fileSize, 0),
  }
}
