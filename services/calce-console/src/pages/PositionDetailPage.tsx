import { useParams, Link } from 'react-router'
import { useQuery } from '@tanstack/react-query'
import type { ColumnDef } from '@tanstack/react-table'
import { useMemo, useState } from 'react'
import { api } from '../api/client'
import type { TradeSummary } from '../api/types'
import { IconChevronLeft } from '../components/icons'
import Card from '../components/Card'
import Badge from '../components/Badge'
import DataTable from '../components/DataTable'
import PriceChart from '../components/PriceChart'
import type { ChartMarker } from '../components/PriceChart'
import Spinner from '../components/Spinner'
import { usePageTitle } from '../hooks/usePageTitle'

export default function PositionDetailPage() {
  const { userId, instrumentId } = useParams()

  const { data: user, isLoading: userLoading } = useQuery({
    queryKey: ['user', userId],
    queryFn: () => api.getUser(userId!),
    enabled: !!userId,
  })

  const { data: positions, isLoading: positionsLoading } = useQuery({
    queryKey: ['user-positions', userId],
    queryFn: () => api.getUserPositions(userId!),
    enabled: !!userId,
  })

  const position = positions?.find((p) => p.instrument_id === instrumentId)

  const { data: trades, isLoading: tradesLoading } = useQuery({
    queryKey: ['position-trades', userId, instrumentId],
    queryFn: () => api.getUserPositionTrades(userId!, instrumentId!),
    enabled: !!userId && !!instrumentId,
  })

  const { data: prices, isLoading: pricesLoading } = useQuery({
    queryKey: ['instrument-prices', instrumentId],
    queryFn: () => {
      const to = new Date().toISOString().slice(0, 10)
      const from = new Date(Date.now() - 5 * 365 * 24 * 60 * 60 * 1000)
        .toISOString()
        .slice(0, 10)
      return api.getInstrumentPrices(instrumentId!, { from, to })
    },
    enabled: !!instrumentId,
  })

  const [showTrades, setShowTrades] = useState(true)

  const tradeMarkers = useMemo<ChartMarker[]>(() => {
    if (!trades) return []
    return trades.map((t) => ({
      time: t.date,
      position: t.quantity > 0 ? 'belowBar' as const : 'aboveBar' as const,
      color: t.quantity > 0
        ? getComputedStyle(document.documentElement).getPropertyValue('--color-success').trim() || '#34a853'
        : getComputedStyle(document.documentElement).getPropertyValue('--color-danger').trim() || '#ea4335',
      shape: 'circle' as const,
      text: t.quantity > 0 ? 'B' : 'S',
    }))
  }, [trades])

  const instrumentLabel = position?.instrument_name || instrumentId
  usePageTitle(instrumentLabel ? `${instrumentLabel} Position` : 'Position')

  const tradeColumns = useMemo<ColumnDef<TradeSummary, unknown>[]>(
    () => [
      {
        accessorKey: 'date',
        header: 'Date',
        cell: ({ getValue }) =>
          new Date(getValue<string>()).toLocaleDateString(),
      },
      {
        accessorKey: 'quantity',
        header: 'Quantity',
        meta: { numeric: true },
        cell: ({ getValue }) => {
          const val = getValue<number>()
          return (
            <span className="ds-text--mono">
              {val > 0 ? '+' : ''}
              {val.toLocaleString(undefined, {
                minimumFractionDigits: 2,
                maximumFractionDigits: 4,
              })}
            </span>
          )
        },
      },
      {
        accessorKey: 'price',
        header: 'Price',
        meta: { numeric: true },
        cell: ({ getValue }) => (
          <span className="ds-text--mono">
            {getValue<number>().toLocaleString(undefined, {
              minimumFractionDigits: 2,
              maximumFractionDigits: 4,
            })}
          </span>
        ),
      },
      {
        accessorKey: 'total_value',
        header: 'Total Value',
        meta: { numeric: true },
        cell: ({ getValue }) => (
          <span className="ds-text--mono">
            {getValue<number>().toLocaleString(undefined, {
              minimumFractionDigits: 2,
              maximumFractionDigits: 2,
            })}
          </span>
        ),
      },
      { accessorKey: 'currency', header: 'Currency' },
      {
        accessorKey: 'account_name',
        header: 'Account',
        cell: ({ getValue, row }) =>
          getValue<string | null>() || row.original.account_id,
      },
    ],
    []
  )

  const isLoading = userLoading || positionsLoading

  if (isLoading) {
    return (
      <div className="ds-page">
        <Spinner size="lg" center />
      </div>
    )
  }

  return (
    <div className="ds-page">
      <Link to={`/users/${userId}`} className="ds-back-link">
        <IconChevronLeft size={12} /> Back to {user?.name || `User ${userId}`}
      </Link>
      <div className="ds-page__header">
        <div className="ds-page__actions">
          <h1 className="ds-page__title">{position?.instrument_name || instrumentId}</h1>
          {position && <Badge variant="neutral">{position.currency}</Badge>}
        </div>
      </div>

      {position && (
        <div className="ds-kv-inline ds-mt-md">
          <span className="ds-kv-inline__item">
            <span className="ds-kv-inline__label">Instrument</span>
            <span className="ds-text--mono">{position.instrument_id}</span>
          </span>
          {position.instrument_name && (
            <span className="ds-kv-inline__item">
              <span className="ds-kv-inline__label">Name</span>
              <span>{position.instrument_name}</span>
            </span>
          )}
          <span className="ds-kv-inline__item">
            <span className="ds-kv-inline__label">Quantity</span>
            <span className="ds-text--mono">
              {position.quantity.toLocaleString(undefined, {
                minimumFractionDigits: 2,
                maximumFractionDigits: 4,
              })}
            </span>
          </span>
          <span className="ds-kv-inline__item">
            <span className="ds-kv-inline__label">Currency</span>
            <span>{position.currency}</span>
          </span>
          <span className="ds-kv-inline__item">
            <span className="ds-kv-inline__label">Trades</span>
            <span>{position.trade_count}</span>
          </span>
        </div>
      )}

      <div className="ds-chart-container ds-mt-lg">
        <div className="ds-kv-inline ds-mb-sm">
          <label className="ds-kv-inline__item" style={{ cursor: 'pointer' }}>
            <button
              type="button"
              className={`ds-toggle${showTrades ? ' ds-toggle--checked' : ''}`}
              onClick={() => setShowTrades(!showTrades)}
              aria-pressed={showTrades}
            />
            <span>Show trades</span>
          </label>
        </div>
        {pricesLoading ? (
          <Spinner size="md" center />
        ) : prices && prices.length > 0 ? (
          <PriceChart data={prices} markers={showTrades ? tradeMarkers : undefined} />
        ) : (
          <p className="ds-text--secondary">No price data available.</p>
        )}
      </div>

      <Card header="Trades" className="ds-mt-xl">
        {tradesLoading ? (
          <Spinner size="md" center />
        ) : trades && trades.length > 0 ? (
          <DataTable data={trades} columns={tradeColumns} />
        ) : (
          <p className="ds-text--secondary">No trades.</p>
        )}
      </Card>
    </div>
  )
}
