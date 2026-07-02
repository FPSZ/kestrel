import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { AppShell } from './app/app-shell'
import './index.css'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <AppShell />
  </StrictMode>,
)
