import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import App from './App'

afterEach(() => {
  cleanup()
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
})
