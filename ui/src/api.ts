/* The page's line to Carbon's engine: the shapes it sends, and one function per thing the page can ask for. */

export type Role = 'website' | 'admin' | 'pro' | 'other'

export type App = {
  /** Its folder, relative to home: "dev/work/thing/web" */
  id: string
  group: string
  project: string
  /** The folder inside the project, when the app isn't the project root */
  part: string | null
  role: Role
  /** vite, next, astro, node, deno, cargo, go, mise or procfile */
  kind: string
  runtime: string | null
  /** What it is built with, main thing first */
  stack: { id: string; name: string }[]
  /** The project names its own tool versions for mise */
  pinned: boolean
  /** The port it is on now, or would be given */
  port: number
  /** Exactly what Carbon runs for it */
  command: string
  byEnv: boolean
  /** Where its code stands */
  git: Git | null
}

export type Git = { branch: string; last: number; subject: string; changes: number }
/** One or more apps that belong together */
export type Project = { key: string; name: string; title: string; group: string; description: string | null; apps: string[] }

export type Slot = {
  port: number
  role: Role
  label: string
  status: 'idle' | 'starting' | 'running' | 'exited'
  app: string | null
  /** What holds the port when it isn't an app Carbon knows */
  holder: string | null
  pid: number | null
  hasLogs: boolean
  /** When it stopped by itself: the last thing it said */
  note: string | null
  since: number | null
  /** The agent that started what is on this port, if one did */
  by: string | null
}

/** One server an agent started */
export type Started = { pid: number; command: string; cwd: string | null; app: string | null; ports: number[]; since: number }
/** One agent session that has servers running */
export type Agent = {
  /** The harness's key and name, e.g. "claude-code" and "Claude Code" */
  harness: string
  name: string
  /** The app the session is running inside, e.g. an editor or a terminal */
  host: { name: string; app: string | null } | null
  pid: number
  cwd: string | null
  since: number
  /** What the session is called, when the harness says */
  title: string | null
  /** The model it last answered with, when the harness says */
  model: string | null
  /** "busy" while the agent is working, "idle" while it waits */
  status: string | null
  servers: Started[]
}

export type Listener = { port: number; pid: number; command: string; cwd: string | null; app: string | null }
export type Job = { key: string; title: string; status: 'running' | 'done' | 'failed' }
export type Template = { id: string; label: string; detail: string }

export type Snapshot = {
  slots: Slot[]
  apps: App[]
  others: Listener[]
  agents: Agent[]
  projects: Project[]
  groups: string[]
  jobs: Job[]
  templates: Template[]
  mise: boolean
}

async function post<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch(`/api/${path}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  })
  const data = await response.json().catch(() => ({}))
  if (!response.ok) throw new Error(data.error ?? `Carbon answered ${response.status}`)
  return data as T
}

export const api = {
  state: () => fetch('/api/state').then((r) => r.json() as Promise<Snapshot>),
  logs: (key: string) => fetch(`/api/logs/${key}`).then((r) => r.json() as Promise<string[]>),
  start: (id: string) => post<{ port: number }>('start', { id }),
  startProject: (group: string, project: string) => post<{ ports: number[] }>('start-project', { group, project }),
  stop: (port: number) => post('stop', { port }),
  kill: (pid: number) => post('kill', { pid }),
  open: (port: number) => post('open', { port }),
  pin: (id: string) => post<{ job: string }>('pin', { id }),
  create: (group: string, name: string, template: string) => post<{ job: string }>('create', { group, name, template }),
}
