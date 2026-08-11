import '@testing-library/jest-dom/vitest'

class ResizeObserverMock {
  private callback: ResizeObserverCallback

  constructor(callback: ResizeObserverCallback) {
    this.callback = callback
  }

  observe(target: Element) {
    this.callback([
      {
        target,
        contentRect: { width: 960, height: 720, top: 0, left: 0, right: 960, bottom: 720, x: 0, y: 0, toJSON: () => ({}) },
        borderBoxSize: [],
        contentBoxSize: [],
        devicePixelContentBoxSize: [],
      },
    ], this as unknown as ResizeObserver)
  }
  unobserve() {}
  disconnect() {}
}

Object.defineProperty(window, 'ResizeObserver', { value: ResizeObserverMock })
Object.defineProperty(globalThis, 'ResizeObserver', { value: ResizeObserverMock })
Object.defineProperty(HTMLElement.prototype, 'clientWidth', { configurable: true, get: () => 960 })
Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 720 })
HTMLElement.prototype.getBoundingClientRect = () => ({
  width: 960,
  height: 720,
  top: 0,
  left: 0,
  right: 960,
  bottom: 720,
  x: 0,
  y: 0,
  toJSON: () => ({}),
})
