import { Link, Outlet, useNavigate } from '@tanstack/react-router'
import { useEffect, useMemo, useRef, useState } from 'react'
import { api, type Agent, type App, type Listener, type Project, type Slot } from './api'
import { AgentMark } from './logos'
import { useCarbon } from './store'

const appName = (a: Pick<App, 'project' | 'part'>) => (a.part ? `${a.project} / ${a.part}` : a.project)
const url = (port: number) => `http://localhost:${port}`

/* Inside the desktop app a link can't leave the window, so "Open" asks Carbon to open the real browser instead.
   In a browser the link is simply followed. */
const inApp = navigator.userAgent.includes('Carbon-Desktop')
const follow = (port: number) => (e: React.MouseEvent) => {
  if (!inApp) return
  e.preventDefault()
  void api.open(port)
}

/** How long something has been up, in the largest unit that fits */
function since(ms: number | null) {
  if (!ms) return ''
  const s = Math.max(0, Math.floor((Date.now() - ms) / 1000))
  return s < 60 ? `${s}s` : s < 3600 ? `${Math.floor(s / 60)}m` : `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`
}

/* Carbon's mark: an open ring, and the thing it is waiting for sitting in the gap */
function Mark() {
  return (
    <svg viewBox="60 60 392 392" fill="currentColor" aria-hidden className="size-[18px] shrink-0">
      <circle
        cx="256"
        cy="256"
        r="150"
        fill="none"
        stroke="currentColor"
        strokeWidth="68"
        strokeDasharray="759.2 183.3"
        transform="rotate(35 256 256)"
      />
      <circle cx="408" cy="256" r="38" />
    </svg>
  )
}

const WORD = { idle: 'Free', starting: 'Starting', running: 'Live', exited: 'Stopped' } as const

/* ---------- Previews: a picture of what a port is serving ---------- */

/** The hub takes a new picture every twenty seconds or so; asking with a new number every ten picks it up */
function usePreview(port: number, live: boolean) {
  const [tick, setTick] = useState(() => Math.floor(Date.now() / 10_000))
  const [seen, setSeen] = useState(false)
  useEffect(() => {
    if (!live) return
    // Ask often until the first picture arrives, then only as often as it changes
    const timer = setInterval(() => setTick(Math.floor(Date.now() / (seen ? 10_000 : 2_500))), seen ? 10_000 : 2_500)
    return () => clearInterval(timer)
  }, [live, seen])
  return { src: live ? `/api/preview/${port}?t=${tick}` : null, seen: live && seen, setSeen }
}

function Preview({ port, live, className = '' }: { port: number; live: boolean; className?: string }) {
  const { src, seen, setSeen } = usePreview(port, live)
  return (
    <div className={`relative overflow-hidden rounded-md border border-line bg-panel ${className}`}>
      {/* The picture only shows once one has arrived: until then the pane is simply empty */}
      {src && (
        <img
          src={src}
          alt=""
          onLoad={(e) => setSeen(e.currentTarget.naturalWidth > 0)}
          onError={() => setSeen(false)}
          className={`absolute inset-0 size-full object-cover object-top transition-opacity duration-500 ${seen ? 'opacity-100' : 'opacity-0'}`}
        />
      )}
    </div>
  )
}

/* ---------- The rack: one tile per port ---------- */

