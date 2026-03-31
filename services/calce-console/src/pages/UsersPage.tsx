import { useQuery } from '@tanstack/react-query'
import type { ColumnDef } from '@tanstack/react-table'
import { useMemo, useEffect } from 'react'
import { useNavigate, useSearchParams } from 'react-router'
import { api } from '../api/client'
import type { User } from '../api/types'
import { PAGE_SIZE } from '../constants'
import DataTable from '../components/DataTable'
import SearchInput from '../components/SearchInput'
import Pagination from '../components/Pagination'
import Spinner from '../components/Spinner'
import { usePageTitle } from '../hooks/usePageTitle'
import { useEntityEvents } from '../hooks/useEntityEvents'
import { usePaginatedSearch } from '../hooks/usePaginatedSearch'

export default function UsersPage() {
  usePageTitle('Users')
  useEntityEvents(['users'])
  const navigate = useNavigate()
  const [searchParams, setSearchParams] = useSearchParams()
  const organizationId = searchParams.get('organization_id') || undefined
  const { page, setPage, search, setSearch, debouncedSearch, offset, totalPages } =
    usePaginatedSearch(PAGE_SIZE)

  useEffect(() => {
    setPage(1)
  }, [organizationId, setPage])

  const { data: orgs } = useQuery({
    queryKey: ['organizations'],
    queryFn: () => api.getOrganizations(),
  })

  const { data, isLoading, error } = useQuery({
    queryKey: ['users', { page, search: debouncedSearch, pageSize: PAGE_SIZE, organizationId }],
    queryFn: () =>
      api.getUsers({
        offset,
        limit: PAGE_SIZE,
        search: debouncedSearch || undefined,
        organization_id: organizationId,
      }),
  })

  const pages = data ? totalPages(data.total) : 0

  function handleOrgChange(e: React.ChangeEvent<HTMLSelectElement>) {
    const value = e.target.value
    if (value) {
      setSearchParams({ organization_id: value })
    } else {
      setSearchParams({})
    }
  }

  const columns = useMemo<ColumnDef<User, unknown>[]>(
    () => [
      {
        accessorKey: 'name',
        header: 'Name',
        cell: ({ getValue }) => getValue<string | null>() || '-',
      },
      {
        accessorKey: 'email',
        header: 'Email',
        cell: ({ getValue }) => getValue<string | null>() || '-',
      },
      {
        accessorKey: 'organization_name',
        header: 'Organization',
        cell: ({ getValue }) => getValue<string | null>() || '-',
      },
      {
        accessorKey: 'account_count',
        header: 'Accounts',
        meta: { numeric: true },
      },
      {
        accessorKey: 'trade_count',
        header: 'Trades',
        meta: { numeric: true },
      },
    ],
    []
  )

  return (
    <div className="ds-page">
      <div className="ds-page__header">
        <h1 className="ds-page__title">Users</h1>
        <div className="ds-page__actions">
          <select
            className="ds-select"
            value={organizationId || ''}
            onChange={handleOrgChange}
          >
            <option value="">All organizations</option>
            {orgs
              ?.slice()
              .sort((a, b) => (a.name || a.id).localeCompare(b.name || b.id))
              .map((org) => (
                <option key={org.id} value={org.id}>
                  {org.name || org.id}
                </option>
              ))}
          </select>
          <SearchInput
            value={search}
            onChange={setSearch}
            placeholder="Search users..."
          />
        </div>
      </div>
      {error ? (
        <p className="ds-text--secondary">Failed to load users: {error.message}</p>
      ) : isLoading ? (
        <Spinner size="lg" center />
      ) : data ? (
        <>
          <DataTable data={data.items} columns={columns} onRowClick={(row) => navigate(`/users/${row.id}`)} />
          {pages > 1 && (
            <Pagination page={page} totalPages={pages} onPageChange={setPage} />
          )}
        </>
      ) : null}
    </div>
  )
}
