import { describe, expect, it } from 'vitest'
import { bridge } from './bridge'

describe('browser asset deletion', () => {
  it('removes assets and refreshes statistics and activity', async () => {
    const before = await bridge.listAssets({ view: 'all', limit: 100 })
    const target = before.items.find((asset) => !asset.missing)
    expect(target).toBeDefined()

    const result = await bridge.deleteAssets([target!.id])
    expect(result.deletedAssets).toBe(1)
    expect(result.movedFiles).toBe(target!.duplicateCount + 1)

    const after = await bridge.listAssets({ view: 'all', limit: 100 })
    expect(after.items.some((asset) => asset.id === target!.id)).toBe(false)
    const bootstrap = await bridge.bootstrap()
    expect(bootstrap.stats.total).toBe(before.total - 1)
    expect(bootstrap.recentActivity[0]?.kind).toBe('delete')
  })
})
