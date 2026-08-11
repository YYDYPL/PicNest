import { describe, expect, it } from 'vitest'
import { bridge } from './bridge'

describe('browser bridge safety flow', () => {
  it('previews, applies, and undoes an organize plan', async () => {
    const before = await bridge.listAssets({ view: 'inbox', limit: 10 })
    const selected = before.items.slice(0, 2)
    const plan = await bridge.createOrganizePlan(selected.map((asset) => asset.id))

    expect(plan.items).toHaveLength(2)
    expect(plan.items[0].targetPath).toContain('PicNest Library')

    const result = await bridge.applyOrganizePlan(plan.id)
    expect(result.moved).toBe(2)

    const restored = await bridge.undoLastOperation()
    expect(restored).toBe(2)
  })
})