function Port({ slot, app, pictures }: { slot: Slot; app?: App; pictures: boolean }) {
  const busy = useCarbon((s) => s.busy)
  const shown = useCarbon((s) => s.console)
  const stop = useCarbon((s) => s.stop)
  const setConsole = useCarbon((s) => s.setConsole)
  const live = slot.status === 'running'
  const key = String(slot.port)
  return (
    <article data-status={slot.status} className="port relative flex min-w-0 flex-col gap-4 bg-bg p-4">
      <div className="flex items-center justify-between gap-2">
        <span className="flex items-center gap-1.5 font-mono text-[11px] text-faint">
          {slot.label}
          {slot.by && <AgentMark kind={slot.by} className="size-3.5" />}
        </span>
        <span className="flex items-center gap-2 font-mono text-[11px] text-muted">
          {live && slot.since ? <span className="text-faint tabular-nums">{since(slot.since)}</span> : null}
          {WORD[slot.status]}
          <span className="led" data-status={slot.status} />
        </span>
      </div>
      {/* Pictures appear for the whole rack once anything is live, so the tiles stay one height */}
      {pictures && <Preview port={slot.port} live={live} className="aspect-[16/10]" />}
      <div className="min-w-0">
        <p className={`font-mono text-[2rem] leading-none font-medium tracking-tighter tabular-nums ${live ? '' : 'text-faint'}`}>
          {slot.port}
        </p>
        {/* Two fixed lines, whatever they say, so every tile is the same height */}
        <p className="mt-2 h-5 truncate text-[13px]" title={slot.holder ?? undefined}>
          {app ? appName(app) : (slot.holder ?? <span className="text-faint">Nothing here</span>)}
        </p>
        <p className="h-4 truncate font-mono text-[11px] text-red-500" title={slot.note ?? undefined}>
          {slot.note}
        </p>
      </div>
      <div className="flex gap-1.5">
        <a
          href={url(slot.port)}
          onClick={follow(slot.port)}
          target="_blank"
          rel="noreferrer"
          aria-disabled={!live}
          className={`btn flex-1 ${live ? 'btn-solid' : 'pointer-events-none opacity-40'}`}
        >
          Open
        </a>
        <button
          type="button"
          className="btn flex-1"
          disabled={!slot.hasLogs}
          aria-pressed={shown === key}
          onClick={() => setConsole(shown === key ? null : key)}
        >
          Output
        </button>
        <button type="button" className="btn flex-1" disabled={slot.status === 'idle' || busy !== null} onClick={() => stop(slot.port)}>
          {busy === `stop:${slot.port}` ? '…' : slot.status === 'exited' ? 'Clear' : 'Stop'}
        </button>
      </div>
    </article>
  )
}

/* ---------- Projects: what each one is, where its code stands, and its apps ---------- */

/** How long ago something happened, in the largest unit that fits */
function ago(ms: number) {
  const s = Math.max(0, Math.floor((Date.now() - ms) / 1000))
  if (s < 3600) return `${Math.max(1, Math.floor(s / 60))}m ago`
  if (s < 86_400) return `${Math.floor(s / 3600)}h ago`
  return `${Math.floor(s / 86_400)}d ago`
}

function AppRow({ app, slot }: { app: App; slot?: Slot }) {
  const busy = useCarbon((s) => s.busy)
  const start = useCarbon((s) => s.start)
  const stop = useCarbon((s) => s.stop)
  const pin = useCarbon((s) => s.pin)
  const mise = useCarbon((s) => s.snap?.mise)
  const here = slot && slot.status !== 'idle'
  return (
    <li className="grid grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-x-3 py-2">
      <div className="min-w-0">
        <p className="flex items-center gap-2 text-[13px]">
          <span className="led shrink-0" data-status={slot?.status ?? 'idle'} />
          <span className="truncate font-medium">{app.part ?? app.project}</span>
          <span className="tag shrink-0">{app.kind}</span>
          {/* Tool versions: named by the project, or one click to name them */}
          {app.pinned ? (
            <span className="tag shrink-0" title="This project names its tool versions for mise">
              pinned
            </span>
          ) : app.runtime && mise ? (
            <button
              type="button"
              className="tag shrink-0 hover:border-line-strong hover:text-fg"
              disabled={busy !== null}
              title="Write this project's tool version into its own mise.toml"
              onClick={() => pin(app.id)}
            >
              pin {app.runtime}
            </button>
          ) : null}
        </p>
        {/* Where the code stands: one line, always there, so rows stay the same height */}
        <p className="h-4 truncate pl-[15px] font-mono text-[11px] text-faint" title={app.git?.subject}>
          {app.git ? (
            <>
              {app.git.branch}, {ago(app.git.last)}
              {app.git.changes > 0 && <span className="text-muted">, {app.git.changes} uncommitted</span>}
              <span className="hidden sm:inline">: {app.git.subject}</span>
            </>
          ) : (
            app.command
          )}
        </p>
      </div>
      <p className="font-mono text-[12px] text-muted tabular-nums">{app.port}</p>
      {here ? (
        <button type="button" className="btn w-14" disabled={busy !== null} onClick={() => stop(slot.port)}>
          Stop
        </button>
      ) : (
        <button type="button" className="btn btn-solid w-14" disabled={busy !== null} onClick={() => start(app.id)}>
          {busy === `start:${app.id}` ? '…' : 'Run'}
        </button>
      )}
    </li>
  )
}

