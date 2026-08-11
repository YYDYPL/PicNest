import { describe, expect, it } from 'vitest'
import { bridge } from './bridge'

describe('browser source removal scope', () => {
  it('cleans nested indexes when removing a non-recursive source with subdirs', async () => {
    await bridge.saveSettings({
      configured: true,
      libraryPath: 'C:\\Users\\me\\Pictures\\PicNest Library',
      sourcePaths: ['C:\\Users\\me\\Desktop'],
      sourceRecursive: {
        'C:\\Users\\me\\Desktop': false,
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

    const preview = await bridge.previewRemoveSource('C:\\Users\\me\\Desktop')
    expect(preview.withSubdirs.indexCount).toBeGreaterThan(preview.current.indexCount)

    const result = await bridge.removeSource('C:\\Users\\me\\Desktop', true)
    expect(result.removedIndexes).toBe(preview.withSubdirs.indexCount)
  })
})
