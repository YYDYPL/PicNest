import { ArrowRight, Check, FolderOpen, Images, LockKeyhole, ShieldCheck } from 'lucide-react'
import { useState } from 'react'
import type { AppSettings } from '../lib/types'

interface OnboardingProps {
  defaults: AppSettings
  onPickLibrary: () => Promise<string | null>
  onPickSources: () => Promise<string[]>
  onFinish: (settings: AppSettings) => Promise<void>
}

export function Onboarding({ defaults, onPickLibrary, onPickSources, onFinish }: OnboardingProps) {
  const [libraryPath, setLibraryPath] = useState(defaults.libraryPath)
  const [sourcePaths, setSourcePaths] = useState(defaults.sourcePaths)
  const [saving, setSaving] = useState(false)

  const chooseLibrary = async () => {
    const path = await onPickLibrary()
    if (path) setLibraryPath(path)
  }
  const chooseSources = async () => {
    const paths = await onPickSources()
    if (paths.length) setSourcePaths(Array.from(new Set([...sourcePaths, ...paths])))
  }
  const finish = async () => {
    setSaving(true)
    try { await onFinish({ ...defaults, configured: true, libraryPath, sourcePaths }) } finally { setSaving(false) }
  }

  return (
    <main className="onboarding-shell">
      <section className="onboarding-panel">
        <div className="onboarding-brand"><span className="brand-mark"><span /><span /><span /></span><span>PicNest</span></div>
        <div className="onboarding-copy">
          <span className="eyebrow">本地图片收件箱</span>
          <h1>把散落的图片，收进一个可找回的地方。</h1>
          <p>PicNest 只建立本地索引。任何移动都会先预览，原图不会上传，也不会被锁在应用里。</p>
        </div>

        <div className="setup-steps">
          <div className="setup-row">
            <span className="setup-index">01</span>
            <span className="setup-icon"><Images size={20} /></span>
            <div><strong>图库位置</strong><p>{libraryPath || '尚未选择'}</p></div>
            <button className="secondary-button" type="button" onClick={chooseLibrary}><FolderOpen size={16} />选择</button>
          </div>
          <div className="setup-row">
            <span className="setup-index">02</span>
            <span className="setup-icon"><FolderOpen size={20} /></span>
            <div><strong>图片来源</strong><p>{sourcePaths.length ? `已选择 ${sourcePaths.length} 个文件夹` : '桌面、下载或微信图片目录'}</p></div>
            <button className="secondary-button" type="button" onClick={chooseSources}><FolderOpen size={16} />添加</button>
          </div>
          <div className="setup-row privacy-row">
            <span className="setup-index">03</span>
            <span className="setup-icon"><LockKeyhole size={20} /></span>
            <div><strong>隐私确认</strong><p>云端 AI 默认关闭，扫描和搜索无需网络。</p></div>
            <span className="confirmed"><Check size={15} />本地优先</span>
          </div>
        </div>

        <div className="onboarding-footer">
          <div><ShieldCheck size={18} /><span>数据库损坏时也可以从原图重建索引</span></div>
          <button className="primary-button large" type="button" onClick={finish} disabled={saving || !libraryPath || sourcePaths.length === 0}>开始建立图库<ArrowRight size={17} /></button>
        </div>
      </section>
    </main>
  )
}
