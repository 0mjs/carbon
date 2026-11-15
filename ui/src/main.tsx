import { RouterProvider, createRootRoute, createRoute, createRouter } from '@tanstack/react-router'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { NewProject, Shell } from './app'
import './styles.css'

/* Two addresses: the dashboard, and the dashboard with the new-project form over it */
const root = createRootRoute({ component: Shell })
const routeTree = root.addChildren([
  createRoute({ getParentRoute: () => root, path: '/' }),
  createRoute({ getParentRoute: () => root, path: '/new', component: NewProject }),
])
const router = createRouter({ routeTree })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}

createRoot(document.getElementById('app')!).render(
  <StrictMode>
    <RouterProvider router={router} />
  </StrictMode>,
)
