import { createContext, useContext, useState, useEffect, useRef, ReactNode } from 'react'
import http from '../api/client'

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

const TOKEN_KEY = 'qorgau_token'

export function AuthProvider({ children }: { children: ReactNode }) {
  const [token, setToken] = useState<string | null>(() => localStorage.getItem(TOKEN_KEY))
  const [user, setUser] = useState<{ username: string; role: string } | null>(null)
  const hydrated = useRef(false)

  // On page reload: restore auth header and hydrate user info
  useEffect(() => {
    if (!token) {
      delete http.defaults.headers.common['Authorization']
      return
    }
    http.defaults.headers.common['Authorization'] = `Bearer ${token}`
    // Only call /auth/me on first mount (page reload), not during login flow
    if (!hydrated.current) {
      hydrated.current = true
      http.get('/auth/me')
        .then(r => setUser(r.data))
        .catch(() => {
          // Token invalid — clear it
          localStorage.removeItem(TOKEN_KEY)
          delete http.defaults.headers.common['Authorization']
          setToken(null)
          setUser(null)
        })
    }
  }, [token])

  const login = async (username: string, password: string) => {
    const resp = await http.post('/auth/login', { username, password })
    const tok: string = resp.data.access_token
    localStorage.setItem(TOKEN_KEY, tok)
    http.defaults.headers.common['Authorization'] = `Bearer ${tok}`
    hydrated.current = true  // prevent effect from calling /auth/me again
    setToken(tok)
    // Get user info from /auth/me (token is already set in headers above)
    try {
      const me = await http.get('/auth/me')
      setUser(me.data)
    } catch {
      // Best-effort: set minimal user from credentials
      setUser({ username, role: 'admin' })
    }
  }

  const logout = () => {
    localStorage.removeItem(TOKEN_KEY)
    delete http.defaults.headers.common['Authorization']
    hydrated.current = false
    setToken(null)
    setUser(null)
  }

  return (
    <AuthContext.Provider value={{ token, user, login, logout, isAuthenticated: !!token }}>
      {children}
    </AuthContext.Provider>
  )
}

export const useAuth = () => useContext(AuthContext)
