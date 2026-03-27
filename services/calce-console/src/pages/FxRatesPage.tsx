import { useQuery } from '@tanstack/react-query'
import type { ColumnDef } from '@tanstack/react-table'
import { useMemo, useState, useEffect } from 'react'
import { useNavigate, useSearchParams } from 'react-router'
import { api } from '../api/client'
import type { FxRateSummary } from '../api/types'
import { PAGE_SIZE } from '../constants'
import CrossRateMatrix from '../components/CrossRateMatrix'
import DataTable from '../components/DataTable'
import SearchInput from '../components/SearchInput'
import Pagination from '../components/Pagination'
import Spinner from '../components/Spinner'
import Badge from '../components/Badge'
import { usePageTitle } from '../hooks/usePageTitle'

const DEBOUNCE_MS = 300

export default function FxRatesPage() {
  usePageTitle('FX Rates')
  const navigate = useNavigate()
  const [searchParams, setSearchParams] = useSearchParams()

  const [page, setPage] = useState(1)
  const [fromFilter, setFromFilter] = useState(searchParams.get('from') || '')
  const [toFilter, setToFilter] = useState(searchParams.get('to') || '')
  const [search, setSearch] = useState('')

  const [debouncedSearch, setDebouncedSearch] = useState('')
  const [debouncedFrom, setDebouncedFrom] = useState(fromFilter)
  const [debouncedTo, setDebouncedTo] = useState(toFilter)

  useEffect(() => {
    const timeout = setTimeout(() => {
      setDebouncedSearch(search)
      setDebouncedFrom(fromFilter)
      setDebouncedTo(toFilter)
      setPage(1)

      const params: Record<string, string> = {}
      if (fromFilter) params.from = fromFilter
      if (toFilter) params.to = toFilter
      setSearchParams(params, { replace: true })
    }, DEBOUNCE_MS)
    return () => clearTimeout(timeout)
  }, [search, fromFilter, toFilter, setSearchParams])

  const { data: allRates } = useQuery({
    queryKey: ['fx-rates-all'],
    queryFn: () => api.getFxRates({ limit: 200 }),
  })

  const offset = (page - 1) * PAGE_SIZE

  const { data, isLoading, error } = useQuery({
    queryKey: ['fx-rates', { page, search: debouncedSearch, from: debouncedFrom, to: debouncedTo }],
    queryFn: () =>
      api.getFxRates({
        offset,
        limit: PAGE_SIZE,
        search: debouncedSearch || undefined,
        from_currency: debouncedFrom || undefined,
        to_currency: debouncedTo || undefined,
      }),
  })

  const totalPages = data ? Math.ceil(data.total / PAGE_SIZE) : 0

  const columns = useMemo<ColumnDef<FxRateSummary, unknown>[]>(
    () => [
      {
        accessorKey: 'pair',
        header: 'Pair',
        cell: ({ getValue }) => (
          <span className="ds-text--mono">
            <Badge variant="neutral">{getValue<string>()}</Badge>
          </span>
        ),
      },
      {
        accessorKey: 'from_currency',
        header: 'From',
      },
      {
        accessorKey: 'to_currency',
        header: 'To',
      },
      {
        accessorKey: 'latest_rate',
        header: 'Latest Rate',
        meta: { numeric: true },
        cell: ({ getValue }) => {
          const v = getValue<number | null>()
          return v != null ? (
            <span className="ds-text--mono">{v.toFixed(4)}</span>
          ) : '-'
        },
      },
      {
        accessorKey: 'data_points',
        header: 'Data Points',
        meta: { numeric: true },
        cell: ({ getValue }) => getValue<number>().toLocaleString(),
      },
    ],
    []
  )

  return (
    <div className="ds-page">
      <div className="ds-page__header">
        <h1 className="ds-page__title">FX Rates</h1>
      </div>

      {allRates && <CrossRateMatrix rates={allRates.items} />}

      <div className="ds-page__header">
        <div className="ds-page__actions">
          <SearchInput
            value={fromFilter}
            onChange={setFromFilter}
            placeholder="From (e.g. USD)"
          />
          <SearchInput
            value={toFilter}
            onChange={setToFilter}
            placeholder="To (e.g. SEK)"
          />
          <SearchInput
            value={search}
            onChange={setSearch}
            placeholder="Search all..."
          />
        </div>
      </div>
      {error ? (
        <p className="ds-text--secondary">Failed to load FX rates: {error.message}</p>
      ) : isLoading ? (
        <Spinner size="lg" center />
      ) : data ? (
        <>
          <DataTable
            data={data.items}
            columns={columns}
            onRowClick={(row) =>
              navigate(`/fx-rates/${row.from_currency}/${row.to_currency}`)
            }
          />
          {totalPages > 1 && (
            <Pagination page={page} totalPages={totalPages} onPageChange={setPage} />
          )}
        </>
      ) : null}
    </div>
  )
}
