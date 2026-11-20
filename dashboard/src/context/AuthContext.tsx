import { createContext, useContext, useState, useEffect, useRef, ReactNode, useCallback } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import http, { TOKEN_KEY } from '../api/client'

interface AuthContextValue {
  token: string | null
  user: { username: string; role: string } | null
  login: (username: string, password: string) => Promise<void>
  logout: () => void
  isAuthenticated: boolean
}

const AuthContext = createContext<AuthContextValue>({
  token: null,
  user: null,
  login: async () => {},
  logout: () => {},
  isAuthenticated: false,
})

export function AuthProvider({ children }: { children: ReactNode }) {
  const qc = useQueryClient()
  const [token, setToken] = useState<string | null>(() => localStorage.getItem(TOKEN_KEY))
  const [user, setUser] = useState<{ username: string; role: string } | null>(null)
  const hydrated = useRef(false)

  const logout = useCallback(() => {
    localStorage.removeItem(TOKEN_KEY)
    delete http.defaults.headers.common['Authorization']
    hydrated.current = false
    setToken(null)
    setUser(null)
    qc.clear()
  }, [qc])

  // Global 401 response interceptor so expired/revoked sessions immediately log out
  useEffect(() => {
    const id = http.interceptors.response.use(
      (res) => res,
      (err) => {
        if (err?.response?.status === 401 && !err?.config?.url?.includes('/auth/login')) {
          logout()
        }
        return Promise.reject(err)
      },
    )
    return () => {
      http.interceptors.response.eject(id)
    }
  }, [logout])

  // Multi-tab logout synchronization
  useEffect(() => {
    const onStorage = (e: StorageEvent) => {
      if (e.key === TOKEN_KEY && !e.newValue) {
        logout()
      }
    }
    window.addEventListener('storage', onStorage)
    return () => window.removeEventListener('storage', onStorage)
  }, [logout])

  // Restore auth state on reload
  useEffect(() => {
    if (!token) {
      delete http.defaults.headers.common['Authorization']
      return
    }
    http.defaults.headers.common['Authorization'] = `Bearer ${token}`
    if (!hydrated.current) {
      hydrated.current = true
      http
        .get('/auth/me')
        .then((r) => setUser(r.data))
        .catch(() => logout())
    }
  }, [token, logout])

  const login = async (username: string, password: string) => {
    const resp = await http.post('/auth/login', { username, password })
    const tok: string = resp.data.access_token
    const role: string = resp.data.role ?? 'viewer'
    const uname: string = resp.data.username ?? username
    localStorage.setItem(TOKEN_KEY, tok)
    http.defaults.headers.common['Authorization'] = `Bearer ${tok}`
    hydrated.current = true
    setToken(tok)
    setUser({ username: uname, role })
  }

  return (
    <AuthContext.Provider value={{ token, user, login, logout, isAuthenticated: !!token }}>
      {children}
    </AuthContext.Provider>
  )
}

export const useAuth = () => useContext(AuthContext)
