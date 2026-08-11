import { useEffect, useState } from 'react'
import { bridge } from './bridge'
import type { Asset } from './types'

export function useAssetImage(asset: Asset | null, preview = false) {
  const assetId = asset?.id ?? null
  const thumbnailDataUrl = asset?.thumbnailDataUrl ?? null
  const [source, setSource] = useState<string | null>(thumbnailDataUrl)

  useEffect(() => {
    let active = true
    setSource(thumbnailDataUrl)
    if (!assetId || thumbnailDataUrl) return () => { active = false }
    const request = preview ? bridge.getAssetPreview(assetId) : bridge.getAssetThumbnail(assetId)
    void request
      .then((value) => {
        if (active) setSource(value)
      })
      .catch(() => {
        if (active) setSource(null)
      })
    return () => { active = false }
  }, [assetId, preview, thumbnailDataUrl])

  return source
}
