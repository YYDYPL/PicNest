import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import App from './App'
import { AddSourcesDialog, DeleteAssetsDialog } from './components/Dialogs'
import { bridge } from './lib/bridge'
import { useLibraryUi } from './store/library'

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  useLibraryUi.setState(useLibraryUi.getInitialState())
  document.body.removeAttribute('data-scroll-locked')
  document.body.style.pointerEvents = ''
  document.querySelectorAll('[data-radix-focus-guard]').forEach((element) => element.remove())
  document.querySelectorAll('[aria-hidden="true"]').forEach((element) => element.removeAttribute('aria-hidden'))
})

describe('PicNest workbench', () => {
  it('loads the local-first inbox in browser preview mode', async () => {
    render(<App />)

    await waitFor(() => expect(screen.getByRole('heading', { name: '待整理' })).toBeInTheDocument())
    expect(screen.getByText(/浏览器预览使用演示图库/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '添加文件夹' })).toBeInTheDocument()
    expect(screen.getByLabelText('搜索图片')).toBeInTheDocument()
  })

  it('filters demo assets using content search', async () => {
    render(<App />)
    const input = await screen.findByLabelText('搜索图片')
    fireEvent.change(input, { target: { value: 'Docker' } })
    await waitFor(() => expect(screen.getByLabelText('照片时间轴，共 2 张')).toBeInTheDocument())
  })

  it('manages recursive scope and removes monitored sources from settings', async () => {
    render(<App />)
    fireEvent.click(await screen.findByRole('button', { name: '设置' }))
    await screen.findByRole('dialog')
    expect(screen.getByText('云端 AI')).toBeInTheDocument()
    const desktopSwitch = screen.getByLabelText('C:\\Users\\me\\Desktop 包含子目录')

    fireEvent.click(desktopSwitch)
    expect(desktopSwitch).toHaveAttribute('aria-checked', 'false')
    fireEvent.click(desktopSwitch)
    expect(desktopSwitch).toHaveAttribute('aria-checked', 'true')
    fireEvent.click(screen.getByLabelText('移除监控 C:\\Users\\me\\Desktop'))
    expect(await screen.findByText('移除监控文件夹')).toBeInTheDocument()
    expect(screen.getByText(/1 个监控项 · /)).toBeInTheDocument()
    expect(screen.getByText(/2 个监控项 · /)).toBeInTheDocument()
    fireEvent.click(screen.getByText('连同子目录一起移除'))
    fireEvent.click(screen.getByRole('button', { name: '移除 2 个监控项' }))

    await waitFor(() => expect(screen.queryByText('移除监控文件夹')).not.toBeInTheDocument())
    expect(screen.getByRole('dialog')).toBeInTheDocument()
    expect(screen.queryByText('C:\\Users\\me\\Desktop')).not.toBeInTheDocument()
  })

  it('lets each newly added source choose whether subdirectories are included', async () => {
    const onConfirm = vi.fn()
    render(
      <AddSourcesDialog
        open
        paths={['C:\\Users\\me\\Desktop', 'C:\\Users\\me\\Downloads']}
        adding={false}
        onOpenChange={() => undefined}
        onConfirm={onConfirm}
      />,
    )

    const desktopSwitch = screen.getByLabelText('C:\\Users\\me\\Desktop 包含子目录')
    const downloadsSwitch = screen.getByLabelText('C:\\Users\\me\\Downloads 包含子目录')
    expect(desktopSwitch).toHaveAttribute('aria-checked', 'true')
    expect(downloadsSwitch).toHaveAttribute('aria-checked', 'true')

    fireEvent.click(desktopSwitch)
    expect(desktopSwitch).toHaveAttribute('aria-checked', 'false')
    fireEvent.click(screen.getByRole('button', { name: '添加并扫描 2 个文件夹' }))

    expect(onConfirm).toHaveBeenCalledWith({
      'C:\\Users\\me\\Desktop': false,
      'C:\\Users\\me\\Downloads': true,
    })
  })

  it('rescans an existing monitored source and preserves settings edits', async () => {
    const scan = vi.spyOn(bridge, 'scanPaths').mockResolvedValue({
      discovered: 3, indexed: 3, skipped: 2, duplicates: 0, unsupported: 0, failed: 0, cancelled: false,
    })
    const path = 'C:\\Users\\me\\Downloads'
    render(<App />)
    fireEvent.click(await screen.findByRole('button', { name: '设置' }))
    const libraryPath = screen.getByLabelText('图库位置')
    fireEvent.change(libraryPath, { target: { value: 'D:\\Photos' } })
    fireEvent.click(screen.getByRole('button', { name: `重新扫描 ${path}` }))

    expect(scan).toHaveBeenCalledWith([path])
    expect(await screen.findByText(/已索引 1 张新增或更新的图片/)).toBeInTheDocument()
    expect(libraryPath).toHaveValue('D:\\Photos')
  })

  it('can copy the successfully organized files when only part of a plan succeeds', async () => {
    const boot = await bridge.bootstrap()
    vi.spyOn(bridge, 'bootstrap').mockResolvedValue({ ...boot, demoMode: false })
    vi.spyOn(bridge, 'applyOrganizePlan').mockResolvedValue({ jobId: 'partially-completed', moved: 1, failed: 1 })
    const copy = vi.spyOn(bridge, 'copyOrganizedFiles').mockResolvedValue(1)
    render(<App />)

    const preview = await screen.findByRole('button', { name: '预览全部建议' })
    await waitFor(() => expect(preview).toBeEnabled())
    fireEvent.click(preview)
    fireEvent.click(await screen.findByRole('button', { name: /^整理 \d+ 张图片$/ }))
    expect(await screen.findByText('已整理 1 张图片，1 张未移动')).toBeInTheDocument()
    const copyButton = screen.getByRole('button', { name: '复制图片文件' })
    await waitFor(() => expect(copyButton).toBeEnabled())
    fireEvent.click(copyButton)

    expect(copy).toHaveBeenCalledWith('partially-completed')
    expect(await screen.findByRole('status')).toHaveTextContent('已复制 1 个图片文件')
  })

  it('confirms moving all indexed copies to the recycle bin', () => {
    const onDelete = vi.fn()
    render(
      <DeleteAssetsDialog
        open
        count={3}
        deleting={false}
        onOpenChange={() => undefined}
        onDelete={onDelete}
      />,
    )

    expect(screen.getByText(/所有已索引本地副本/)).toBeInTheDocument()
    expect(screen.getByText(/将处理/)).toHaveTextContent('3')
    fireEvent.click(screen.getByRole('button', { name: '移入回收站' }))
    expect(onDelete).toHaveBeenCalledOnce()
  })
})
