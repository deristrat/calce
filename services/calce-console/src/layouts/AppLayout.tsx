import { Outlet, useLocation } from 'react-router'
import Sidebar from '../components/Sidebar'
import Breadcrumbs from '../components/Breadcrumbs'
import type { BreadcrumbItem } from '../components/Breadcrumbs'
import { useMemo } from 'react'

const segmentLabels: Record<string, string> = {
  dashboard: 'Dashboard',
  organizations: 'Organizations',
  users: 'Users',
  instruments: 'Instruments',
  'fx-rates': 'FX Rates',
  'update-simulator': 'Update Simulator',
  design: 'Design System',
}

export default function AppLayout() {
  const { pathname } = useLocation()

  const breadcrumbItems = useMemo<BreadcrumbItem[]>(() => {
    const segments = pathname.split('/').filter(Boolean)
    return segments.map((segment, i) => {
      const label = segmentLabels[segment] || segment
      const isLast = i === segments.length - 1
      const to = isLast ? undefined : '/' + segments.slice(0, i + 1).join('/')
      return { label, to }
    })
  }, [pathname])

  return (
    <div className="ds-app">
      <Sidebar />
      <div className="ds-app__main">
        <Breadcrumbs items={breadcrumbItems} />
        <div className="ds-app__content">
          <Outlet />
        </div>
      </div>
    </div>
  )
}
