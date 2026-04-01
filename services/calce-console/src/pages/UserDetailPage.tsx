import { useParams, Link, useNavigate } from 'react-router'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import type { ColumnDef } from '@tanstack/react-table'
import { useMemo, useState } from 'react'
import { api } from '../api/client'
import type { AccountSummary, PositionSummary, TradeSummary } from '../api/types'
import { useAuth } from '../auth/AuthContext'
import { IconChevronLeft } from '../components/icons'
import Card from '../components/Card'
import Badge from '../components/Badge'
import Button from '../components/Button'
import Input from '../components/Input'
import DataTable from '../components/DataTable'
import Spinner from '../components/Spinner'
import { usePageTitle } from '../hooks/usePageTitle'
import { useEntityEvents } from '../hooks/useEntityEvents'

export default function UserDetailPage() {
  const { id } = useParams()
  const navigate = useNavigate()
  const { user: authUser } = useAuth()
  const queryClient = useQueryClient()
  const isAdmin = authUser?.role === 'admin'

  const [editing, setEditing] = useState(false)
  const [editName, setEditName] = useState('')
  const [editEmail, setEditEmail] = useState('')

  useEntityEvents(['users', 'trades'])

  const { data: user, isLoading: userLoading, error: userError } = useQuery({
    queryKey: ['user', id],
    queryFn: () => api.getUser(id!),
    enabled: !!id,
  })

  const updateMutation = useMutation({
    mutationFn: (body: { name?: string; email?: string }) =>
      api.updateUser(id!, body),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['user', id] })
      queryClient.invalidateQueries({ queryKey: ['users'] })
      setEditing(false)
    },
  })

  function startEditing() {
    setEditName(user?.name || '')
    setEditEmail(user?.email || '')
    setEditing(true)
  }

  function saveEdit() {
    const body: { name?: string; email?: string } = {}
    if (editName !== (user?.name || '')) body.name = editName
    if (isAdmin && editEmail !== (user?.email || '')) body.email = editEmail
    if (Object.keys(body).length > 0) {
      updateMutation.mutate(body)
    } else {
      setEditing(false)
    }
  }

  usePageTitle(user?.name || user?.email || id || 'User')

  const { data: accounts, isLoading: accountsLoading } = useQuery({
    queryKey: ['accounts', { userId: id }],
    queryFn: () => api.getUserAccounts(id!),
    enabled: !!id,
  })

  const { data: positions, isLoading: positionsLoading } = useQuery({
    queryKey: ['trades', 'positions', { userId: id }],
    queryFn: () => api.getUserPositions(id!),
    enabled: !!id,
  })

  const { data: trades, isLoading: tradesLoading } = useQuery({
    queryKey: ['trades', { userId: id }],
    queryFn: () => api.getUserTrades(id!),
    enabled: !!id,
  })

  const accountColumns = useMemo<ColumnDef<AccountSummary, unknown>[]>(
    () => [
      { accessorKey: 'label', header: 'Name' },
      { accessorKey: 'position_count', header: 'Positions', meta: { numeric: true } },
      { accessorKey: 'trade_count', header: 'Trades', meta: { numeric: true } },
      {
        accessorKey: 'market_value',
        header: 'Market Value',
        meta: { numeric: true },
        cell: ({ getValue, row }) => {
          const val = getValue<number | null>()
          if (val == null) return <span className="ds-text--secondary">-</span>
          return (
            <span className="ds-text--mono">
              {val.toLocaleString(undefined, {
                minimumFractionDigits: 0,
                maximumFractionDigits: 0,
              })}{' '}
              {row.original.currency}
            </span>
          )
        },
      },
    ],
    []
  )

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
        accessorKey: 'account_name',
        header: 'Account',
        cell: ({ getValue }) => getValue<string | null>() || '-',
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

  if (userLoading) {
    return (
      <div className="ds-page">
        <Spinner size="lg" center />
      </div>
    )
  }

  if (userError || !user) {
    return (
      <div className="ds-page">
        <Link to="/users" className="ds-back-link">
          <IconChevronLeft size={12} /> Back to Users
        </Link>
        <p className="ds-text--secondary">{userError?.message || 'User not found.'}</p>
      </div>
    )
  }

  return (
    <div className="ds-page">
      <Link to="/users" className="ds-back-link">
        <IconChevronLeft size={12} /> Back to Users
      </Link>
      <div className="ds-page__header">
        <div className="ds-page__actions">
          <h1 className="ds-page__title">{user.name || user.id}</h1>
          {user.organization_name && (
            <Badge variant="neutral">{user.organization_name}</Badge>
          )}
        </div>
      </div>

      <Card
        header="User Details"
        actions={
          !editing ? (
            <Button variant="outline" size="sm" onClick={startEditing}>
              Edit
            </Button>
          ) : undefined
        }
      >
        <div className="ds-kv-grid">
          <span className="ds-kv-grid__label">ID</span>
          <span className="ds-text--mono">{user.id}</span>
          <span className="ds-kv-grid__label">Name</span>
          {editing ? (
            <Input
              value={editName}
              onChange={(e) => setEditName(e.target.value)}
              placeholder="Name"
            />
          ) : (
            <span>{user.name || '-'}</span>
          )}
          <span className="ds-kv-grid__label">Email</span>
          {editing && isAdmin ? (
            <Input
              value={editEmail}
              onChange={(e) => setEditEmail(e.target.value)}
              placeholder="Email"
            />
          ) : (
            <span>{user.email || '-'}</span>
          )}
          <span className="ds-kv-grid__label">Organization</span>
          <span>{user.organization_name || '-'}</span>
          <span className="ds-kv-grid__label">Accounts</span>
          <span>{user.account_count}</span>
          <span className="ds-kv-grid__label">Trades</span>
          <span>{user.trade_count}</span>
        </div>
        {editing && (
          <div className="ds-flex ds-flex--center ds-flex--gap-2 ds-mt-md">
            <Button size="sm" onClick={saveEdit} disabled={updateMutation.isPending}>
              {updateMutation.isPending ? 'Saving...' : 'Save'}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setEditing(false)}
              disabled={updateMutation.isPending}
            >
              Cancel
            </Button>
            {updateMutation.isError && (
              <span className="ds-text--error">
                {updateMutation.error?.message || 'Failed to save'}
              </span>
            )}
          </div>
        )}
      </Card>

      <Card header="Accounts" className="ds-mt-xl">
        {accountsLoading ? (
          <Spinner size="md" center />
        ) : accounts && accounts.length > 0 ? (
          <DataTable
            data={accounts}
            columns={accountColumns}
            onRowClick={(row) => navigate(`/users/${id}/accounts/${row.id}`)}
          />
        ) : (
          <p className="ds-text--secondary">No accounts.</p>
        )}
      </Card>

      <Card header="Positions" className="ds-mt-xl">
        {positionsLoading ? (
          <Spinner size="md" center />
        ) : positions && positions.length > 0 ? (
          <DataTable
            data={positions}
            columns={positionColumns}
            onRowClick={(row) =>
              navigate(`/users/${id}/positions/${encodeURIComponent(row.instrument_id)}`)
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
              navigate(`/users/${id}/positions/${encodeURIComponent(row.instrument_id)}`)
            }
          />
        ) : (
          <p className="ds-text--secondary">No transactions.</p>
        )}
      </Card>
    </div>
  )
}