function ProjectCard({ project, apps, slots }: { project: Project; apps: App[]; slots: Slot[] }) {
  const busy = useCarbon((s) => s.busy)
  const startProject = useCarbon((s) => s.startProject)
  const live = apps.filter((a) => slots.some((s) => s.app === a.id && s.status === 'running')).length
  return (
    <section className="flex min-w-0 flex-col bg-bg p-4">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="flex items-center gap-2 text-[15px] font-medium tracking-tight">
            <span className="truncate">{project.name}</span>
            {live > 0 && <span className="font-mono text-[11px] font-normal text-on">{live} live</span>}
          </h3>
          <p className="truncate font-mono text-[11px] text-faint">~/{project.group}</p>
        </div>
        {apps.length > 1 && (
          <button
            type="button"
            className="btn shrink-0"
            disabled={busy !== null}
            onClick={() => startProject(apps[0]!.group, project.name)}
          >
            {busy === `project:${apps[0]!.group}/${project.name}` ? 'Starting…' : `Run all ${apps.length}`}
          </button>
        )}
      </div>
      {/* What it says it is. Two lines kept whether or not it says anything, so cards in a row line up */}
      <p className="mt-2 line-clamp-2 h-10 text-[13px] leading-5 text-muted">{project.description}</p>
      <ul className="mt-1 border-t border-line pt-1">
        {apps.map((a) => (
          <AppRow key={a.id} app={a} slot={slots.find((s) => s.app === a.id)} />
        ))}
      </ul>
    </section>
  )
}

function Elsewhere({ item, app }: { item: Listener; app?: App }) {
  const busy = useCarbon((s) => s.busy)
  const kill = useCarbon((s) => s.kill)
  return (
    <li className="grid grid-cols-[4.5rem_3.5rem_minmax(0,1fr)_auto] items-center gap-4 border-t border-line px-4 py-2.5">
      <Preview port={item.port} live className="aspect-[16/10]" />
      <p className="font-mono text-[13px] tabular-nums">{item.port}</p>
      <div className="min-w-0">
        <p className="truncate text-[13px]">{app ? appName(app) : item.command}</p>
        <p className="truncate font-mono text-[11px] text-faint">
          {item.command}, pid {item.pid}, {item.cwd?.replace(/^\/Users\/[^/]+/, '~')}
        </p>
      </div>
      <div className="flex gap-1.5">
        <a href={url(item.port)} onClick={follow(item.port)} target="_blank" rel="noreferrer" className="btn">
          Open
        </a>
        <button type="button" className="btn" disabled={busy !== null} onClick={() => kill(item.pid)}>
          Stop
        </button>
      </div>
    </li>
  )
}

/* ---------- Agents: the servers coding agents have started, folded into the session that started them ---------- */

