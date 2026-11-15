import { create } from 'zustand'
import { api, type Snapshot } from './api'

type Store = {
  snap: Snapshot | null
  /** Carbon has stopped answering */
  offline: boolean
  /** What is mid-flight, e.g. "start:work/shop/web", "stop:4242", "project:work/shop" */
  busy: string | null
  error: string | null
  /** Whose output the console shows: a port ("4242") or a job key ("create:lab/thing"), or nothing */
  console: string | null
  palette: boolean
  setConsole: (key: string | null) => void
  setPalette: (open: boolean) => void
  refresh: () => Promise<void>
  start: (id: string) => Promise<void>
  startProject: (group: string, project: string) => Promise<void>
  stop: (port: number) => Promise<void>
  kill: (pid: number) => Promise<void>
  pin: (id: string) => Promise<void>
  create: (group: string, name: string, template: string) => Promise<boolean>
}

export const useCarbon = create<Store>((set, get) => {
  /** Runs one action, shows it as busy, and reads the new state straight after */
  const act = async (busy: string, run: () => Promise<unknown>) => {
    set({ busy, error: null })
    let ok = true
    try {
      await run()
    } catch (e) {
      ok = false
      set({ error: e instanceof Error ? e.message : String(e) })
    }
    await get().refresh()
    set({ busy: null })
    return ok
  }
  return {
    snap: null,
    offline: false,
    busy: null,
    error: null,
    console: null,
    palette: false,
    setConsole: (key) => set({ console: key }),
    setPalette: (palette) => set({ palette }),
    refresh: async () => {
      try {
        set({ snap: await api.state(), offline: false })
      } catch {
        set({ offline: true })
      }
    },
    start: async (id) => {
      await act(`start:${id}`, async () => set({ console: String((await api.start(id)).port) }))
    },
    startProject: async (group, project) => {
      await act(`project:${group}/${project}`, async () => set({ console: String((await api.startProject(group, project)).ports[0]) }))
    },
    stop: async (port) => {
      await act(`stop:${port}`, () => api.stop(port))
    },
    kill: async (pid) => {
      await act(`kill:${pid}`, () => api.kill(pid))
    },
    pin: async (id) => {
      await act(`pin:${id}`, async () => set({ console: (await api.pin(id)).job }))
    },
    create: (group, name, template) => act('create', async () => set({ console: (await api.create(group, name, template)).job })),
  }
})
