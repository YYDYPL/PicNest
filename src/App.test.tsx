import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import App from './App'

afterEach(cleanup)

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

  it('opens settings from the persistent sidebar', async () => {
    render(<App />)
    fireEvent.click(await screen.findByRole('button', { name: '设置' }))
    expect(await screen.findByRole('dialog')).toBeInTheDocument()
    expect(screen.getByText('云端 AI')).toBeInTheDocument()
  })
})