function AgentSession({ agent, apps }: { agent: Agent; apps: Map<string, App> }) {
  const busy = useCarbon((s) => s.busy)
  const kill = useCarbon((s) => s.kill)
  return (
    <div className="border-t border-line">
      <div className="flex items-center gap-2.5 px-4 pt-3 pb-1">
        <AgentMark kind={agent.vendor} />
        <h3 className="min-w-0 truncate text-[13px] font-medium">{agent.title ?? agent.name}</h3>
        <span className="tag shrink-0">{agent.name}</span>
        {agent.status && (
          <span className="flex shrink-0 items-center gap-1.5 font-mono text-[11px] text-faint">
            <span className="led" data-status={agent.status === 'busy' ? 'starting' : 'idle'} />
            {agent.status === 'busy' ? 'working' : 'waiting'}
          </span>
        )}
        <p className="hidden min-w-0 truncate font-mono text-[11px] text-faint sm:block">{agent.cwd}</p>
      </div>
      <ul className="pb-1.5">
        {agent.servers.map((s) => {
          const app = s.app ? apps.get(s.app) : undefined
          return (
            <li key={s.pid} className="grid grid-cols-[4.5rem_3.5rem_minmax(0,1fr)_auto] items-center gap-4 px-4 py-2">
              <Preview port={s.ports[0]!} live className="aspect-[16/10]" />
              <p className="font-mono text-[13px] tabular-nums">{s.ports[0]}</p>
              <div className="min-w-0">
                <p className="truncate text-[13px]">
                  {app ? appName(app) : s.cwd} <span className="font-mono text-[11px] text-faint">up {since(s.since)}</span>
                </p>
                <p className="truncate font-mono text-[11px] text-faint" title={s.command}>
                  {s.command}
                </p>
              </div>
              <div className="flex gap-1.5">
                {s.ports.map((port) => (
                  <a
                    key={port}
                    href={url(port)}
                    onClick={follow(port)}
                    target="_blank"
                    rel="noreferrer"
                    className={`btn tabular-nums ${port === s.ports[0] ? 'btn-solid' : ''}`}
                  >
                    {s.ports.length > 1 ? port : 'Open'}
                  </a>
                ))}
                <button type="button" className="btn" disabled={busy !== null} onClick={() => kill(s.pid)}>
                  Stop
                </button>
              </div>
            </li>
          )
        })}
      </ul>
    </div>
  )
}

/* ---------- The console: output of a port or a job, docked at the bottom ---------- */

function Console() {
  const key = useCarbon((s) => s.console)
  const snap = useCarbon((s) => s.snap)
  const setConsole = useCarbon((s) => s.setConsole)
  const [lines, setLines] = useState<string[]>([])
  const box = useRef<HTMLPreElement>(null)

  useEffect(() => {
    if (!key) return
    let live = true
    const read = () =>
      api.logs(key).then(
        (l) => live && setLines(l),
        () => {},
      )
    read()
    const timer = setInterval(read, 900)
    return () => {
      live = false
      clearInterval(timer)
    }
  }, [key])

  // Stay on the newest line
  useEffect(() => {
    box.current?.scrollTo({ top: box.current.scrollHeight })
  }, [lines.length, key])

  if (!snap) return null
  // Everything with output: the ports Carbon started something on, and recent jobs
  const tabs = [
    ...snap.slots.filter((s) => s.hasLogs).map((s) => ({ key: String(s.port), label: String(s.port), status: s.status as string })),
    ...snap.jobs.map((j) => ({
      key: j.key,
      label: j.title,
      status: j.status === 'running' ? 'starting' : j.status === 'done' ? 'running' : 'exited',
    })),
  ]
  if (!key || !tabs.some((t) => t.key === key)) return null
  return (
    <section aria-label="Output" className="flex h-72 shrink-0 flex-col border-t border-line-strong bg-panel">
      <div className="flex items-center gap-1 border-b border-line px-2">
        <div className="flex min-w-0 flex-1 gap-1 overflow-x-auto py-1.5">
          {tabs.map((t) => (
            <button
              key={t.key}
              type="button"
              onClick={() => setConsole(t.key)}
              className={`flex h-7 shrink-0 items-center gap-2 rounded-md px-2.5 font-mono text-[11px] ${t.key === key ? 'bg-bg text-fg' : 'text-faint hover:text-fg'}`}
            >
              <span className="led" data-status={t.status} />
              {t.label}
            </button>
          ))}
        </div>
        <button type="button" className="btn border-0" onClick={() => setConsole(null)}>
          Close
        </button>
      </div>
      <pre
        ref={box}
        className="min-h-0 flex-1 overflow-auto px-4 py-3 font-mono text-[11.5px] leading-relaxed whitespace-pre-wrap text-muted"
      >
        {lines.length ? lines.join('\n') : 'No output yet.'}
      </pre>
    </section>
  )
}

