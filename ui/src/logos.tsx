/* The marks of the coding agents Carbon can recognise, so anything one of them started is labelled at a glance,
   and of the tools apps are built with. The artwork is the shared set in /marks (see its README for where each
   comes from), one file per harness id and, in tech/, one per tool.
   Marks are light, so each sits on a dark tile of its own in either theme. A harness without a mark is shown by
   its first letter. */

const MARKS = import.meta.glob<string>('../../marks/*.svg', { eager: true, query: '?url', import: 'default' })
const BY_ID = new Map(Object.entries(MARKS).map(([path, url]) => [path.slice(path.lastIndexOf('/') + 1, -'.svg'.length), url]))

const TECH = import.meta.glob<string>('../../marks/tech/*.svg', { eager: true, query: '?url', import: 'default' })
const TECH_BY_ID = new Map(Object.entries(TECH).map(([path, url]) => [path.slice(path.lastIndexOf('/') + 1, -'.svg'.length), url]))

/** A tool an app is built with: its mark on a dark tile, and its name beside it when it is the main one */
export function TechMark({ id, name, named = false }: { id: string; name: string; named?: boolean }) {
  const url = TECH_BY_ID.get(id)
  const mark = (
    <span role="img" aria-label={name} title={name} className="grid size-4 shrink-0 place-items-center rounded-[28%] bg-[#1b1b1d]">
      {url && <img src={url} alt="" className="size-[62%]" />}
    </span>
  )
  if (!named) return mark
  return (
    <span className="tag flex shrink-0 items-center gap-1.5">
      {mark}
      {name}
    </span>
  )
}

export function AgentMark({ harness, name, className = 'size-5' }: { harness: string; name: string; className?: string }) {
  const url = BY_ID.get(harness)
  return (
    <span role="img" aria-label={name} title={name} className={`grid shrink-0 place-items-center rounded-[28%] bg-[#1b1b1d] ${className}`}>
      {url ? (
        <img src={url} alt="" className="size-[62%]" />
      ) : (
        <span className="text-[0.6em] font-semibold text-[#ededed]">{name.slice(0, 1).toUpperCase()}</span>
      )}
    </span>
  )
}
