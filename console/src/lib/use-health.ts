import { useEffect, useState } from 'react'
import { client } from './client'
import type { Health } from './types'

/** Poll /api/health for the status pill (server up + model name). */
export function useHealth(pollMs = 10_000): Health | null {
  const [health, setHealth] = useState<Health | null>(null)

  useEffect(() => {
    let alive = true
    const ping = () =>
      client
        .health()
        .then((h) => alive && setHealth(h))
        .catch(() => alive && setHealth(null))
    ping()
    const t = setInterval(ping, pollMs)
    return () => {
      alive = false
      clearInterval(t)
    }
  }, [pollMs])

  return health
}
