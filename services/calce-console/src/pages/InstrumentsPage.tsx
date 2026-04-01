import { useQuery } from '@tanstack/react-query'
import type { ColumnDef } from '@tanstack/react-table'
import { useMemo } from 'react'
import { useNavigate } from 'react-router'
import { api } from '../api/client'
import type { Instrument } from '../api/types'
import { PAGE_SIZE } from '../constants'
import DataTable from '../components/DataTable'
import SearchInput from '../components/SearchInput'
import Pagination from '../components/Pagination'
import Spinner from '../components/Spinner'
import { usePageTitle } from '../hooks/usePageTitle'
import { usePaginatedSearch } from '../hooks/usePaginatedSearch'
import { useEntityEvents } from '../hooks/useEntityEvents'
import Badge from '../components/Badge'

export default function InstrumentsPage() {
  usePageTitle('Instruments')
  useEntityEvents(['instruments'])
  const navigate = useNavigate()
  const { page, setPage, search, setSearch, debouncedSearch, offset, totalPages } =
    usePaginatedSearch(PAGE_SIZE)

  const { data, isLoading, error } = useQuery({
    queryKey: ['instruments', { page, search: debouncedSearch, pageSize: PAGE_SIZE }],
    queryFn: () =>
      api.getInstruments({
        offset,
        limit: PAGE_SIZE,
        search: debouncedSearch || undefined,
      }),
  })

  const pages = data ? totalPages(data.total) : 0

  const columns = useMemo<ColumnDef<Instrument, unknown>[]>(
    () => [
      {
        accessorKey: 'ticker',
        header: 'Ticker',
        cell: ({ getValue }) => (
          <span className="ds-text--mono">
            <Badge variant="neutral">{getValue<string>()}</Badge>
          </span>
        ),
      },
      {
        accessorKey: 'name',
        header: 'Name',
        cell: ({ getValue }) => getValue<string | null>() || '-',
      },
      {
        accessorKey: 'instrument_type',
        header: 'Type',
        cell: ({ getValue }) => <Badge>{getValue<string>()}</Badge>,
      },
      {
        accessorKey: 'currency',
        header: 'Currency',
      },
    ],
    []
  )

  return (
    <div className="ds-page">
      <div className="ds-page__header">
        <h1 className="ds-page__title">Instruments</h1>
        <div className="ds-page__actions">
          <SearchInput
            value={search}
            onChange={setSearch}
            placeholder="Search instruments..."
          />
        </div>
      </div>
      {error ? (
        <p className="ds-text--secondary">Failed to load instruments: {error.message}</p>
      ) : isLoading ? (
        <Spinner size="lg" center />
      ) : data ? (
        <>
          <DataTable
            data={data.items}
            columns={columns}
            onRowClick={(row) => navigate(`/instruments/${row.id}`)}
          />
          {pages > 1 && (
            <Pagination page={page} totalPages={pages} onPageChange={setPage} />
          )}
        </>
      ) : null}
    </div>
  )
}
