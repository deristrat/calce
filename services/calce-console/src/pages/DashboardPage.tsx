import { useQuery } from '@tanstack/react-query'
import { api } from '../api/client'
import StatCard from '../components/StatCard'
import Spinner from '../components/Spinner'
import { usePageTitle } from '../hooks/usePageTitle'
import { useEntityEvents } from '../hooks/useEntityEvents'

export default function DashboardPage() {
  usePageTitle('Dashboard')
  useEntityEvents([], { alsoInvalidate: ['stats'] })
  const { data: stats, isLoading, error } = useQuery({
    queryKey: ['stats'],
    queryFn: () => api.getStats(),
  })

  return (
    <div className="ds-page">
      <div className="ds-page__header">
        <h1 className="ds-page__title">Dashboard</h1>
      </div>
      {error ? (
        <p className="ds-text--secondary">Failed to load stats: {error.message}</p>
      ) : isLoading ? (
        <Spinner size="lg" center />
      ) : stats ? (
        <div className="ds-grid ds-grid--cols-3">
          <StatCard label="Organizations" value={stats.organization_count.toLocaleString()} to="/organizations" />
          <StatCard label="Users" value={stats.user_count.toLocaleString()} to="/users" />
          <StatCard label="Instruments" value={stats.instrument_count.toLocaleString()} to="/instruments" />
          <StatCard label="Trades" value={stats.trade_count.toLocaleString()} />
          <StatCard label="Prices" value={stats.price_count.toLocaleString()} />
          <StatCard label="FX Rates" value={stats.fx_rate_count.toLocaleString()} to="/fx-rates" />
        </div>
      ) : null}
    </div>
  )
}