/* ---------- The palette: everything Carbon can do, one keystroke away ---------- */

function Palette() {
  const snap = useCarbon((s) => s.snap)
  const close = useCarbon((s) => s.setPalette)
  const { start, stop } = useCarbon.getState()
  const navigate = useNavigate()
  const [query, setQuery] = useState('')
  const [at, setAt] = useState(0)

  const actions = useMemo(() => {
    if (!snap) return []
    const list: { label: string; hint: string; run: () => void }[] = []
    for (const s of snap.slots.filter((s) => s.status === 'running')) {
      const a = snap.apps.find((a) => a.id === s.app)
      list.push({
        label: `Open ${a ? appName(a) : s.port}`,
        hint: String(s.port),
        run: () => (inApp ? void api.open(s.port) : window.open(url(s.port), '_blank')),
      })
      list.push({ label: `Stop ${a ? appName(a) : s.port}`, hint: String(s.port), run: () => void stop(s.port) })
    }
    for (const a of snap.apps) {
      if (!snap.slots.some((s) => s.app === a.id && s.status !== 'idle'))
        list.push({ label: `Run ${appName(a)}`, hint: String(a.port), run: () => void start(a.id) })
    }
    list.push({ label: 'New project', hint: '', run: () => void navigate({ to: '/new' }) })
    return list
  }, [snap, start, stop, navigate])

  const q = query.trim().toLowerCase()
  const shown = q ? actions.filter((a) => q.split(/\s+/).every((word) => a.label.toLowerCase().includes(word))) : actions
  const pick = (i: number) => {
    shown[i]?.run()
    close(false)
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/50 px-4 pt-[14vh] backdrop-blur-sm"
      onClick={() => close(false)}
    >
      <div
        role="dialog"
        aria-label="Run something"
        className="w-full max-w-lg overflow-hidden rounded-xl border border-line-strong bg-bg shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <input
          autoFocus
          value={query}
          onChange={(e) => {
            setQuery(e.target.value)
            setAt(0)
          }}
          onKeyDown={(e) => {
            if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
              e.preventDefault()
              setAt((i) => Math.max(0, Math.min(i + (e.key === 'ArrowDown' ? 1 : -1), shown.length - 1)))
            }
            if (e.key === 'Enter') pick(at)
            if (e.key === 'Escape') close(false)
          }}
          placeholder="Run, stop or open…"
          aria-label="Run, stop or open"
          className="w-full border-b border-line bg-transparent px-4 py-3.5 text-[15px] placeholder:text-faint focus:outline-none"
        />
        <ul className="max-h-80 overflow-auto p-1.5">
          {shown.map((a, i) => (
            <li key={a.label}>
              <button
                type="button"
                onMouseEnter={() => setAt(i)}
                onClick={() => pick(i)}
                className={`flex w-full items-center justify-between rounded-md px-3 py-2 text-left text-[13px] ${i === at ? 'bg-panel text-fg' : 'text-muted'}`}
              >
                {a.label}
                <span className="font-mono text-[11px] text-faint">{a.hint}</span>
              </button>
            </li>
          ))}
          {shown.length === 0 && <li className="px-3 py-2 text-[13px] text-faint">Nothing matches.</li>}
        </ul>
      </div>
    </div>
  )
}

/* ---------- New project, at /new ---------- */

