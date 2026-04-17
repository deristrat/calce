import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { api } from '../api/client'
import Badge from '../components/Badge'
import Card from '../components/Card'
import Spinner from '../components/Spinner'
import { usePageTitle } from '../hooks/usePageTitle'

function statusVariant(status: string | null | undefined): 'neutral' | 'success' | 'warning' | 'error' {
  if (!status) return 'neutral'
  const s = status.toLowerCase()
  if (s === 'ok' || s === 'running' || s === 'connected' || s === 'enabled' || s === 'available') return 'success'
  if (s === 'disabled') return 'neutral'
  if (s === 'degraded' || s === 'warning') return 'warning'
  if (s === 'error' || s === 'down') return 'error'
  return 'neutral'
}

function formatUptime(startedAt: string): string {
  const ms = Date.now() - new Date(startedAt).getTime()
  if (ms < 0) return '0s'
  const s = Math.floor(ms / 1000)
  const d = Math.floor(s / 86400)
  const h = Math.floor((s % 86400) / 3600)
  const m = Math.floor((s % 3600) / 60)
  const sec = s % 60
  if (d > 0) return `${d}d ${h}h ${m}m`
  if (h > 0) return `${h}h ${m}m`
  if (m > 0) return `${m}m ${sec}s`
  return `${sec}s`
}

export default function SystemOverviewPage() {
  usePageTitle('System · Overview')

  const { data, isLoading, error } = useQuery({
    queryKey: ['system', 'info'],
    queryFn: () => api.getSystemInfo(),
    refetchInterval: 10_000,
  })

  const consoleOrigin = useMemo(() => window.location.origin, [])

  if (isLoading) return <div className="ds-page"><Spinner size="lg" center /></div>
  if (error) {
    return (
      <div className="ds-page">
        <div className="ds-page__header"><h1 className="ds-page__title">System · Overview</h1></div>
        <p className="ds-text--error">Failed to load: {error.message}</p>
      </div>
    )
  }
  if (!data) return null

  return (
    <div className="ds-page">
      <div className="ds-page__header">
        <div>
          <h1 className="ds-page__title">System · Overview</h1>
          <p className="ds-text--secondary ds-mt-xs">
            Running services and in-process components
          </p>
        </div>
      </div>

      <Card header={<h3>API</h3>} className="ds-mb-lg">
        <div className="ds-kv-grid">
          <div className="ds-kv-grid__label">Version</div>
          <div className="ds-text--mono">{data.api.version}</div>
          <div className="ds-kv-grid__label">Profile</div>
          <div className="ds-text--mono">{data.api.profile}</div>
          <div className="ds-kv-grid__label">Target</div>
          <div className="ds-text--mono">{data.api.target}</div>
          <div className="ds-kv-grid__label">Started</div>
          <div className="ds-text--mono">{new Date(data.api.started_at).toLocaleString()}</div>
          <div className="ds-kv-grid__label">Uptime</div>
          <div className="ds-text--mono">{formatUptime(data.api.started_at)}</div>
        </div>
      </Card>

      <Card header={<h3>Services</h3>} className="ds-mb-lg">
        <table className="ds-table">
          <thead>
            <tr>
              <th className="ds-table__cell">Name</th>
              <th className="ds-table__cell">Role</th>
              <th className="ds-table__cell">URL</th>
              <th className="ds-table__cell">Status</th>
            </tr>
          </thead>
          <tbody>
            {data.services.map((svc) => {
              const url = svc.name === 'calce-console' ? consoleOrigin : svc.url
              return (
                <tr key={svc.name}>
                  <td className="ds-table__cell">
                    <strong>{svc.name}</strong>
                  </td>
                  <td className="ds-table__cell ds-text--secondary">{svc.role}</td>
                  <td className="ds-table__cell ds-text--mono">{url}</td>
                  <td className="ds-table__cell">
                    {svc.status ? (
                      <Badge variant={statusVariant(svc.status)}>{svc.status}</Badge>
                    ) : (
                      <span className="ds-text--secondary">—</span>
                    )}
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </Card>

      <Card header={<h3>Components</h3>}>
        <table className="ds-table">
          <thead>
            <tr>
              <th className="ds-table__cell">Name</th>
              <th className="ds-table__cell">Status</th>
              <th className="ds-table__cell">Detail</th>
            </tr>
          </thead>
          <tbody>
            {data.components.map((c) => (
              <tr key={c.name}>
                <td className="ds-table__cell">{c.name}</td>
                <td className="ds-table__cell">
                  <Badge variant={statusVariant(c.status)}>{c.status}</Badge>
                </td>
                <td className="ds-table__cell ds-text--secondary">{c.detail ?? '—'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </Card>
    </div>
  )
}
