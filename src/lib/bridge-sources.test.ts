import { describe, expect, it } from 'vitest'
import { bridge } from './bridge'

describe('browser source removal', () => {
  it('keeps nested source indexes first, then removes the parent with subdirs', async () => {
    const preview = await bridge.previewRemoveSource('C:\\Users\\me\\Desktop')
    expect(preview.current.monitoredCount).toBe(1)
    expect(preview.withSubdirs.monitoredCount).toBe(2)

    const result = await bridge.removeSource('C:\\Users\\me\\Desktop', false)
    expect(result.removedPaths).toEqual(['C:\\Users\\me\\Desktop'])
    expect(result.removedIndexes).toBeGreaterThan(0)

    await bridge.saveSettings({
      configured: true,
      libraryPath: 'C:\\Users\\me\\Pictures\\PicNest Library',
      sourcePaths: ['C:\\Users\\me\\Desktop', 'C:\\Users\\me\\Desktop\\WeChat Images', 'C:\\Users\\me\\Downloads'],
      sourceRecursive: {
        'C:\\Users\\me\\Desktop': true,
        'C:\\Users\\me\\Desktop\\WeChat Images': true,
        'C:\\Users\\me\\Downloads': true,
      },
      locale: 'zh-CN',
      cloudAiEnabled: false,
      aiBaseUrl: 'https://api.openai.com/v1',
      visionModel: 'gpt-4.1-mini',
      embeddingModel: 'text-embedding-3-small',
      aiBatchLimit: 20,
      apiKeyConfigured: false,
      telemetryEnabled: false,
    })

    const secondPreview = await bridge.previewRemoveSource('C:\\Users\\me\\Desktop')
    expect(secondPreview.current.monitoredCount).toBe(1)
    expect(secondPreview.withSubdirs.monitoredCount).toBe(2)
    const secondResult = await bridge.removeSource('C:\\Users\\me\\Desktop', true)
    expect(secondResult.removedPaths).toHaveLength(2)
    expect(secondResult.removedIndexes).toBeGreaterThan(0)
  })
})
