import { useQuery } from '@tanstack/react-query'
import type { ColumnDef } from '@tanstack/react-table'
import { useMemo } from 'react'
import { useNavigate } from 'react-router'
import { api } from '../api/client'
import type { Organization } from '../api/types'
import DataTable from '../components/DataTable'
import Spinner from '../components/Spinner'
import { usePageTitle } from '../hooks/usePageTitle'
import { useEntityEvents } from '../hooks/useEntityEvents'

export default function OrganizationsPage() {
  usePageTitle('Organizations')
  useEntityEvents(['organizations'])
  const navigate = useNavigate()
  const { data, isLoading, error } = useQuery({
    queryKey: ['organizations'],
    queryFn: () => api.getOrganizations(),
  })

  const columns = useMemo<ColumnDef<Organization, unknown>[]>(
    () => [
      {
        accessorKey: 'name',
        header: 'Name',
        cell: ({ getValue }) => getValue<string | null>() || '-',
      },
      {
        accessorKey: 'user_count',
        header: 'Users',
        meta: { numeric: true },
      },
      {
        accessorKey: 'created_at',
        header: 'Created',
        cell: ({ getValue }) =>
          new Date(getValue<string>()).toLocaleDateString(),
      },
    ],
    []
  )

  return (
    <div className="ds-page">
      <div className="ds-page__header">
        <h1 className="ds-page__title">Organizations</h1>
      </div>
      {error ? (
        <p className="ds-text--secondary">Failed to load organizations: {error.message}</p>
      ) : isLoading ? (
        <Spinner size="lg" center />
      ) : data ? (
        <DataTable data={data} columns={columns} onRowClick={(row) => navigate(`/organizations/${row.id}`)} />
      ) : null}
    </div>
  )
}