export function NewProject() {
  const snap = useCarbon((s) => s.snap)
  const busy = useCarbon((s) => s.busy)
  const error = useCarbon((s) => s.error)
  const create = useCarbon((s) => s.create)
  const navigate = useNavigate()
  const [name, setName] = useState('')
  const [group, setGroup] = useState('')
  const [template, setTemplate] = useState('start')
  if (!snap) return null
  const into = group || snap.groups[0] || ''
  const valid = /^[a-z0-9][a-z0-9-]*$/.test(name)
  const back = () => void navigate({ to: '/' })

  return (
    <div className="fixed inset-0 z-40 flex items-start justify-center bg-black/50 px-4 pt-[10vh] backdrop-blur-sm" onClick={back}>
      <form
        aria-label="New project"
        className="w-full max-w-lg rounded-xl border border-line-strong bg-bg shadow-2xl"
        onClick={(e) => e.stopPropagation()}
        onSubmit={async (e) => {
          e.preventDefault()
          if (await create(into, name, template)) back()
        }}
      >
        <div className="border-b border-line px-5 py-4">
          <h2 className="text-[15px] font-medium">New project</h2>
          <p className="mt-0.5 font-mono text-[11px] text-faint">
            ~/{into}/{name || 'name'}
          </p>
        </div>
        <div className="flex flex-col gap-5 px-5 py-5">
          <label className="flex flex-col gap-2 text-[13px]">
            Name
            <input
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value.toLowerCase().replace(/\s+/g, '-'))}
              placeholder="lowercase-with-dashes"
              className="h-9 rounded-md border border-line bg-transparent px-3 font-mono text-[13px] placeholder:text-faint focus:border-line-strong focus:outline-none"
            />
          </label>
          <fieldset className="flex flex-col gap-2 text-[13px]">
            <legend className="mb-2">Where</legend>
            <div className="flex flex-wrap gap-1.5">
              {snap.groups.map((g) => (
                <button
                  key={g}
                  type="button"
                  aria-pressed={g === into}
                  onClick={() => setGroup(g)}
                  className={`btn ${g === into ? 'btn-solid' : ''}`}
                >
                  {g}
                </button>
              ))}
            </div>
          </fieldset>
          <fieldset className="text-[13px]">
            <legend className="mb-2">Starting from</legend>
            <div className="grid gap-1.5">
              {snap.templates.map((t) => (
                <button
                  key={t.id}
                  type="button"
                  aria-pressed={t.id === template}
                  onClick={() => setTemplate(t.id)}
                  className={`rounded-md border px-3 py-2.5 text-left transition-colors ${t.id === template ? 'border-fg' : 'border-line hover:border-line-strong'}`}
                >
                  <span className="block text-[13px] font-medium">{t.label}</span>
                  <span className="block text-[12px] text-muted">{t.detail}</span>
                </button>
              ))}
            </div>
          </fieldset>
          {error && <p className="font-mono text-[12px] text-red-500">{error}</p>}
        </div>
        <div className="flex justify-end gap-2 border-t border-line px-5 py-3.5">
          <button type="button" className="btn" onClick={back}>
            Cancel
          </button>
          <button type="submit" className="btn btn-solid" disabled={!valid || !into || busy !== null}>
            {busy === 'create' ? 'Creating…' : 'Create'}
          </button>
        </div>
      </form>
    </div>
  )
}

/* ---------- The page ---------- */

