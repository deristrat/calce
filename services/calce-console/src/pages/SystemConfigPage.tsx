import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { api } from '../api/client'
import Badge from '../components/Badge'
import Card from '../components/Card'
import Spinner from '../components/Spinner'
import type { SystemConfigEntry } from '../api/types'
import { usePageTitle } from '../hooks/usePageTitle'

export default function SystemConfigPage() {
  usePageTitle('System · Config')

  const { data, isLoading, error } = useQuery({
    queryKey: ['system', 'config'],
    queryFn: () => api.getSystemConfig(),
  })

  const grouped = useMemo(() => {
    if (!data) return []
    const byGroup = new Map<string, SystemConfigEntry[]>()
    for (const entry of data.entries) {
      const list = byGroup.get(entry.group) ?? []
      list.push(entry)
      byGroup.set(entry.group, list)
    }
    return Array.from(byGroup.entries()).sort(([a], [b]) => a.localeCompare(b))
  }, [data])

  if (isLoading) return <div className="ds-page"><Spinner size="lg" center /></div>
  if (error) {
    return (
      <div className="ds-page">
        <div className="ds-page__header"><h1 className="ds-page__title">System · Config</h1></div>
        <p className="ds-text--error">Failed to load: {error.message}</p>
      </div>
    )
  }
  if (!data) return null

  return (
    <div className="ds-page">
      <div className="ds-page__header">
        <div>
          <h1 className="ds-page__title">System · Config</h1>
          <p className="ds-text--secondary ds-mt-xs">
            Environment variables visible to the API. Secrets are masked — first
            8 characters shown alongside total length.
          </p>
        </div>
      </div>

      {grouped.map(([group, entries]) => (
        <Card key={group} header={<h3>{group}</h3>} className="ds-mb-lg">
          <table className="ds-table">
            <thead>
              <tr>
                <th className="ds-table__cell">Key</th>
                <th className="ds-table__cell">Value</th>
                <th className="ds-table__cell">Description</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((e) => (
                <tr key={e.key}>
                  <td className="ds-table__cell">
                    <span className="ds-text--mono">{e.key}</span>
                    {e.secret && (
                      <>
                        {' '}
                        <Badge variant="warning">secret</Badge>
                      </>
                    )}
                  </td>
                  <td className="ds-table__cell ds-text--mono">
                    {e.value === null ? (
                      <span className="ds-text--secondary">(unset)</span>
                    ) : (
                      e.value
                    )}
                  </td>
                  <td className="ds-table__cell ds-text--secondary">{e.description}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      ))}
    </div>
  )
}
