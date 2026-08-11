import { create } from 'zustand'
import type { ViewId } from '../lib/types'

interface LibraryUiState {
  activeView: ViewId
  search: string
  selectedAssetId: number | null
  selectedIds: number[]
  inspectorOpen: boolean
  settingsOpen: boolean
  organizeOpen: boolean
  viewerOpen: boolean
  setActiveView: (view: ViewId) => void
  setSearch: (search: string) => void
  selectAsset: (id: number | null) => void
  toggleSelected: (id: number, additive?: boolean) => void
  clearSelection: () => void
  setInspectorOpen: (open: boolean) => void
  setSettingsOpen: (open: boolean) => void
  setOrganizeOpen: (open: boolean) => void
  setViewerOpen: (open: boolean) => void
}

export const useLibraryUi = create<LibraryUiState>((set) => ({
  activeView: 'inbox',
  search: '',
  selectedAssetId: null,
  selectedIds: [],
  inspectorOpen: true,
  settingsOpen: false,
  organizeOpen: false,
  viewerOpen: false,
  setActiveView: (activeView) => set({ activeView, selectedIds: [] }),
  setSearch: (search) => set({ search }),
  selectAsset: (selectedAssetId) => set({ selectedAssetId }),
  toggleSelected: (id, additive = false) =>
    set((state) => {
      if (!additive) return { selectedIds: [id], selectedAssetId: id }
      const selectedIds = state.selectedIds.includes(id)
        ? state.selectedIds.filter((candidate) => candidate !== id)
        : [...state.selectedIds, id]
      return { selectedIds, selectedAssetId: id }
    }),
  clearSelection: () => set({ selectedIds: [] }),
  setInspectorOpen: (inspectorOpen) => set({ inspectorOpen }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  setOrganizeOpen: (organizeOpen) => set({ organizeOpen }),
  setViewerOpen: (viewerOpen) => set({ viewerOpen }),
}))
