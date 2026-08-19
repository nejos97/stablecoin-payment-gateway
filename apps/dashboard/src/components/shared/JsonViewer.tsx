import type { ReactNode } from "react"

// One JSON token per match: string (optionally a key, when followed by a
// colon), number, boolean or null. Pretty-printed JSON never splits a token
// across lines, so each line can be highlighted independently.
const TOKEN_RE =
  /("(?:\\.|[^"\\])*")(\s*:)?|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?|\b(?:true|false|null)\b/g

function highlightLine(line: string, lineIndex: number): ReactNode[] {
  const parts: ReactNode[] = []
  let cursor = 0
  for (const match of line.matchAll(TOKEN_RE)) {
    const index = match.index ?? 0
    if (index > cursor) parts.push(line.slice(cursor, index))
    const [token, string, colon] = match
    const key = `${lineIndex}-${index}`
    if (string !== undefined) {
      if (colon !== undefined) {
        parts.push(
          <span key={key} className="text-sky-700 dark:text-sky-300">
            {string}
          </span>,
          colon,
        )
      } else {
        parts.push(
          <span key={key} className="text-emerald-700 dark:text-emerald-400">
            {string}
          </span>,
        )
      }
    } else if (token === "true" || token === "false" || token === "null") {
      parts.push(
        <span key={key} className="text-purple-700 dark:text-purple-400">
          {token}
        </span>,
      )
    } else {
      parts.push(
        <span key={key} className="text-amber-700 dark:text-amber-400">
          {token}
        </span>,
      )
    }
    cursor = index + token.length
  }
  if (cursor < line.length) parts.push(line.slice(cursor))
  return parts
}

/** Pretty-printed, syntax-highlighted JSON block with line numbers. */
export function JsonViewer({ value }: { value: unknown }) {
  const lines = JSON.stringify(value, null, 2).split("\n")
  return (
    <div className="overflow-auto rounded-md border bg-muted/50 text-xs">
      <pre className="min-w-max p-3 font-mono leading-relaxed">
        {lines.map((line, i) => (
          <div key={i} className="flex">
            <span className="w-8 shrink-0 select-none pr-3 text-right text-muted-foreground/60">
              {i + 1}
            </span>
            <code>{highlightLine(line, i)}</code>
          </div>
        ))}
      </pre>
    </div>
  )
}
