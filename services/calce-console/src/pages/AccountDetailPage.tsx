import { useParams, Link, useNavigate } from 'react-router'
import { useQuery } from '@tanstack/react-query'
import type { ColumnDef } from '@tanstack/react-table'
import { useMemo } from 'react'
import { api } from '../api/client'
import type { PositionSummary, TradeSummary } from '../api/types'
import { IconChevronLeft } from '../components/icons'
import Card from '../components/Card'
import Badge from '../components/Badge'
import DataTable from '../components/DataTable'
import Spinner from '../components/Spinner'
import { usePageTitle } from '../hooks/usePageTitle'
import { useEntityEvents } from '../hooks/useEntityEvents'

export default function AccountDetailPage() {
  const { userId, accountId } = useParams()
  const navigate = useNavigate()
  const accountIdNum = Number(accountId)
  useEntityEvents(['trades', 'accounts'])

  const { data: user, isLoading: userLoading } = useQuery({
    queryKey: ['user', userId],
    queryFn: () => api.getUser(userId!),
    enabled: !!userId,
  })

  const { data: accounts, isLoading: accountsLoading } = useQuery({
    queryKey: ['accounts', { userId }],
    queryFn: () => api.getUserAccounts(userId!),
    enabled: !!userId,
  })

  const account = accounts?.find((a) => a.id === accountIdNum)

  const { data: positions, isLoading: positionsLoading } = useQuery({
    queryKey: ['trades', 'positions', { userId, accountId }],
    queryFn: () => api.getAccountPositions(userId!, accountIdNum),
    enabled: !!userId && !!accountId,
  })

  const { data: trades, isLoading: tradesLoading } = useQuery({
    queryKey: ['trades', { userId, accountId }],
    queryFn: () => api.getAccountTrades(userId!, accountIdNum),
    enabled: !!userId && !!accountId,
  })

  usePageTitle(account?.label || `Account ${accountId}`)

  const positionColumns = useMemo<ColumnDef<PositionSummary, unknown>[]>(
    () => [
      { accessorKey: 'instrument_id', header: 'Instrument' },
      {
        accessorKey: 'instrument_name',
        header: 'Name',
        cell: ({ getValue }) => getValue<string | null>() || '-',
      },
      {
        accessorKey: 'quantity',
        header: 'Quantity',
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
      { accessorKey: 'currency', header: 'Currency' },
      { accessorKey: 'trade_count', header: 'Trades', meta: { numeric: true } },
    ],
    []
  )

  const tradeColumns = useMemo<ColumnDef<TradeSummary, unknown>[]>(
    () => [
      {
        accessorKey: 'date',
        header: 'Date',
        cell: ({ getValue }) =>
          new Date(getValue<string>()).toLocaleDateString(),
      },
      {
        accessorKey: 'instrument_id',
        header: 'Instrument',
        cell: ({ getValue }) => (
          <span className="ds-text--mono">{getValue<string>()}</span>
        ),
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
    ],
    []
  )

  const isLoading = userLoading || accountsLoading

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
          <h1 className="ds-page__title">{account?.label || `Account ${accountId}`}</h1>
          {account && <Badge variant="neutral">{account.currency}</Badge>}
        </div>
      </div>

      {account && (
        <Card header="Account Details">
          <div className="ds-kv-grid">
            <span className="ds-kv-grid__label">ID</span>
            <span className="ds-text--mono">{account.id}</span>
            <span className="ds-kv-grid__label">Label</span>
            <span>{account.label}</span>
            <span className="ds-kv-grid__label">Currency</span>
            <span>{account.currency}</span>
            <span className="ds-kv-grid__label">Positions</span>
            <span>{account.position_count}</span>
            <span className="ds-kv-grid__label">Trades</span>
            <span>{account.trade_count}</span>
            <span className="ds-kv-grid__label">Market Value</span>
            <span className="ds-text--mono">
              {account.market_value != null
                ? `${account.market_value.toLocaleString(undefined, { minimumFractionDigits: 0, maximumFractionDigits: 0 })} ${account.currency}`
                : '-'}
            </span>
          </div>
        </Card>
      )}

      <Card header="Positions" className="ds-mt-xl">
        {positionsLoading ? (
          <Spinner size="md" center />
        ) : positions && positions.length > 0 ? (
          <DataTable
            data={positions}
            columns={positionColumns}
            onRowClick={(row) =>
              navigate(`/users/${userId}/positions/${encodeURIComponent(row.instrument_id)}`)
            }
          />
        ) : (
          <p className="ds-text--secondary">No positions.</p>
        )}
      </Card>

      <Card header="Transactions" className="ds-mt-xl">
        {tradesLoading ? (
          <Spinner size="md" center />
        ) : trades && trades.length > 0 ? (
          <DataTable
            data={trades}
            columns={tradeColumns}
            onRowClick={(row) =>
              navigate(`/users/${userId}/positions/${encodeURIComponent(row.instrument_id)}`)
            }
          />
        ) : (
          <p className="ds-text--secondary">No transactions.</p>
        )}
      </Card>
    </div>
  )
}