export function Shell() {
  const snap = useCarbon((s) => s.snap)
  const offline = useCarbon((s) => s.offline)
  const error = useCarbon((s) => s.error)
  const palette = useCarbon((s) => s.palette)
  const setPalette = useCarbon((s) => s.setPalette)
  const refresh = useCarbon((s) => s.refresh)

  // Keep reading what is running, but only while the tab is being looked at
  useEffect(() => {
    void refresh()
    const tick = () => {
      if (!document.hidden) void refresh()
    }
    const timer = setInterval(tick, 1500)
    document.addEventListener('visibilitychange', tick)
    return () => {
      clearInterval(timer)
      document.removeEventListener('visibilitychange', tick)
    }
  }, [refresh])

  // ⌘K, or / when not typing, opens the palette
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLInputElement
      if ((e.key === 'k' && (e.metaKey || e.ctrlKey)) || (e.key === '/' && !typing)) {
        e.preventDefault()
        setPalette(true)
      }
    }
    addEventListener('keydown', onKey)
    return () => removeEventListener('keydown', onKey)
  }, [setPalette])

  if (!snap) {
    return (
      <p className="grid h-full place-items-center font-mono text-[12px] text-faint">{offline ? 'Carbon isn’t answering on 3000.' : ''}</p>
    )
  }

  const byId = new Map(snap.apps.map((a) => [a.id, a]))
  // Every server running: on a Carbon port, started by hand elsewhere, or started by an agent off Carbon's ports
  const onSlots = new Set(snap.slots.map((s) => s.pid))
  const byAgents = snap.agents.flatMap((a) => a.servers).filter((s) => !onSlots.has(s.pid)).length
  const running = snap.slots.filter((s) => s.status === 'running').length + snap.others.length + byAgents

  return (
    <div className="flex h-full flex-col">
      <header className="flex shrink-0 items-center justify-between gap-4 border-b border-line px-4 py-2.5">
        <div className="flex items-center gap-2.5">
          <Mark />
          <h1 className="text-[14px] font-semibold tracking-tight">Carbon</h1>
          <span className="font-mono text-[11px] text-faint">3000</span>
        </div>
        <button
          type="button"
          onClick={() => setPalette(true)}
          className="hidden h-8 w-72 items-center justify-between rounded-md border border-line px-3 text-[13px] text-faint transition-colors hover:border-line-strong sm:flex"
        >
          Run, stop or open…
          <kbd className="font-mono text-[11px]">⌘K</kbd>
        </button>
        <div className="flex items-center gap-3">
          <p className="hidden font-mono text-[11px] text-muted md:block">
            {running} live <span className="text-faint">of {snap.apps.length}</span>
          </p>
          <Link to="/new" className="btn btn-solid">
            New project
          </Link>
        </div>
      </header>

      {(error || offline) && (
        <p className="shrink-0 border-b border-line bg-panel px-4 py-2 font-mono text-[12px] text-red-500">
          {offline ? 'Carbon isn’t answering. Showing the last thing it said.' : error}
        </p>
      )}

      <main className="min-h-0 flex-1 overflow-auto">
        {/* The rack. gap-px over a line-coloured background draws the hairlines between the ports */}
        <section aria-label="Ports" className="grid grid-cols-2 gap-px border-b border-line bg-line md:grid-cols-4">
          {snap.slots.map((s) => (
            <Port
              key={s.port}
              slot={s}
              app={s.app ? byId.get(s.app) : undefined}
              pictures={snap.slots.some((x) => x.status === 'running')}
            />
          ))}
        </section>

        {snap.agents.length > 0 && (
          <section className="border-b border-line">
            <h2 className="px-4 py-3 text-[13px] font-medium">
              From agents <span className="font-normal text-faint">servers a coding agent started, by session</span>
            </h2>
            {snap.agents.map((a) => (
              <AgentSession key={a.pid} agent={a} apps={byId} />
            ))}
          </section>
        )}

        {snap.others.length > 0 && (
          <section className="border-b border-line">
            <h2 className="px-4 py-3 text-[13px] font-medium">
              Elsewhere <span className="font-normal text-faint">dev servers started by hand, outside Carbon</span>
            </h2>
            <ul>
              {snap.others.map((o) => (
                <Elsewhere key={`${o.pid}:${o.port}`} item={o} app={o.app ? byId.get(o.app) : undefined} />
              ))}
            </ul>
          </section>
        )}

        <section>
          <h2 className="px-4 py-3 text-[13px] font-medium">
            Projects <span className="font-normal text-faint">running an app puts it on its port and clears what was there</span>
          </h2>
          {/* gap-px over a line-coloured background draws the hairlines between the cards */}
          <div className="grid grid-cols-1 gap-px border-t border-line bg-line lg:grid-cols-2 2xl:grid-cols-3">
            {snap.projects.map((p) => (
              <ProjectCard key={p.key} project={p} apps={p.apps.map((id) => byId.get(id)!).filter(Boolean)} slots={snap.slots} />
            ))}
            {/* Blank cells finish the last row, so the grid's line colour never shows as a block */}
            {snap.projects.length % 2 === 1 && <div className="hidden bg-bg lg:block 2xl:hidden" />}
            {Array.from({ length: (3 - (snap.projects.length % 3)) % 3 }, (_, i) => (
              <div key={i} className="hidden bg-bg 2xl:block" />
            ))}
          </div>
        </section>
      </main>

      <Console />
      {palette && <Palette />}
      <Outlet />
    </div>
  )
}
