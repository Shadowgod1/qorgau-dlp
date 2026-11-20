import { useEffect, useState } from 'react'
import http from '../api/client'

/**
 * Loads a protected binary media endpoint (screenshot, webcam photo, audio WAV)
 * using the Axios `Authorization: Bearer` header and converts it into a short-lived
 * Blob object URL — eliminating JWT leakage in URL query strings (`?token=`).
 */
export function useAuthMediaUrl(apiPath: string | null): string | undefined {
  const [blobUrl, setBlobUrl] = useState<string | undefined>(undefined)

  useEffect(() => {
    if (!apiPath) {
      setBlobUrl(undefined)
      return
    }
    let active = true
    let created: string | null = null

    http
      .get(apiPath, { responseType: 'blob' })
      .then((resp) => {
        if (!active) return
        created = URL.createObjectURL(resp.data as Blob)
        setBlobUrl(created)
      })
      .catch(() => {
        if (active) setBlobUrl(undefined)
      })

    return () => {
      active = false
      if (created) URL.revokeObjectURL(created)
    }
  }, [apiPath])

  return blobUrl
}
